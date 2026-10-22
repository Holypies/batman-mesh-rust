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
use crate::bitwindow::BitWindowResult;
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
                //TODO: Take a deeper look at this, seems likely to have a bug
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



#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardif::InterfaceError;
    use crate::originator::OriginatorError;
    use crate::originatortable::OriginatorTableError;
    use crate::packets::{BATADV_BCAST, BATADV_TTL};

    // --- test helpers ---

    const NOW: Instant = Instant::from_ticks(0);
    const OGM_LEN: usize = BatmanAdvOGMPacket::WIRE_LEN;

    // node ids for mac()
    const MY: u8 = 1; // this node
    const N: u8 = 2;  // direct neighbor
    const O: u8 = 3;  // remote originator, reached via N
    const N2: u8 = 4; // second neighbor


    fn mac(n: u8) -> Mac {
        Mac::from_octets([0x02, 0x00, 0x00, 0x00, 0x00, n])
    }

    struct MockDriver {
        mac: Mac,
        link: bool,
        tx_calls: u32,
        last_tx: [u8; OGM_FRAME_LEN],
        last_tx_len: usize,
    }

    impl LinkDriver for MockDriver {
        fn hardware_address(&self) -> Mac {
            self.mac
        }
        fn payload_size(&self) -> usize {
            1500
        }
        fn link_state(&self) -> bool {
            self.link
        }
        fn transmit(&mut self, buf: &[u8]) -> Result<(), InterfaceError> {
            self.tx_calls += 1;
            self.last_tx_len = buf.len();
            self.last_tx[..buf.len()].copy_from_slice(buf);
            Ok(())
        }
    }

    // active interface, link up, owned by mac(MY)
    fn up_if() -> HardInterface<MockDriver> {
        let drv = MockDriver {
            mac: mac(MY),
            link: true,
            tx_calls: 0,
            last_tx: [0u8; OGM_FRAME_LEN],
            last_tx_len: 0,
        };
        let mut hardif = HardInterface::new(drv, 0).expect("unicast MAC must construct");
        hardif.set_active(true);
        hardif
    }

    // table filled to capacity with unrelated originators
    fn full_table() -> OriginatorTable {
        let mut table = OriginatorTable::new();
        for i in 0..MAX_NEIGHBORS {
            table.add(Originator::new(mac(100 + i as u8), NOW), NOW).unwrap();
        }
        table
    }

    // direct neighbor `n` in table: routed via itself, `rx` of its OGMs received,
    // `echo` of ours echoed back, link tq sample `tq`
    fn add_direct(table: &mut OriginatorTable, n: Mac, rx: u8, echo: u8, tq: u8) {
        let mut neigh = Neighbor::new(n, NOW);
        for _ in 0..rx {
            neigh.slide_window(1, true);
        }
        neigh.record_transmit_quality(tq);
        let mut orig = Originator::new(n, NOW);
        orig.add_neighbor(neigh, NOW).unwrap();
        orig.set_router(n);
        for i in 0..echo {
            orig.record_echo(i as i32);
        }
        table.add(orig, NOW).unwrap();
    }

    // OGM header as received; ttl in 1..=BATADV_TTL
    fn ogm(orig: Mac, seqno: u32, ttl: u8, tq: u8, flags: u8, prev: Mac) -> BatmanAdvOGMPacket {
        let mut p = BatmanAdvOGMPacket::new(orig, seqno);
        while p.ttl() > ttl && p.decrement_ttl() {}
        p.clear_flags();
        p.set_flag(flags);
        p.set_tq(tq);
        p.set_prev_sender(prev);
        p
    }

    // wire bytes of `p`
    fn wire(p: BatmanAdvOGMPacket) -> [u8; OGM_LEN] {
        let mut b = [0u8; OGM_LEN];
        p.write(&mut b).unwrap();
        b
    }

    // OGM header of the last frame the mock sent
    fn sent_ogm(hardif: &mut HardInterface<MockDriver>) -> BatmanAdvOGMPacket {
        BatmanAdvOGMPacket::parse(&hardif.driver_mut().last_tx[ETH_HLEN..]).unwrap()
    }

    // eth header every OGM we send must carry
    fn eth_from_me() -> [u8; ETH_HLEN] {
        let mut e = [0u8; ETH_HLEN];
        assert!(EthHeader::new(Mac::BROADCAST, mac(MY), ETH_P_BATMAN).write(&mut e).is_ok());
        e
    }
    
    #[test]
    fn build_ogm_frame_test(){
        let hdr = BatmanAdvOGMPacket::new(mac(MY), 0x0102_0304);
        let mut buffer = FrameBuffer::<OGM_FRAME_LEN, OGM_HEADROOM>::new();
        assert!(build_ogm_frame(hdr, mac(MY), &mut buffer).is_ok());

        // minimum-size frame
        assert_eq!(buffer.len(), MIN_ETH_FRAME_SIZE);
        let out = buffer.bytes();

        // eth: broadcast from us, batman ethertype
        assert_eq!(&out[..ETH_HLEN], &eth_from_me()[..]);
        assert_eq!(&out[12..14], &[0x43u8, 0x05]);

        // OGM header round-trips
        assert_eq!(BatmanAdvOGMPacket::parse(&out[ETH_HLEN..]), Ok(hdr));

        // padding zero
        assert!(out[ETH_HLEN + OGM_LEN..].iter().all(|&b| b == 0));
    }

    #[test]
    fn build_ogm_frame_reuse_test(){
        let hdr = BatmanAdvOGMPacket::new(mac(MY), 7);
        let mut buffer = FrameBuffer::<OGM_FRAME_LEN, OGM_HEADROOM>::new();

        // buffer previously used: stale bytes everywhere
        buffer.rx_space().fill(0xEE);
        assert!(build_ogm_frame(hdr, mac(MY), &mut buffer).is_ok());
        let out = buffer.bytes();
        assert_eq!(BatmanAdvOGMPacket::parse(&out[ETH_HLEN..]), Ok(hdr));

        // padding must not leak old data onto the wire
        assert!(out[ETH_HLEN + OGM_LEN..].iter().all(|&b| b == 0), "stale bytes in padding");

    }

    #[test]
    fn send_own_ogm_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 1, 1, 200); // N echoed one OGM (slot 0)
        let now = Instant::from_ticks(5_000);
        let start = hardif.ogm_seqno();

        // sends OGM with current seqno, returns jittered next send time
        let next = send_own_ogm(&mut hardif, &mut table, now, 1234);
        assert_eq!(next, ogm_send_time(now, ORIG_INTER, 1234));
        assert_eq!(hardif.ogm_seqno(), start.wrapping_add(1));

        // frame: broadcast from us, fresh OGM, min size, zero padded
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, MIN_ETH_FRAME_SIZE);
        assert_eq!(&hardif.driver_mut().last_tx[..ETH_HLEN], &eth_from_me()[..]);
        assert_eq!(sent_ogm(&mut hardif), BatmanAdvOGMPacket::new(mac(MY), start));
        assert!(hardif.driver_mut().last_tx[ETH_HLEN + OGM_LEN..MIN_ETH_FRAME_SIZE]
            .iter()
            .all(|&b| b == 0)
        );

        // echo windows slid: old echo kept, slot 0 free again
        let n = table.find_mac_mut(mac(N)).unwrap();
        assert_eq!(n.echo(), 1);
        n.record_echo(0);
        assert_eq!(n.echo(), 2);

        // next call uses next seqno
        send_own_ogm(&mut hardif, &mut table, now, 0);
        assert_eq!(sent_ogm(&mut hardif).seqno(), start.wrapping_add(1));
    }

    #[test]
    fn send_own_ogm_interface_down_test(){
        let mut hardif = up_if();
        hardif.set_active(false);
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 1, 1, 200);
        let start = hardif.ogm_seqno();

        // nothing sent, but seqno consumed, windows slid, schedule kept
        let next = send_own_ogm(&mut hardif, &mut table, NOW, 7);
        assert_eq!(next, ogm_send_time(NOW, ORIG_INTER, 7));
        assert_eq!(hardif.driver_mut().tx_calls, 0);
        assert_eq!(hardif.tx_dropped(), 1);
        assert_eq!(hardif.ogm_seqno(), start.wrapping_add(1));

        let n = table.find_mac_mut(mac(N)).unwrap();
        n.record_echo(0);
        assert_eq!(n.echo(), 2);

    }

    #[test]
    fn classify_ogm_test(){
        let my = mac(MY);

        // originated by the sending neighbor: single hop
        let p = ogm(mac(N), 7, BATADV_TTL, 255, BATADV_PRIMARIES_FIRST_HOP, mac(N));
        assert_eq!(
            classify_ogm(my, mac(N), &wire(p)),
            OgmType::Normal { ogm: p, single_hop: true }
        );

        // relayed by N: multi hop
        let r = ogm(mac(O), 7, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(
            classify_ogm(my, mac(N), &wire(r)),
            OgmType::Normal { ogm: r, single_hop: false }
        );

        // trailing bytes after header ignored
        let mut long = [0xEEu8; OGM_LEN + 8];
        long[..OGM_LEN].copy_from_slice(&wire(r));
        assert_eq!(
            classify_ogm(my, mac(N), &long),
            OgmType::Normal { ogm: r, single_hop: false }
        );

        // our OGM echoed back by N (DIRECTLINK, prev_sender = us): Echo
        let echo = ogm(my, 42, BATADV_TTL - 1, 255, BATADV_DIRECTLINK, my);
        assert_eq!(classify_ogm(my, mac(N), &wire(echo)), OgmType::Echo { seqno: 42 });

        // our OGM relayed further, no DIRECTLINK: Invalid
        let relayed = ogm(my, 42, BATADV_TTL - 2, 200, 0, mac(N));
        assert_eq!(classify_ogm(my, mac(N), &wire(relayed)), OgmType::Invalid);

        // someone else's OGM we already rebroadcast (prev_sender = us): Invalid
        let back = ogm(mac(O), 7, BATADV_TTL - 2, 200, 0, my);
        assert_eq!(classify_ogm(my, mac(N), &wire(back)), OgmType::Invalid);

        // invalid originator: zero, broadcast, multicast
        for &bad in [Mac::ZERO, Mac::BROADCAST, Mac::STP].iter() {
            let bad_p = ogm(bad, 7, BATADV_TTL, 255, 0, bad);
            assert_eq!(classify_ogm(my, mac(N), &wire(bad_p)), OgmType::Invalid, "orig {:?}", bad);
        }

        // unparsable: truncated, wrong type, bad version, nonzero tvlv_len
        let raw = wire(p);
        assert_eq!(classify_ogm(my, mac(N), &raw[..OGM_LEN - 1]), OgmType::Invalid);
        let mut wrong = raw;
        wrong[0] = BATADV_BCAST;
        assert_eq!(classify_ogm(my, mac(N), &wrong), OgmType::Invalid);
        let mut bad_ver = raw;
        bad_ver[1] = 14;
        assert_eq!(classify_ogm(my, mac(N), &bad_ver), OgmType::Invalid);
        let mut tvlv = raw;
        tvlv[OGM_LEN - 1] = 1;
        assert_eq!(classify_ogm(my, mac(N), &tvlv), OgmType::Invalid);

    }

    #[test]
    fn calc_tq_test(){
        let w = WINDOW_SIZE as u8;

        // no data: 0
        assert_eq!(calc_tq(255, 0, 0), 0);
        assert_eq!(calc_tq(255, 0, w), 0); // never echoed
        assert_eq!(calc_tq(255, w, 0), 0); // never received
        assert_eq!(calc_tq(0, w, w), 0);   // incoming tq 0

        // perfect link (full window both ways): incoming tq passes through
        for &tq in [1u8, 100, 200, 255].iter() {
            assert_eq!(calc_tq(tq, w, w), tq, "tq_in {}", tq);
        }

        // half our OGMs echoed on a full receive window: half
        assert_eq!(calc_tq(255, w / 2, w), 127);

        // more echoes than receives clamped (local tq never above 100 %)
        assert_eq!(calc_tq(200, w, w / 2), calc_tq(200, w / 2, w / 2));

        // counts above window clamped
        assert_eq!(calc_tq(200, 255, 255), calc_tq(200, w, w));

        // asymmetry penalty: same ratio, fewer receives -> lower
        assert!(calc_tq(255, w / 2, w / 2) < calc_tq(255, w, w));

        // monotonic in echoes (receives fixed) and in symmetric link quality
        let mut prev = 0;
        for e in 1..=w {
            let t = calc_tq(255, e, w);
            assert!(t >= prev, "echo {}: {} < {}", e, t, prev);
            prev = t;
        }
        let mut prev = 0;
        for r in 1..=w {
            let t = calc_tq(255, r, r);
            assert!(t >= prev, "rx {}: {} < {}", r, t, prev);
            prev = t;
        }

        // full input range: never above incoming tq
        for r in 0..=u8::MAX {
            for e in 0..=u8::MAX {
                assert!(calc_tq(200, e, r) <= 200, "echo {} rx {}", e, r);
            }
        }
    }

    #[test]
    fn ensure_originator_test(){
        let mut table = OriginatorTable::new();
        let t1 = Instant::from_ticks(1_000);

        // new: added, stamped with now
        assert_eq!(ensure_originator(&mut table, mac(N), NOW), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().last_seen(), NOW);

        // existing: Ok, not touched
        assert_eq!(ensure_originator(&mut table, mac(N), t1), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().last_seen(), NOW);

        // not added twice
        assert!(table.remove_mac(mac(N)).is_some());
        assert!(table.find_mac(mac(N)).is_none());

        // full table: error for new entry, existing still fine
        let mut full = full_table();
        assert_eq!(
            ensure_originator(&mut full, mac(N), NOW),
            Err(DropReason::OriginatorTable(OriginatorTableError::TableFull))
        );
        assert_eq!(ensure_originator(&mut full, mac(100), NOW), Ok(()));
    }
        
    #[test]
    fn ensure_neighbor_test(){
        let t1 = Instant::from_ticks(1_000);
        let mut orig = Originator::new(mac(O), NOW);

        // new: added, stamped with now
        assert_eq!(ensure_neighbor(&mut orig, mac(N), NOW), Ok(()));
        assert_eq!(orig.neighbors().len(), 1);
        assert_eq!(orig.find_neighbor(mac(N)).unwrap().last_seen(), NOW);

        // existing: touched, not duplicated
        assert_eq!(ensure_neighbor(&mut orig, mac(N), t1), Ok(()));
        assert_eq!(orig.neighbors().len(), 1);
        assert_eq!(orig.find_neighbor(mac(N)).unwrap().last_seen(), t1);

        // full list: error for new neighbor, existing still touched
        let mut full = Originator::new(mac(O), NOW);
        for i in 0..MAX_NEIGHBORS {
            full.add_neighbor(Neighbor::new(mac(100 + i as u8), NOW), NOW).unwrap();
        }
        assert_eq!(
            ensure_neighbor(&mut full, mac(N), NOW),
            Err(DropReason::Originator(OriginatorError::NeighborListFull))
        );
        assert_eq!(ensure_neighbor(&mut full, mac(100), t1), Ok(()));
        assert_eq!(full.find_neighbor(mac(100)).unwrap().last_seen(), t1);

    }

    #[test]
    fn handle_echo_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        for _ in 0..5 {
            hardif.next_ogm_seqno(); // sent seqnos 0..=4
        }
        let t = Instant::from_ticks(1_000);

        // echo of latest OGM: neighbor entry created, slot 0 marked, touched
        assert_eq!(handle_echo(&hardif, &mut table, mac(N), 4, t), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().echo(), 1);
        assert_eq!(table.find_mac(mac(N)).unwrap().last_seen(), t);

        // same echo again: not double counted
        assert_eq!(handle_echo(&hardif, &mut table, mac(N), 4, t), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().echo(), 1);

        // older OGM inside window: counted
        assert_eq!(handle_echo(&hardif, &mut table, mac(N), 2, t), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().echo(), 2);

        // oldest slot in window (seqno wraps below 0): counted
        let oldest = 4u32.wrapping_sub(WINDOW_SIZE as u32 - 1);
        assert_eq!(handle_echo(&hardif, &mut table, mac(N), oldest, t), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().echo(), 3);

        // just outside window: ignored, unknown neighbor not created
        let stale = 4u32.wrapping_sub(WINDOW_SIZE as u32);
        assert_eq!(handle_echo(&hardif, &mut table, mac(N2), stale, t), Ok(()));
        assert!(table.find_mac(mac(N2)).is_none());

        // future seqno (never sent): ignored
        assert_eq!(handle_echo(&hardif, &mut table, mac(N2), 5, t), Ok(()));
        assert!(table.find_mac(mac(N2)).is_none());

        // echoes kept per neighbor
        assert_eq!(handle_echo(&hardif, &mut table, mac(N2), 4, t), Ok(()));
        assert_eq!(table.find_mac(mac(N2)).unwrap().echo(), 1);
        assert_eq!(table.find_mac(mac(N)).unwrap().echo(), 3);

        // full table, new neighbor: error
        let mut full = full_table();
        assert_eq!(
            handle_echo(&hardif, &mut full, mac(N), 4, t),
            Err(DropReason::OriginatorTable(OriginatorTableError::TableFull))
        );
    }

    #[test]
    fn process_ogm_invalid_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();

        // garbage / truncated
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &[0u8; 10], NOW),
            Err(DropReason::InvalidOgm)
        );

        // our own OGM relayed without DIRECTLINK
        let own = ogm(mac(MY), 1, BATADV_TTL - 2, 200, 0, mac(N));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(own), NOW),
            Err(DropReason::InvalidOgm)
        );

        // our own rebroadcast coming back (prev_sender = us)
        let back = ogm(mac(O), 1, BATADV_TTL - 2, 200, 0, mac(MY));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(back), NOW),
            Err(DropReason::InvalidOgm)
        );

        // invalid originator
        for &bad in [Mac::ZERO, Mac::BROADCAST, Mac::STP].iter() {
            let p = ogm(bad, 1, BATADV_TTL, 255, 0, bad);
            assert_eq!(
                process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW),
                Err(DropReason::InvalidOgm),
                "orig {:?}", bad
            );
        }

        // nothing learned, nothing sent
        assert!(table.find_mac(mac(N)).is_none());
        assert!(table.find_mac(mac(O)).is_none());
        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_ogm_echo_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        for _ in 0..3 {
            hardif.next_ogm_seqno(); // sent seqnos 0..=2
        }
        let t = Instant::from_ticks(1_000);

        // N echoes our latest OGM: recorded on N's entry, not forwarded
        let echo = ogm(mac(MY), 2, BATADV_TTL - 1, 255, BATADV_DIRECTLINK, mac(MY));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(echo), t), Ok(()));
        assert_eq!(table.find_mac(mac(N)).unwrap().echo(), 1);
        assert_eq!(table.find_mac(mac(N)).unwrap().last_seen(), t);
        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_ogm_single_hop_first_contact_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        let t = Instant::from_ticks(1_000);

        // first OGM from unknown direct neighbor N
        let p = ogm(mac(N), 10, BATADV_TTL, 255, BATADV_PRIMARIES_FIRST_HOP, mac(N));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), t), Ok(()));

        // N learned as originator + neighbor of itself; no echoes yet -> tq 0 -> no route
        let n = table.find_mac(mac(N)).unwrap();
        assert_eq!(n.last_seen(), t);
        assert_eq!(n.find_neighbor(mac(N)).unwrap().last_seen(), t);
        assert_eq!(n.last_seqno(), 10);
        assert_eq!(n.router_id(), None);

        // still rebroadcast for link detection: DIRECTLINK + NOT_BEST, tq 0
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(&hardif.driver_mut().last_tx[..ETH_HLEN], &eth_from_me()[..]);
        let fwd = sent_ogm(&mut hardif);
        assert_eq!(fwd.originator(), mac(N));
        assert_eq!(fwd.seqno(), 10);
        assert_eq!(fwd.ttl(), BATADV_TTL - 1);
        assert_eq!(fwd.tq(), 0);
        assert_eq!(fwd.prev_sender(), mac(N)); 
        assert_eq!(fwd.flags(), BATADV_DIRECTLINK | BATADV_NOT_BEST_NEXT_HOP);

    }

    #[test]
    fn process_ogm_single_hop_bidirectional_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);

        let p = ogm(mac(N), 1, BATADV_TTL, 255, BATADV_PRIMARIES_FIRST_HOP, mac(N));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW), Ok(()));

        // this OGM is the 11th received from N
        let tq = calc_tq(255, 8, 11);
        assert!(tq > 0);

        // route kept, new tq sample + ttl recorded on the link
        let n = table.find_mac(mac(N)).unwrap();
        assert_eq!(n.router_id(), Some(mac(N)));
        let link = n.find_neighbor(mac(N)).unwrap();
        assert_eq!(link.tq().values()[1], tq);
        assert_eq!(link.last_ttl(), BATADV_TTL);

        // forwarded from best next hop: DIRECTLINK only, penalised tq
        let mut expected = p;
        expected.decrement_ttl();
        expected.set_tq(tq);
        expected.apply_hop_penalty(HOP_PENALTY);
        expected.set_prev_sender(mac(N));
        expected.clear_flags();
        expected.set_flag(BATADV_DIRECTLINK);
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(sent_ogm(&mut hardif), expected);
    }

    #[test]
    fn process_ogm_multi_hop_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);
        let tq = calc_tq(200, 8, 10);
        assert!(tq > 0);

        // first OGM from O via N: route learned, not forwarded (N not router yet)
        let p1 = ogm(mac(O), 1, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p1), NOW), Ok(()));
        let o = table.find_mac(mac(O)).unwrap();
        assert_eq!(o.router_id(), Some(mac(N)));
        assert_eq!(o.find_neighbor(mac(N)).unwrap().tq_avg(), tq);
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // next OGM via router N: forwarded, DIRECTLINK cleared, prev_sender = N
        let p2 = ogm(mac(O), 2, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p2), NOW), Ok(()));
        let mut expected = p2;
        expected.decrement_ttl();
        expected.set_tq(tq);
        expected.apply_hop_penalty(HOP_PENALTY);
        expected.set_prev_sender(mac(N));
        expected.clear_flags();
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(sent_ogm(&mut hardif), expected);

    }

    #[test]
    fn process_ogm_unknown_neighbor_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();

        // multi-hop OGM via N before any bidirectional link to N exists
        let p = ogm(mac(O), 1, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW),
            Err(DropReason::Originator(OriginatorError::UnknownNeighbor))
        );
        assert_eq!(table.find_mac(mac(O)).unwrap().router_id(), None);
        assert_eq!(hardif.driver_mut().tx_calls, 0);
    }

    #[test]
    fn process_ogm_drop_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);

        // tq 0
        let zero = ogm(mac(O), 1, BATADV_TTL - 1, 0, BATADV_DIRECTLINK, mac(O));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(zero), NOW),
            Err(DropReason::Originator(OriginatorError::ZeroTq))
        );

        // relay marked it NOT_BEST_NEXT_HOP
        let nb = ogm(mac(O), 2, BATADV_TTL - 1, 200, BATADV_DIRECTLINK | BATADV_NOT_BEST_NEXT_HOP, mac(O));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(nb), NOW),
            Err(DropReason::OriginatorTable(OriginatorTableError::NotBestNextHop))
        );

        // neither drop touched routing or sent anything
        assert_eq!(table.find_mac(mac(O)).unwrap().router_id(), None);
        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_ogm_duplicate_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);

        // route to O via N; second OGM forwarded
        for seqno in 1..=2 {
            let p = ogm(mac(O), seqno, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
            assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW), Ok(()));
        }
        assert_eq!(hardif.driver_mut().tx_calls, 1);

        // same OGM again from same neighbor: accepted, not forwarded again
        let p = ogm(mac(O), 2, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW), Ok(()));
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(table.find_mac(mac(O)).unwrap().last_seqno(), 2);
    }

    #[test]
    fn process_ogm_new_seqno_zeroes_other_neighbors_test(){
        assert!(TQ_RING_SIZE >= 2, "test assumes TQ_RING_SIZE >= 2");
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);

        // O reachable via N and N2; N2's ring full of good samples
        let mut o = Originator::new(mac(O), NOW);
        o.add_neighbor(Neighbor::new(mac(N), NOW), NOW).unwrap();
        let mut n2 = Neighbor::new(mac(N2), NOW);
        for _ in 0..TQ_RING_SIZE {
            n2.record_transmit_quality(100);
        }
        o.add_neighbor(n2, NOW).unwrap();
        table.add(o, NOW).unwrap();

        let n2_zeros = |t: &OriginatorTable| {
            t.find_mac(mac(O)).unwrap()
                .find_neighbor(mac(N2)).unwrap()
                .tq().values().iter().filter(|&&v| v == 0).count()
        };

        // new seqno via N: one 0 sample pushed to N2, real sample to N
        let p = ogm(mac(O), 1, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW), Ok(()));
        assert_eq!(n2_zeros(&table), 1);
        assert!(table.find_mac(mac(O)).unwrap().find_neighbor(mac(N)).unwrap().tq_avg() > 0);

        // same seqno again (duplicate): no further zeroing
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW), Ok(()));
        assert_eq!(n2_zeros(&table), 1);
    }

    #[test]
    fn process_ogm_ttl_test(){
        let mut hardif = up_if();
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);

        // single hop, ttl 1: link updated, not forwarded
        let p = ogm(mac(N), 1, 1, 255, BATADV_PRIMARIES_FIRST_HOP, mac(N));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW), Ok(()));
        let link_ttl = table.find_mac(mac(N)).unwrap().find_neighbor(mac(N)).unwrap().last_ttl();
        assert_eq!(link_ttl, 1);
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // multi hop via router, ttl 1: route kept current, not forwarded
        let p1 = ogm(mac(O), 1, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p1), NOW), Ok(()));
        let p2 = ogm(mac(O), 2, 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(process_ogm(&mut hardif, &mut table, mac(N), &wire(p2), NOW), Ok(()));
        assert_eq!(table.find_mac(mac(O)).unwrap().last_seqno(), 2);
        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_ogm_capacity_test(){
        // originator table full: new originator rejected
        let mut hardif = up_if();
        let mut table = full_table();
        let p = ogm(mac(N), 1, BATADV_TTL, 255, BATADV_PRIMARIES_FIRST_HOP, mac(N));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW),
            Err(DropReason::OriginatorTable(OriginatorTableError::TableFull))
        );
        assert!(table.find_mac(mac(N)).is_none());

        // O's neighbor list full: new neighbor rejected
        let mut table = OriginatorTable::new();
        add_direct(&mut table, mac(N), 10, 8, 200);
        let mut o = Originator::new(mac(O), NOW);
        for i in 0..MAX_NEIGHBORS {
            o.add_neighbor(Neighbor::new(mac(100 + i as u8), NOW), NOW).unwrap();
        }
        table.add(o, NOW).unwrap();
        let p = ogm(mac(O), 1, BATADV_TTL - 1, 200, BATADV_DIRECTLINK, mac(O));
        assert_eq!(
            process_ogm(&mut hardif, &mut table, mac(N), &wire(p), NOW),
            Err(DropReason::Originator(OriginatorError::NeighborListFull))
        );

        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }


}