use crate::eth::EthHeader;
use crate::framebuffer::FrameBuffer;
use crate::originator::{Neighbor,Originator, OriginatorError::{ZeroTq, UnknownNeighbor}};
use crate::originatortable::OriginatorTable;
use crate::originatortable::OriginatorTableError::NotBestNextHop;
use crate::packets::{BATADV_DIRECTLINK, BatmanAdvOGMPacket, ETH_P_BATMAN, Packet, PacketError};
use crate::mac::Mac;
use crate::time::{Instant, ogm_send_time};
use crate::{FrameError, HardInterface, MeshError, consts::*};
use crate::hardif::LinkDriver;
use crate::meshctx::DropReason;
use crate::bitwindow::BitWindowResult::self;
use crate::consts::HOP_PENALTY;
use crate::packets::BATADV_PRIMARIES_FIRST_HOP;
use crate::packets::BATADV_NOT_BEST_NEXT_HOP;


// temp constants
pub const OGM_HEADROOM:usize =14;
pub const OGM_FRAME_LEN:usize =60;
pub const MIN_ETH_FRAME_SIZE:usize=60;

// move to better place (libs.rs or meshctx)


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OgmType {
    /// An OGM from another node.
    /// `single_hop` is true when the neighbor that sent it is its originator.
    Normal { ogm: BatmanAdvOGMPacket, single_hop: bool },
    /// A neighbor echoing one of our own OGMs straight back to us.
    Echo { seqno: u32 },
    /// Anything to drop.
    Invalid,
}


impl From<PacketError> for MeshError {
    fn from(e: PacketError) -> Self { MeshError::Packet(e) }
}
impl From<FrameError> for MeshError {
    fn from(e: FrameError) -> Self { MeshError::Frame(e) }
}


pub fn build_ogm_frame(
    ogm_header: BatmanAdvOGMPacket, 
    source_mac: Mac, 
    buffer: &mut FrameBuffer<OGM_FRAME_LEN, OGM_HEADROOM>
)->Result<(), MeshError>{
    buffer.reset_cursors();

    let buf: &mut [u8] = buffer.append(BatmanAdvOGMPacket::WIRE_LEN)?;
    ogm_header.write(buf)?;

    let eth_slot = buffer.prepend(ETH_HLEN)?;
    let eth_header: EthHeader = EthHeader::new(
        Mac::BROADCAST, 
        source_mac, 
        ETH_P_BATMAN
    );
    eth_header.write(eth_slot)?;

    buffer.append(MIN_ETH_FRAME_SIZE.saturating_sub(buffer.len()))?;
    Ok(())
}


pub fn send_own_ogm<D: LinkDriver>(
    hardif:&mut HardInterface<D>,
    orig_table:&mut OriginatorTable, 
    now:Instant,
    rand:u32
)->Instant{
    let seqno = hardif.next_ogm_seqno();
    let orig_mac = hardif.hardware_addr();
    let batman_header: BatmanAdvOGMPacket = BatmanAdvOGMPacket::new(orig_mac, seqno);

    let mut buffer = FrameBuffer::<OGM_FRAME_LEN, OGM_HEADROOM>::new();
    if build_ogm_frame(batman_header, orig_mac, &mut buffer).is_ok() {
        let _ = hardif.transmit(buffer.bytes());
    }

    orig_table.slide_echo_windows();

    return ogm_send_time(now, ORIG_INTER, rand);

}


pub fn classify_ogm(my_mac: Mac, neighbor_mac: Mac, payload: &[u8]) -> OgmType {
    let ogm = match BatmanAdvOGMPacket::parse(payload) {
        Ok(ogm) => ogm,
        Err(_e) => return OgmType::Invalid,
    };
 
    let orig = ogm.originator();
 
    if !orig.is_valid() {
        return OgmType::Invalid;
    }
    

    if orig == my_mac {
        if ogm.has_flag(BATADV_DIRECTLINK) {
            return OgmType::Echo{ seqno: ogm.seqno() };
        }
        return OgmType::Invalid;
    }
 
    if ogm.prev_sender() == my_mac {
        return OgmType::Invalid;
    }
 
    OgmType::Normal {
        ogm,
        single_hop: orig == neighbor_mac,
    }
}


