use crate::eth::EthHeader;
use crate::framebuffer::FrameBuffer;
use crate::originatortable::OriginatorTable;
use crate::packets::{BatmanAdvOGMPacket, ETH_P_BATMAN, Packet, PacketError};
use crate::mac::Mac;
use crate::time::{Instant, ogm_send_time};
use crate::{FrameError, HardInterface, consts::*};
use crate::hardif::LinkDriver;

// temp constants
pub const OGM_HEADROOM:usize =14;
pub const OGM_FRAME_LEN:usize =60;
pub const MIN_ETH_FRAME_SIZE:usize=60;

// move to better place (libs.rs or meshctx)
pub enum MeshError {
    Frame(FrameError),
    Packet(PacketError),
    //Interface(InterfaceError)

}

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
        Err(e) => return OgmType::Invalid,
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


pub fn dispatch_ogm<D: LinkDriver>(
    hardif: &mut HardInterface<D>,
    orig_table: &mut OriginatorTable,
    neighbor_mac: Mac,
    payload: &[u8],
    now: Instant,
) {
    let my_mac: Mac = hardif.hardware_addr();
 
    match classify_ogm(my_mac, neighbor_mac, payload) {
        OgmType::Invalid => hardif.record_rx_dropped(),
 
        OgmType::Echo { seqno } => {
            handle_echo(hardif, orig_table, neighbor_mac, seqno, now);
        }
 
        OgmType::Normal { ogm: _, single_hop: _ } => {
            // TODO: bookkeeping, TQ calculation, route update, forwarding.



        }
    }
}

fn handle_echo<D: LinkDriver>(
    hardif: &HardInterface<D>,
    orig_table: &mut OriginatorTable,
    neighbor_mac: Mac,
    echoed_seqno: u32,
    now: Instant,
) {

    let last_sent = hardif.ogm_seqno().wrapping_sub(1);

    let bit = last_sent.wrapping_sub(echoed_seqno);
    if bit >= WINDOW_SIZE as u32 {
        return;
    }
 
    if orig_table.find_mac(neighbor_mac).is_none()
        && !orig_table.add(Originator::new(neighbor_mac, now))
    {
        return;
    }
 
    if let Some(orig) = orig_table.find_mac_mut(neighbor_mac) {
        orig.record_echo(bit as i32);
        orig.touch(now);
    }
}

