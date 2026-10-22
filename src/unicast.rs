use crate::packets::{Packet, BatmanAdvUnicastPacket, ETH_P_BATMAN};
use crate::meshctx::{DropReason, TransmitionError};
use crate::hardif::{HardInterface, LinkDriver};
use crate::originatortable::OriginatorTable;
use crate::mac::Mac;
use crate::consts::{ETH_HLEN, UNICAST_HEADROOM, MAX_BATMAN_FRAME_SIZE};
use crate::broadcast::pad_to_min_frame;
use crate::eth::EthHeader;
use crate::framebuffer::{FrameBuffer, FrameError::TooLarge};
use crate::hardif::InterfaceError::InterfaceDown;

const UNICAST_INNER_OFFSET: usize = ETH_HLEN + BatmanAdvUnicastPacket::WIRE_LEN;
pub enum UnicastType {
    Invalid,
    ForMe,
    Forward{
        unicast: BatmanAdvUnicastPacket,
    },
}


pub fn process_unicast<'f, D: LinkDriver>(
    hardif: &mut HardInterface<D>,
    orig_table: &mut OriginatorTable,
    frame: &'f mut [u8],
   // now: Instant,             might be needed later 
) -> Result<Option<&'f [u8]>, DropReason> {
    let my_mac: Mac = hardif.hardware_addr();
    if frame.len() > MAX_BATMAN_FRAME_SIZE || frame.len() < UNICAST_INNER_OFFSET{
        return Err(DropReason::InvalidUnicast);
    } 
 
    match classify_unicast(my_mac, &frame[ETH_HLEN..]) {
        UnicastType::Invalid => {
            return Err(DropReason::InvalidUnicast);
        },
        UnicastType::ForMe => {
            // return only the ethernet frame playload
            let frame: &'f [u8] = frame;
            Ok(Some(&frame[UNICAST_INNER_OFFSET..]))
        },
        UnicastType::Forward { mut unicast } => {
            
            if !unicast.decrement_ttl() {
                return Err(DropReason::TtlExceeded);
            }

            // find destination in orig table
            let dest_orig = match orig_table.find_mac(unicast.dest()) {
                Some(orig) => orig,
                Option::None => {
                    return Err(DropReason::NoRoute);
                }
            };

            // find router addrss
            let router_mac = match dest_orig.router_id() {
                Some(mac) => mac,
                Option::None => {
                    return Err(DropReason::NoRoute);
                }
            };

            // write payload
            unicast.write(&mut frame[ETH_HLEN..])?;

            // write new reciver
            EthHeader::new(router_mac, my_mac, ETH_P_BATMAN)
                .write(&mut frame[..ETH_HLEN])?;

            // transmit to next node
            let _ = hardif.transmit(frame);
            
            return Ok(None);
        }

    }
}



pub fn classify_unicast(my_mac: Mac, payload: &[u8]) -> UnicastType {
    // parse payload
    let unicast:BatmanAdvUnicastPacket = match BatmanAdvUnicastPacket::parse(payload) {
        Ok(unicast) => unicast,
        Err(_e) => return UnicastType::Invalid,
    };

    // check if empty 
    if payload.len()<UNICAST_INNER_OFFSET{
        return UnicastType::Invalid;
    }
    // check if valid mac
    if !unicast.dest().is_valid(){
        return UnicastType::Invalid;
    }
    // check if its for this node
    if unicast.dest()==my_mac{
        return UnicastType::ForMe;
    }
    // 
    UnicastType::Forward { 
        unicast 
    }

}