pub fn process_ogm<D: LinkDriver>(
    hardif: &mut HardInterface<D>,
    orig_table: &mut OriginatorTable,
    neighbor_mac: Mac,
    payload: &[u8],
    now: Instant,
)->Result<(),DropReason> {
    let my_mac: Mac = hardif.hardware_addr();
 
    match classify_ogm(my_mac, neighbor_mac, payload) {
        OgmType::Invalid =>{ 
            return Err(DropReason::InvalidOgm);
        },
        OgmType::Echo { seqno } => {
            return handle_echo(hardif, orig_table, neighbor_mac, seqno, now);
        },
        OgmType::Normal { mut ogm, single_hop } => {
            let orig_mac = ogm.originator();
 
            // ensure originator exists in orig_table
            ensure_originator(orig_table, orig_mac, now)?;     
            ensure_originator(orig_table, neighbor_mac, now)?; 
 
            // get orig that exists for orig_mac
            let Some(orig) = orig_table.find_mac_mut(orig_mac) else { return Err(DropReason::UnknownOriginator) };
 
            // check if they exist in origs neighbor table,
            ensure_neighbor(orig,neighbor_mac,now)?; 

            // record that they are still in the mesh
            orig.touch(now);
 
            // try to update the sequnce number, retrun its status
            let seqno_status = orig.update_seqno(ogm.seqno(), neighbor_mac, ogm.ttl(), now);
            
            // drop if protected
            if seqno_status == BitWindowResult::Protected{
                return Err(DropReason::Protected);
            }
 
            // drop if 0 in quality
            if ogm.tq() == 0{
                return Err(DropReason::Originator(ZeroTq));
            }

            // drop if not best next hop
            if ogm.has_flag(BATADV_NOT_BEST_NEXT_HOP){
                return Err(DropReason::OriginatorTable(NotBestNextHop));
            }
 
            // Is this neighbor our current router towards orig?
            let is_router = orig.router_id() == Some(neighbor_mac);
 
            // Is the link through it still alive?
            let router_alive = match orig.find_neighbor(neighbor_mac) {
                Some(neighbor) => neighbor.tq_avg() > 0,
                Option::None => false,
            };
 
            let from_best_next_hop = is_router && router_alive;
 
            let Some(neigh_orig) = orig_table.find_mac(neighbor_mac) else { 
                return Err(DropReason::Originator(UnknownNeighbor)) 
            };
 
            // check if neighbor is known
            if !single_hop && (!neigh_orig.router_id().is_some()){
                return Err(DropReason::Originator(UnknownNeighbor));
            }
 
            // echo count and recive count
            let echo_count = neigh_orig.echo();
            let nigh = match neigh_orig.find_neighbor(neighbor_mac) {
                Some(nigh) => nigh,
                Option::None => {
                    return Err(DropReason::Originator(UnknownNeighbor));
                }
            };
            let recieve_count = nigh.real_bits_count();
 
            // calculate transmit quality
            let tq = calc_tq(ogm.tq(), echo_count, recieve_count);
 
            let Some(orig) = orig_table.find_mac_mut(orig_mac) else { return Err(DropReason::UnknownOriginator) };
            
            // if sequnce number is new or it was from itself
            if tq != 0 && (seqno_status == BitWindowResult::New ||
                (ogm.seqno() == orig.last_seqno() && ogm.ttl() >= orig.last_ttl().saturating_sub(3))) {
                // if new neighbor, send tq 0 to every other nighbor except for sender
                if seqno_status==BitWindowResult::New{
                    orig.update_new_ogm_tq(neighbor_mac);
                }
 
                // record the calculated tq to sender neighbor and extend its time to live
                if let Some(n) = orig.find_neighbor_mut(neighbor_mac) { n.record_transmit_quality(tq); n.set_last_ttl(ogm.ttl()); }
 
                // select routeing path
                if orig_table.select_router(orig_mac, neighbor_mac){
                    if let Some(orig) = orig_table.find_mac_mut(orig_mac) { orig.set_router(neighbor_mac); }
                }
 
            }
 
            // if it was a single hop or a bidirectional message, forward it
            if single_hop || (tq !=0 && seqno_status != BitWindowResult::NeighDup
                    && from_best_next_hop){ 
                if !ogm.decrement_ttl(){
                    return Ok(());
                }

                ogm.set_tq(tq);
                ogm.apply_hop_penalty(HOP_PENALTY);
                ogm.set_prev_sender(neighbor_mac);
                ogm.clear_flag(BATADV_PRIMARIES_FIRST_HOP);
                ogm.clear_flag(BATADV_DIRECTLINK);
                if single_hop {
                    ogm.set_flag(BATADV_DIRECTLINK);

                    // mark echoes that did not come via our best path
                    if !from_best_next_hop {
                        ogm.set_flag(BATADV_NOT_BEST_NEXT_HOP);
                    }
                }

 
                
                let my_mac = hardif.hardware_addr();
                let mut buffer = FrameBuffer::<OGM_FRAME_LEN, OGM_HEADROOM>::new();
                if build_ogm_frame(ogm, my_mac, &mut buffer).is_ok() {
                    let _ = hardif.transmit(buffer.bytes());
                }
            }
 
            return Ok(())
        
        }
    }
}


fn calc_tq(tq_in: u8, echo_count: u8, receive_count: u8) -> u8 {
    const MAX: u32 = TQ_MAX as u32; // 100 %
    const WINDOW: u32 = WINDOW_SIZE as u32;
 
    let receive_count = (receive_count as u32).min(WINDOW);
    // more echoes than received OGMs would give a TQ_local above 100 %.
    let echo_count = (echo_count as u32).min(receive_count);
 
    if receive_count == 0 || echo_count == 0 {
        return 0;
    }
 
    // RQ and EQ as percentages, where 100 % = MAX.
    let rq = MAX * receive_count / WINDOW;
    let eq = MAX * echo_count / WINDOW;
 
    // TQ_local = EQ / RQ
    let tq_local = MAX * eq / rq;
 
    // (100% - RQ)^3 as a percentage. Each multiply by a percentage is
    // followed by a divide by MAX, so the value stays in 0..=MAX.
    let rq_inv = MAX - rq;
    let rq_inv_cubed = rq_inv * rq_inv / MAX * rq_inv / MAX;
 
    // f_asym = 100% - (100% - RQ)^3
    let f_asym = MAX - rq_inv_cubed;
 
    // TQ_out = TQ_in * TQ_local * f_asym, again dividing by MAX^2 so the
    // product of three percentages is a percentage.
    let tq_out: u32 = tq_in as u32 * tq_local * f_asym / (MAX * MAX);
    tq_out as u8
}



fn ensure_originator(
    orig_table: &mut OriginatorTable,
    mac: Mac,
    now: Instant,
) -> Result<(), DropReason> {
    if orig_table.find_mac(mac).is_some() {
        return Ok(());
    }
    match orig_table.add(Originator::new(mac, now), now) {
        Ok(_) => Ok(()),
        Err(e) => Err(DropReason::OriginatorTable(e)),
    }
}

fn ensure_neighbor(
    orig: &mut Originator,
    mac: Mac,
    now: Instant,
) -> Result<(), DropReason> {
    if orig.find_neighbor(mac).is_some() {
        match orig.find_neighbor_mut(mac){
            Some(neigh) => neigh.touch(now),
            Option::None=>{},
        };
        
        return Ok(());
    }
    match orig.add_neighbor(Neighbor::new(mac, now), now) {
        Ok(_) => Ok(()),
        Err(e) => Err(DropReason::Originator(e)),
    }
}


fn handle_echo<D: LinkDriver>(
    hardif: &HardInterface<D>,
    orig_table: &mut OriginatorTable,
    neighbor_mac: Mac,
    echoed_seqno: u32,
    now: Instant,
) -> Result<(), DropReason> {
    let last_sent = hardif.ogm_seqno().wrapping_sub(1);

    let bit = last_sent.wrapping_sub(echoed_seqno);
    if bit >= WINDOW_SIZE as u32 {
        return Ok(());
    }

    ensure_originator(orig_table, neighbor_mac, now)?;

    let Some(orig) = orig_table.find_mac_mut(neighbor_mac) else {
        return Err(DropReason::UnknownOriginator);
    };
    orig.record_echo(bit as i32);
    orig.touch(now);

    Ok(())
}