pub fn send_unicast<D:LinkDriver>(
    hardif: &mut HardInterface<D>,
    orig_table: &OriginatorTable,
   // buffer: &mut FrameBuffer<MAX_BATMAN_FRAME_SIZE, UNICAST_HEADROOM>,
    eth_frame: &[u8],
)->Result<(),TransmitionError>{
    if !hardif.is_up(){
        return Err(TransmitionError::HardInterface(InterfaceDown));
    }
    
    let my_mac: Mac = hardif.hardware_addr();

    // parse the inner header for its destination
    let dest: Mac = EthHeader::parse(eth_frame)?.dest();
 
    // error if it wants to send it to itself
    if dest == my_mac {
        return Err(TransmitionError::InvalidDestination);
    }
 
    // find next hop 
    let next_hop: Mac = match orig_table.find_mac(dest) {
        Some(orig) => match orig.router_id() {
            Some(router) => router,
            Option::None => return Err(TransmitionError::NoRoute),
        },
        Option::None => return Err(TransmitionError::NoRoute),
    };

    if (eth_frame.len() + UNICAST_INNER_OFFSET) > hardif.max_frame_len(){
        return Err(TransmitionError::FrameBuffer(TooLarge));
    } 

    let unicast_header: BatmanAdvUnicastPacket = BatmanAdvUnicastPacket::new(dest);
    let buffer = &mut FrameBuffer::<MAX_BATMAN_FRAME_SIZE, UNICAST_HEADROOM>::new();
    match build_unicast_frame(unicast_header, next_hop, my_mac, eth_frame, buffer){
        Ok(())=>{
            match hardif.transmit(buffer.bytes()){
                Ok(()) => Ok(()),
                Err(e)=> return Err(TransmitionError::HardInterface(e)),
            }
        },
        Err(err)=>return Err(err),
    }


}


pub fn build_unicast_frame(
    unicast_header: BatmanAdvUnicastPacket,
    next_hop: Mac,
    source_mac: Mac,
    eth_frame: &[u8],
    buffer: &mut FrameBuffer<MAX_BATMAN_FRAME_SIZE, UNICAST_HEADROOM>,
) -> Result<(), TransmitionError> {
    buffer.reset_cursors();
 
    buffer.append(eth_frame.len())?.copy_from_slice(eth_frame);
 
    let buf: &mut [u8] = buffer.prepend(BatmanAdvUnicastPacket::WIRE_LEN)?;
    unicast_header.write(buf)?;
 
    let eth_slot = buffer.prepend(ETH_HLEN)?;
    let eth_header: EthHeader = EthHeader::new(next_hop, source_mac, ETH_P_BATMAN);
    eth_header.write(eth_slot)?;
 
    pad_to_min_frame(buffer)?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::MIN_ETH_FRAME_SIZE;
    use crate::framebuffer::FrameError;
    use crate::hardif::InterfaceError;
    use crate::originator::Originator;
    use crate::packets::{BATADV_BCAST, BATADV_IV_OGM, BATADV_TTL};
    use crate::time::Instant;

    // --- test helpers ---

    const NOW: Instant = Instant::from_ticks(0);

    // node ids for mac()
    const MY: u8 = 1;      // this node
    const NEIGH: u8 = 2;   // previous hop
    const DEST: u8 = 3;    // final destination
    const ROUTER: u8 = 4;  // next hop towards DEST
    const HOST: u8 = 5;    // inner frame source
    const UNKNOWN: u8 = 9; // in no table

    const INNER_LEN: usize = ETH_HLEN + 6;


    fn mac(n: u8) -> Mac {
        Mac::from_octets([0x02, 0x00, 0x00, 0x00, 0x00, n])
    }

    struct MockDriver {
        mac: Mac,
        link: bool,
        tx_calls: u32,
        last_tx: [u8; MAX_BATMAN_FRAME_SIZE],
        last_tx_len: usize,
    }

    impl LinkDriver for MockDriver {
        fn hardware_address(&self) -> Mac {
            self.mac
        }
        fn payload_size(&self) -> usize {
            MAX_BATMAN_FRAME_SIZE - ETH_HLEN
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
            last_tx: [0u8; MAX_BATMAN_FRAME_SIZE],
            last_tx_len: 0,
        };
        let mut hardif = HardInterface::new(drv, 0).expect("unicast MAC must construct");
        hardif.set_active(true);
        hardif
    }

    // table with route DEST -> next hop ROUTER
    fn routed_table() -> OriginatorTable {
        let mut orig = Originator::new(mac(DEST), NOW);
        orig.set_router(mac(ROUTER));
        let mut table = OriginatorTable::new();
        table.add(orig, NOW).unwrap();
        table
    }

    // inner ethernet frame addressed to `dst`, 6 payload bytes
    fn inner_to(dst: Mac) -> [u8; INNER_LEN] {
        let mut f = [0u8; INNER_LEN];
        assert!(EthHeader::new(dst, mac(HOST), 0x0800).write(&mut f[..ETH_HLEN]).is_ok());
        f[ETH_HLEN..].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02]);
        f
    }

    // eth(eth_dst, eth_src) + unicast(dest, ttl) + inner, written into buf; returns len
    fn unicast_frame(buf: &mut [u8], eth_dst: Mac, eth_src: Mac, dest: Mac, ttl: u8, inner: &[u8]) -> usize {
        assert!(EthHeader::new(eth_dst, eth_src, ETH_P_BATMAN)
            .write(&mut buf[..ETH_HLEN])
            .is_ok());
        BatmanAdvUnicastPacket::new(dest)
            .write(&mut buf[ETH_HLEN..])
            .unwrap();
        buf[ETH_HLEN + 2] = ttl; // unicast TTL offset in batman-adv wire layout
        let end = UNICAST_INNER_OFFSET + inner.len();
        buf[UNICAST_INNER_OFFSET..end].copy_from_slice(inner);
        end
    }
        
    #[test]
    fn classify_unicast_test(){
        let my = mac(MY);
        let hdr_len = BatmanAdvUnicastPacket::WIRE_LEN;

        // payload = unicast header for DEST + inner frame
        let hdr = BatmanAdvUnicastPacket::new(mac(DEST));
        let mut payload = [0u8; 64];
        hdr.write(&mut payload).unwrap();
        payload[hdr_len..hdr_len + INNER_LEN].copy_from_slice(&inner_to(mac(DEST)));
        let len = hdr_len + INNER_LEN;

        // for another node: Forward with parsed header
        assert!(matches!(
            classify_unicast(my, &payload[..len]),
            UnicastType::Forward { unicast } if unicast == hdr
        ));

        // for this node: ForMe
        let mut to_me = payload;
        BatmanAdvUnicastPacket::new(my).write(&mut to_me).unwrap();
        assert!(matches!(classify_unicast(my, &to_me[..len]), UnicastType::ForMe));

        // inner exactly one eth header: accepted
        assert!(matches!(
            classify_unicast(my, &payload[..hdr_len + ETH_HLEN]),
            UnicastType::Forward { .. }
        ));

        // inner shorter than eth header: Invalid
        assert!(matches!(
            classify_unicast(my, &payload[..hdr_len + ETH_HLEN - 1]),
            UnicastType::Invalid
        ));

        // truncated unicast header 
        assert!(matches!(classify_unicast(my, &payload[..hdr_len - 1]), UnicastType::Invalid));

        // empty unicast header
        assert!(matches!(classify_unicast(my, &[]), UnicastType::Invalid));

        // wrong packet type (broadcast)
        let mut wrong = payload;
        wrong[0] = BATADV_BCAST;
        assert!(matches!(classify_unicast(my, &wrong[..len]), UnicastType::Invalid));

        // wrong packet type (ogm)
        let mut wrong = payload;
        wrong[0] = BATADV_IV_OGM;
        assert!(matches!(classify_unicast(my, &wrong[..len]), UnicastType::Invalid));

        // bad compat version
        let mut bad = payload;
        bad[1] = 14;
        assert!(matches!(classify_unicast(my, &bad[..len]), UnicastType::Invalid));

        // invalid dest: zero, broadcast, multicast
        for &bad_dest in [Mac::ZERO, Mac::BROADCAST, Mac::STP].iter() {
            let mut p = payload;
            BatmanAdvUnicastPacket::new(bad_dest).write(&mut p).unwrap();
            assert!(
                matches!(classify_unicast(my, &p[..len]), UnicastType::Invalid),
                "dest {:?}", bad_dest
            );
        }
    }

    #[test]
    fn process_unicast_for_me_test(){
        let mut hardif = up_if();
        let mut table = routed_table();
        let inner = inner_to(mac(MY));
        let mut frame = [0u8; 64];

        // addressed to this node: inner delivered, nothing sent, frame untouched
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(MY), BATADV_TTL, &inner);
        let before = frame;
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(Some(&inner[..])));
        assert_eq!(frame, before);

        // ttl irrelevant for local delivery
        for &ttl in [1u8, 0].iter() {
            let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(MY), ttl, &inner);
            let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
            assert_eq!(res, Ok(Some(&inner[..])), "ttl {}", ttl);
        }

        // no route needed for local delivery
        let mut empty = OriginatorTable::new();
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(MY), BATADV_TTL, &inner);
        let res = process_unicast(&mut hardif, &mut empty, &mut frame[..len]);
        assert_eq!(res, Ok(Some(&inner[..])));

        // no transmittions sent
        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_unicast_forward_test(){
        let mut hardif = up_if();
        let mut table = routed_table();

        // inner ethernet frame
        let inner = inner_to(mac(DEST));

        // frames
        let mut frame = [0u8; 64];
        let mut expected = [0u8; 64];

        // for another node with route: forwarded, nothing delivered locally
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(None));

        // frame rewritten in place: eth dst = next hop, src = this node, ttl - 1
        unicast_frame(&mut expected, mac(ROUTER), mac(MY), mac(DEST), BATADV_TTL - 1, &inner);
        assert_eq!(&frame[..len], &expected[..len]);

        // exactly that frame sent, once
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, len);
        assert_eq!(&hardif.driver_mut().last_tx[..len], &expected[..len]);
        assert_eq!(hardif.tx_frames_sent(), 1);

        // ttl 2: last forwarding hop, goes out with ttl 1
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), 2, &inner);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(None));
        assert_eq!(frame[ETH_HLEN + 2], 1);
        assert_eq!(hardif.driver_mut().tx_calls, 2);

        // ttvn carried through unchanged
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        frame[ETH_HLEN + 3] = 7; // unicast TTVN offset
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(None));
        assert_eq!(frame[ETH_HLEN + 3], 7);

    }

    #[test]
    fn process_unicast_ttl_exceeded_test(){
        let mut hardif = up_if();
        let mut table = routed_table();
        let inner = inner_to(mac(DEST));
        let mut frame = [0u8; 64];

        // ttl 1 and 0 for another node: dropped, nothing sent, frame untouched
        for &ttl in [1u8, 0].iter() {
            let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), ttl, &inner);
            let before = frame;
            let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
            assert_eq!(res, Err(DropReason::TtlExceeded), "ttl {}", ttl);
            assert_eq!(frame, before, "ttl {}: frame modified", ttl);
        }

        // ttl checked before route lookup
        let unknown = inner_to(mac(UNKNOWN));
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(UNKNOWN), 1, &unknown);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::TtlExceeded));

        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_unicast_no_route_test(){
        let mut hardif = up_if();
        let mut table = routed_table();
        let mut frame = [0u8; 64];

        // dest not in table: NoRoute, frame untouched
        let unknown = inner_to(mac(UNKNOWN));
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(UNKNOWN), BATADV_TTL, &unknown);
        let before = frame;
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::NoRoute));
        assert_eq!(frame, before);

        // dest known, no router yet: NoRoute, frame untouched
        let mut unrouted = OriginatorTable::new();
        unrouted.add(Originator::new(mac(DEST), NOW), NOW).unwrap();
        let inner = inner_to(mac(DEST));
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        let before = frame;
        let res = process_unicast(&mut hardif, &mut unrouted, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::NoRoute));
        assert_eq!(frame, before);

        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_unicast_invalid_test(){
        let mut hardif = up_if();
        let mut table = routed_table();
        let inner = inner_to(mac(DEST));
        let mut frame = [0u8; 64];

        // wrong packet type (Broadcast)
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        frame[ETH_HLEN] = BATADV_BCAST;
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::InvalidUnicast));

        // wrong packet type (OGM)
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        frame[ETH_HLEN] = BATADV_IV_OGM;
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::InvalidUnicast));

        // bad compat version
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        frame[ETH_HLEN + 1] = 14;
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::InvalidUnicast));

        // invalid dest: zero, broadcast, multicast
        for &bad in [Mac::ZERO, Mac::BROADCAST, Mac::STP].iter() {
            let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), bad, BATADV_TTL, &inner);
            let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
            assert_eq!(res, Err(DropReason::InvalidUnicast), "dest {:?}", bad);
        }

        // inner shorter than an eth header
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner[..ETH_HLEN - 1]);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Err(DropReason::InvalidUnicast));

        // nothing sent
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // control: inner exactly one eth header is forwarded
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner[..ETH_HLEN]);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(None));
        assert_eq!(hardif.driver_mut().tx_calls, 1);

    }

    #[test]
    fn process_unicast_interface_down_test(){
        let mut hardif = up_if();
        let mut table = routed_table();
        let inner = inner_to(mac(DEST));
        let mut frame = [0u8; 64];

        // forward while inactive: Ok(None), failed send counted as dropped
        hardif.set_active(false);
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(None));
        assert_eq!(hardif.driver_mut().tx_calls, 0);
        assert_eq!(hardif.tx_dropped(), 1);

        // link down: same
        hardif.set_active(true);
        hardif.driver_mut().link = false;
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(None));
        assert_eq!(hardif.driver_mut().tx_calls, 0);
        assert_eq!(hardif.tx_dropped(), 2);

        // local delivery unaffected
        let to_me = inner_to(mac(MY));
        let len = unicast_frame(&mut frame, mac(MY), mac(NEIGH), mac(MY), BATADV_TTL, &to_me);
        let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
        assert_eq!(res, Ok(Some(&to_me[..])));

    }

    #[test]
    fn process_unicast_short_frame_test(){
        let mut hardif = up_if();
        let mut table = routed_table();
        let inner = inner_to(mac(DEST));

        // valid frame to truncate
        let mut full = [0u8; 64];
        let full_len = unicast_frame(&mut full, mac(MY), mac(NEIGH), mac(DEST), BATADV_TTL, &inner);

        // every length below eth + unicast + inner eth header: dropped as InvalidUnicast
        for len in 0..UNICAST_INNER_OFFSET + ETH_HLEN {
            let mut frame = full;
            let res = process_unicast(&mut hardif, &mut table, &mut frame[..len]);
            assert_eq!(res, Err(DropReason::InvalidUnicast), "{}-byte frame", len);
        }
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // control: full frame forwarded
        let res = process_unicast(&mut hardif, &mut table, &mut full[..full_len]);
        assert_eq!(res, Ok(None));

    }

    #[test]
    fn build_unicast_frame_test(){
        // both headers must fit in headroom
        assert!(UNICAST_HEADROOM >= UNICAST_INNER_OFFSET);

        let hdr = BatmanAdvUnicastPacket::new(mac(DEST));
        let mut buffer = FrameBuffer::<MAX_BATMAN_FRAME_SIZE, UNICAST_HEADROOM>::new();

        // inner frame above minimum: no padding
        let mut inner = [0u8; MIN_ETH_FRAME_SIZE + 10];
        for (i, b) in inner.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1); // nonzero, distinct
        }
        assert!(build_unicast_frame(hdr, mac(ROUTER), mac(MY), &inner, &mut buffer).is_ok());
        assert_eq!(buffer.len(), UNICAST_INNER_OFFSET + inner.len());
        let out = buffer.bytes();

        // eth header: next hop dst, our src, batman ethertype
        let mut eth = [0u8; ETH_HLEN];
        assert!(EthHeader::new(mac(ROUTER), mac(MY), ETH_P_BATMAN).write(&mut eth).is_ok());
        assert_eq!(&out[..ETH_HLEN], &eth[..]);
        assert_eq!(&out[12..14], &[0x43u8, 0x05]); // ethertype big-endian

        // unicast header round-trips
        assert_eq!(BatmanAdvUnicastPacket::parse(&out[ETH_HLEN..]), Ok(hdr));

        // inner frame copied verbatim after headers
        assert_eq!(&out[UNICAST_INNER_OFFSET..], &inner[..]);

        // reuse buffer, small inner frame: padded, old bytes not leaked
        let small = [0xAAu8; 4];
        assert!(build_unicast_frame(hdr, mac(ROUTER), mac(MY), &small, &mut buffer).is_ok());
        let unpadded = UNICAST_INNER_OFFSET + small.len();
        assert_eq!(buffer.len(), unpadded.max(MIN_ETH_FRAME_SIZE));
        let out = buffer.bytes();
        assert_eq!(BatmanAdvUnicastPacket::parse(&out[ETH_HLEN..]), Ok(hdr));
        assert_eq!(&out[UNICAST_INNER_OFFSET..unpadded], &small[..]);
        assert!(out[unpadded..].iter().all(|&b| b == 0));

        // empty inner frame: headers only, padded
        assert!(build_unicast_frame(hdr, mac(ROUTER), mac(MY), &[], &mut buffer).is_ok());
        assert_eq!(buffer.len(), UNICAST_INNER_OFFSET.max(MIN_ETH_FRAME_SIZE));

        // largest inner frame that fits tailroom
        let room = MAX_BATMAN_FRAME_SIZE - UNICAST_HEADROOM;
        let big = [0x55u8; MAX_BATMAN_FRAME_SIZE];
        assert!(build_unicast_frame(hdr, mac(ROUTER), mac(MY), &big[..room], &mut buffer).is_ok());
        assert_eq!(buffer.len(), UNICAST_INNER_OFFSET + room);

        // one byte more: NoTailRoom wrapped as FrameBuffer error
        let res = build_unicast_frame(hdr, mac(ROUTER), mac(MY), &big[..room + 1], &mut buffer);
        assert!(matches!(
            res,
            Err(TransmitionError::FrameBuffer(FrameError::NoTailRoom { need, got }))
                if need == room + 1 && got == room
        ));

    }

    #[test]
    fn send_unicast_test(){
        let mut hardif = up_if();
        let table = routed_table();
        let inner = inner_to(mac(DEST));

        // routed dest: framed for next hop, sent once
        assert!(send_unicast(&mut hardif, &table, &inner).is_ok());

        let mut expected = [0u8; 64];
        let len = unicast_frame(&mut expected, mac(ROUTER), mac(MY), mac(DEST), BATADV_TTL, &inner);
        let sent = len.max(MIN_ETH_FRAME_SIZE);
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, sent);
        assert_eq!(&hardif.driver_mut().last_tx[..len], &expected[..len]);

        // padded to minimum with zeros
        assert!(hardif.driver_mut().last_tx[len..sent].iter().all(|&b| b == 0));
        assert_eq!(hardif.tx_frames_sent(), 1);
        
    }

    #[test]
    fn send_unicast_reject_test(){
        let table = routed_table();
        let inner = inner_to(mac(DEST));

        // interface down
        let mut down = up_if();
        down.set_active(false);
        let res = send_unicast(&mut down, &table, &inner);
        assert!(matches!(res, Err(TransmitionError::HardInterface(InterfaceError::InterfaceDown))));
        assert_eq!(down.driver_mut().tx_calls, 0);

        let mut hardif = up_if();

        // inner too short to hold an eth header
        assert!(send_unicast(&mut hardif, &table, &inner[..ETH_HLEN - 1]).is_err());

        // addressed to this node
        let res = send_unicast(&mut hardif, &table, &inner_to(mac(MY)));
        assert!(matches!(res, Err(TransmitionError::InvalidDestination)));

        // unknown dest
        let res = send_unicast(&mut hardif, &table, &inner_to(mac(UNKNOWN)));
        assert!(matches!(res, Err(TransmitionError::NoRoute)));

        // known dest, no router yet
        let mut unrouted = OriginatorTable::new();
        unrouted.add(Originator::new(mac(DEST), NOW), NOW).unwrap();
        let res = send_unicast(&mut hardif, &unrouted, &inner);
        assert!(matches!(res, Err(TransmitionError::NoRoute)));

        // nothing sent
        assert_eq!(hardif.driver_mut().tx_calls, 0);
        
    }

    #[test]
    fn send_unicast_frame_size_limit_test(){
        let mut hardif = up_if();
        let table = routed_table();

        // inner frame to DEST, sized up to what the interface allows
        let max_inner = MAX_BATMAN_FRAME_SIZE - UNICAST_INNER_OFFSET;
        let mut big = [0x5Au8; MAX_BATMAN_FRAME_SIZE];
        assert!(EthHeader::new(mac(DEST), mac(HOST), 0x0800).write(&mut big[..ETH_HLEN]).is_ok());

        // one byte over: TooLarge, nothing sent
        let res = send_unicast(&mut hardif, &table, &big[..max_inner + 1]);
        assert!(matches!(res, Err(TransmitionError::FrameBuffer(FrameError::TooLarge))));
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // exactly at limit: sent at full size
        assert!(send_unicast(&mut hardif, &table, &big[..max_inner]).is_ok());
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, MAX_BATMAN_FRAME_SIZE);
    }
}