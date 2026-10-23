use crate::FrameError::TooLarge;
use crate::InterfaceError::InterfaceDown;
use crate::framebuffer::{ FrameBuffer, FrameError};
use crate::meshctx::{TransmitionError, DropReason};
use crate::hardif::{HardInterface, LinkDriver};
use crate::originatortable::OriginatorTable;
use crate::mac::Mac;
use crate::packets::{BatmanAdvBroadcastPacket, Packet, ETH_P_BATMAN};
use crate::time::Instant;
use crate::consts::{ETH_HLEN, MAX_BATMAN_FRAME_SIZE, BCAST_HEADROOM, MIN_ETH_FRAME_SIZE};
use crate::eth::EthHeader;


const BCAST_INNER_OFFSET: usize = ETH_HLEN + BatmanAdvBroadcastPacket::WIRE_LEN;

pub enum BroadcastType{
    Invalid,
    Normal{
        bcast: BatmanAdvBroadcastPacket, 
    }

}


pub fn process_bcast<'f, D: LinkDriver>(
    hardif: &mut HardInterface<D>,
    orig_table: &mut OriginatorTable,
    frame: &'f mut [u8],
    now: Instant,
) -> Result<&'f [u8], DropReason> {
    let my_mac: Mac = hardif.hardware_addr();
    if frame.len() > MAX_BATMAN_FRAME_SIZE || frame.len() < BCAST_INNER_OFFSET{
        return Err(DropReason::InvalidBcast);
    } 
 
    match classify_bcast(my_mac, &frame[ETH_HLEN..]) {
        BroadcastType::Invalid => {
            return Err(DropReason::InvalidBcast);
        }
        BroadcastType::Normal { mut bcast } => {
            let sender_orig = match orig_table.find_mac_mut(bcast.originator()) {
                Some(orig) => orig,
                Option::None => {
                    return Err(DropReason::UnknownOriginator);
                }
            };
 
            let is_not_dup = sender_orig.check_bcast(bcast.seqno(), now);
 
            if !is_not_dup {
                return Err(DropReason::Duplicate);
            }
 
            // rebroadcast
            if bcast.decrement_ttl() {
                // write payload
                bcast.write(&mut frame[ETH_HLEN..])?;

                // write new reciver
                EthHeader::new(Mac::BROADCAST, my_mac, ETH_P_BATMAN)
                    .write(&mut frame[..ETH_HLEN])?;

                // transmit to next node
                let _ = hardif.transmit(frame);
            }
 
            // return only the ethernet frame playload
            let frame: &'f [u8] = frame;
            Ok(&frame[BCAST_INNER_OFFSET..])
        }
    }
}



pub fn classify_bcast(my_mac: Mac, payload: &[u8]) -> BroadcastType {
    let bcast:BatmanAdvBroadcastPacket = match BatmanAdvBroadcastPacket::parse(payload) {
        Ok(bcast) => bcast,
        Err(_e) => return BroadcastType::Invalid,
    };
 
    // check if empty 
    if payload.len()<14{
        return BroadcastType::Invalid;
    }

    let orig = bcast.originator();

    // check if zero
    if !orig.is_valid() {
        return BroadcastType::Invalid;
    }

    // did this node send it
    if orig == my_mac {
        return BroadcastType::Invalid;
    }
 
    BroadcastType::Normal {
        bcast,
    }
    
}

pub fn send_broadcast<D:LinkDriver>(
    hardif: &mut HardInterface<D>, 
    bcast_seqno: &mut u32, 
   // buffer: &mut FrameBuffer<MAX_BATMAN_FRAME_SIZE, BCAST_HEADROOM>,
    eth_frame: &[u8],
)->Result<(), TransmitionError>{
    if !hardif.is_up(){
        return Err(TransmitionError::HardInterface(InterfaceDown));
    }
    
    let my_mac: Mac = hardif.hardware_addr();

    if (eth_frame.len() + BCAST_INNER_OFFSET) > hardif.max_frame_len(){
        return Err(TransmitionError::FrameBuffer(TooLarge));
    } 
    let seqno: u32 = *bcast_seqno;
    *bcast_seqno = bcast_seqno.wrapping_add(1);

    let bcast_header: BatmanAdvBroadcastPacket = BatmanAdvBroadcastPacket::new(my_mac, seqno);
    let buffer = &mut FrameBuffer::<MAX_BATMAN_FRAME_SIZE, BCAST_HEADROOM>::new();

    match build_bcast_frame(bcast_header, my_mac, eth_frame, buffer) {
        Ok(())=>{
            match hardif.transmit(buffer.bytes()){
                Ok(()) => Ok(()),
                Err(e)=> return Err(TransmitionError::HardInterface(e)),
            }
        },
        Err(err)=>return Err(err),
    }
}


pub fn build_bcast_frame(
    bcast_header:BatmanAdvBroadcastPacket,
    source_mac: Mac, 
    eth_frame: &[u8],
    buffer: &mut FrameBuffer<MAX_BATMAN_FRAME_SIZE, BCAST_HEADROOM>,
)->Result<(), TransmitionError>{
    buffer.reset_cursors();

    buffer.append(eth_frame.len())?.copy_from_slice(eth_frame);

    let buf: &mut [u8] = buffer.prepend(BatmanAdvBroadcastPacket::WIRE_LEN)?;

    bcast_header.write(buf)?;

    let eth_slot = buffer.prepend(ETH_HLEN)?;
    let eth_header: EthHeader = EthHeader::new(
        Mac::BROADCAST, 
        source_mac, 
        ETH_P_BATMAN
    );
    eth_header.write(eth_slot)?;

    pad_to_min_frame(buffer)?;
    Ok(())
}

pub fn pad_to_min_frame<const N: usize, const H: usize>(
    buffer: &mut FrameBuffer<N, H>,
) -> Result<(), FrameError> {
    let missing = MIN_ETH_FRAME_SIZE.saturating_sub(buffer.len());
    buffer.append(missing)?.fill(0);
    Ok(())
}


#[cfg(test)]
mod test{
    use super::*;
    use crate::hardif::InterfaceError;
    use crate::originator::Originator;
    use crate::packets::{BATADV_TTL, BATADV_UNICAST};

    // --- test helpers ---
    const NOW: Instant = Instant::from_ticks(0);
    const INNER: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];

    // node ids for mac()
    const MY: u8 = 1;
    const NEIGH: u8 = 2;
    const ORIG: u8 = 3;

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

    /// active interface, link up, owned by mac(MY)
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

    /// table holding one known originator
    fn table_with_orig(id: u8) -> OriginatorTable {
        let mut table = OriginatorTable::new();
        table.add(Originator::new(mac(id), NOW), NOW).unwrap();
        table
    }

    /// eth(dst bcast, src) + bcast(orig, seqno, ttl) + inner, written into buf; returns len
    fn bcast_frame(buf: &mut [u8], src: Mac, orig: Mac, seqno: u32, ttl: u8, inner: &[u8]) -> usize {
        assert!(EthHeader::new(Mac::BROADCAST, src, ETH_P_BATMAN)
            .write(&mut buf[..ETH_HLEN])
            .is_ok());
        BatmanAdvBroadcastPacket::new(orig, seqno)
            .write(&mut buf[ETH_HLEN..])
            .unwrap();
        buf[ETH_HLEN + 2] = ttl; // bcast TTL offset in batman-adv wire layout
        let end = BCAST_INNER_OFFSET + inner.len();
        buf[BCAST_INNER_OFFSET..end].copy_from_slice(inner);
        end
    }
    
    
    #[test]
    fn pad_to_min_frame_test(){
        type Buf = FrameBuffer<{ MIN_ETH_FRAME_SIZE * 2 }, 16>;

        // empty: padded to minimum, all zero
        let mut buf = Buf::new();
        assert_eq!(pad_to_min_frame(&mut buf), Ok(()));
        assert_eq!(buf.len(), MIN_ETH_FRAME_SIZE);
        assert!(buf.bytes().iter().all(|&b| b == 0));

        // short frame: content kept, stale bytes in pad region zeroed
        let mut buf = Buf::new();
        buf.rx_space().fill(0xEE); // dirty backing store, cursors reset
        buf.append(10).unwrap().copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(pad_to_min_frame(&mut buf), Ok(()));
        assert_eq!(buf.len(), MIN_ETH_FRAME_SIZE);
        assert_eq!(&buf.bytes()[..10], &[1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert!(buf.bytes()[10..].iter().all(|&b| b == 0));

        // prepended bytes count toward length
        let mut buf = Buf::new();
        buf.append(4).unwrap().fill(0x11);
        buf.prepend(4).unwrap().fill(0x22);
        assert_eq!(pad_to_min_frame(&mut buf), Ok(()));
        assert_eq!(buf.len(), MIN_ETH_FRAME_SIZE);
        assert!(buf.bytes()[..4].iter().all(|&b| b == 0x22));
        assert!(buf.bytes()[4..8].iter().all(|&b| b == 0x11));
        assert!(buf.bytes()[8..].iter().all(|&b| b == 0));

        // exactly minimum: unchanged
        let mut buf = Buf::new();
        buf.append(MIN_ETH_FRAME_SIZE).unwrap().fill(0xAB);
        assert_eq!(pad_to_min_frame(&mut buf), Ok(()));
        assert_eq!(buf.len(), MIN_ETH_FRAME_SIZE);
        assert!(buf.bytes().iter().all(|&b| b == 0xAB));

        // above minimum: unchanged
        let mut buf = Buf::new();
        buf.append(MIN_ETH_FRAME_SIZE + 5).unwrap().fill(0xAB);
        assert_eq!(pad_to_min_frame(&mut buf), Ok(()));
        assert_eq!(buf.len(), MIN_ETH_FRAME_SIZE + 5);
        assert!(buf.bytes().iter().all(|&b| b == 0xAB));

        // not enough tailroom: NoTailRoom, buffer unchanged
        let mut tight: FrameBuffer<{ MIN_ETH_FRAME_SIZE - 1 }, 0> = FrameBuffer::new();
        assert_eq!(
            pad_to_min_frame(&mut tight),
            Err(FrameError::NoTailRoom {
                need: MIN_ETH_FRAME_SIZE,
                got: MIN_ETH_FRAME_SIZE - 1,
            })
        );
        assert_eq!(tight.len(), 0);

    }

    #[test]
    fn build_bcast_frame_test(){
        let src = mac(MY);
        let header = BatmanAdvBroadcastPacket::new(mac(ORIG), 0x01020304);

        // normal frame
        let mut buf = FrameBuffer::<MAX_BATMAN_FRAME_SIZE,BCAST_HEADROOM>::new();
        let mut data = [0u8;MIN_ETH_FRAME_SIZE+10];

        // add data to data varible
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        } 
        assert!(build_bcast_frame(header, src, &data, &mut buf).is_ok());
        assert_eq!(buf.len(),BCAST_INNER_OFFSET+data.len());

        let out = buf.bytes();

        let mut eth = [0u8; ETH_HLEN];
        assert!(EthHeader::new(Mac::BROADCAST, src, ETH_P_BATMAN).write(&mut eth).is_ok());
        assert_eq!(&out[..ETH_HLEN], &eth[..]);

        // ethertype is big-endian
        assert_eq!(&out[12..14], &[0x43u8, 0x05]); 

        // same data in buffer and data vec
        assert_eq!(&out[BCAST_INNER_OFFSET..], &data[..]);
    
        // frame can be parsed
        assert_eq!(BatmanAdvBroadcastPacket::parse(&out[ETH_HLEN..]), Ok(header));


        // padded frames
        let small = [0xAAu8;4];
        assert!(build_bcast_frame(header, src, &small, &mut buf).is_ok());
        assert_eq!(buf.len(),MIN_ETH_FRAME_SIZE);
        let out = buf.bytes();

        // data is correctly added to buffer
        let unpadded = small.len()+BCAST_INNER_OFFSET;
        assert_eq!(&out[BCAST_INNER_OFFSET..unpadded], &small[..]);

        // padding is correctly added to buffer
        assert!(&out[unpadded..].iter().all(|&val| val == 0));

        // frame can be parsed
        assert_eq!(BatmanAdvBroadcastPacket::parse(&out[ETH_HLEN..]), Ok(header));

        // empty inner frame (no data)
        assert!(build_bcast_frame(header, src, &[], &mut buf).is_ok());
        assert_eq!(buf.len(), MIN_ETH_FRAME_SIZE);
        let out = buf.bytes();

        // all zeros
        assert!(&out[BCAST_INNER_OFFSET..].iter().all(|&val| val == 0));


        // largest inner frame that fits tailroom
        let room = MAX_BATMAN_FRAME_SIZE - BCAST_HEADROOM;
        let big = [0x55u8; MAX_BATMAN_FRAME_SIZE];
        assert!(build_bcast_frame(header, src, &big[..room], &mut buf).is_ok());
        assert_eq!(buf.len(), BCAST_INNER_OFFSET + room);


        // one byte more: NoTailRoom wrapped as FrameBuffer error
        let res = build_bcast_frame(header, src, &big[..room + 1], &mut buf);
        assert_eq!(
            res,
            Err(TransmitionError::FrameBuffer(FrameError::NoTailRoom { need:room+1, got:room })),
        );

    }

    #[test]
    fn classify_bcast_test(){
        let my = mac(MY);
        let other = mac(ORIG);

        let header = BatmanAdvBroadcastPacket::new(other, 7);
        let mut payload = [0u8; BatmanAdvBroadcastPacket::WIRE_LEN + 8];
        header.write(&mut payload).unwrap();

        // callsify normal Bcast
        match classify_bcast(my, &payload) {
            BroadcastType::Normal { bcast } => assert_eq!(bcast, header),
            BroadcastType::Invalid => panic!("valid bcast classified Invalid"),
        }

        // header only 
        assert!(matches!(
            classify_bcast(my, &payload[..BatmanAdvBroadcastPacket::WIRE_LEN]),
            BroadcastType::Normal { .. }
        ));

        // truncated
        assert!(matches!(classify_bcast(my, &payload[..13]), BroadcastType::Invalid));

        // empty
        assert!(matches!(classify_bcast(my, &[]), BroadcastType::Invalid));

        // wrong packet type
        let mut wrong = payload;
        wrong[0] = BATADV_UNICAST;
        assert!(matches!(classify_bcast(my, &wrong), BroadcastType::Invalid));

        // bad version
        let mut bad = payload;
        bad[1] = 14;
        assert!(matches!(classify_bcast(my, &bad), BroadcastType::Invalid));

        // sent by this node
        let mut own = [0u8; BatmanAdvBroadcastPacket::WIRE_LEN];
        BatmanAdvBroadcastPacket::new(my, 7).write(&mut own).unwrap();
        assert!(matches!(classify_bcast(my, &own), BroadcastType::Invalid));

        // invalid originator: zero
        let zero = Mac::ZERO;
        let mut header = [0u8; BatmanAdvBroadcastPacket::WIRE_LEN];
        BatmanAdvBroadcastPacket::new(zero, 7).write(&mut header).unwrap();
        assert!(matches!(classify_bcast(my, &header), BroadcastType::Invalid));

    }

    #[test]
    fn process_bcast_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];
        let mut expected = [0u8; 64];

        // new bcast from known orig: inner returned, rebroadcast with ttl - 1
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Ok(p) if p == &INNER[..]));

        // frame rewritten in place: src = this node, ttl decremented
        bcast_frame(&mut expected, mac(MY), mac(ORIG), 1, BATADV_TTL - 1, &INNER);
        assert_eq!(&frame[..len], &expected[..len]);

        // exactly that frame handed to driver
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, len);
        assert_eq!(&hardif.driver_mut().last_tx[..len], &expected[..len]);
        assert_eq!(hardif.tx_frames_sent(), 1);

        // same seqno again: Duplicate, no rebroadcast
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Err(DropReason::Duplicate)));
        assert_eq!(hardif.driver_mut().tx_calls, 1);

        // next seqno: accepted
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 2, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Ok(p) if p == &INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 2);

        // ttl 1: delivered locally, not rebroadcast, frame untouched
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 3, 1, &INNER);
        let before = frame;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Ok(p) if p == &INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 2);
        assert_eq!(frame, before);

        // headers only: empty payload, still rebroadcast
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 4, BATADV_TTL, &[]);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Ok(p) if p.is_empty()));
        assert_eq!(hardif.driver_mut().tx_calls, 3);

        // unknown originator: dropped, nothing sent
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(9), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Err(DropReason::UnknownOriginator)));
        assert_eq!(hardif.driver_mut().tx_calls, 3);

        // own broadcast echoed back: InvalidBcast
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(MY), 10, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Err(DropReason::InvalidBcast)));

        // wrong packet type after eth header
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 11, BATADV_TTL, &INNER);
        frame[ETH_HLEN] = BATADV_UNICAST;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert!(matches!(res, Err(DropReason::InvalidBcast)));

        // truncated batman header
        bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 12, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..ETH_HLEN + 5], NOW);
        assert!(matches!(res, Err(DropReason::InvalidBcast)));

        // eth header only
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..ETH_HLEN], NOW);
        assert!(matches!(res, Err(DropReason::InvalidBcast)));

        // no invalid frame reached the driver
        assert_eq!(hardif.driver_mut().tx_calls, 3);

    }

    #[test]
    fn process_bcast_forward_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];
        let mut expected = [0u8; 64];

        // new bcast from known orig: inner delivered
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));

        // frame rewritten in place: src = this node, ttl - 1, rest unchanged
        bcast_frame(&mut expected, mac(MY), mac(ORIG), 1, BATADV_TTL - 1, &INNER);
        assert_eq!(&frame[..len], &expected[..len]);

        // exactly that frame rebroadcast, once
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, len);
        assert_eq!(&hardif.driver_mut().last_tx[..len], &expected[..len]);
        assert_eq!(hardif.tx_frames_sent(), 1);
        assert_eq!(hardif.tx_dropped(), 0);

    }

    #[test]
    fn process_bcast_empty_payload_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];

        // headers only: empty payload delivered, still rebroadcast
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &[]);
        assert_eq!(len, BCAST_INNER_OFFSET);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&[][..]));
        assert_eq!(hardif.driver_mut().tx_calls, 1);

    }

    #[test]
    fn process_bcast_duplicate_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];

        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));

        // same seqno via another neighbor: Duplicate, not rebroadcast, frame untouched
        let len = bcast_frame(&mut frame, mac(9), mac(ORIG), 1, BATADV_TTL, &INNER);
        let before = frame;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::Duplicate));
        assert_eq!(frame, before);
        assert_eq!(hardif.driver_mut().tx_calls, 1);

        // next seqno: accepted
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 2, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 2);

        // older seqno already seen: Duplicate
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::Duplicate));
        assert_eq!(hardif.driver_mut().tx_calls, 2);

    }

    #[test]
    fn process_bcast_ttl_expired_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];

        // ttl 1 and 0: delivered locally, not rebroadcast, frame untouched
        for (i, &ttl) in [1u8, 0].iter().enumerate() {
            let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), i as u32 + 1, ttl, &INNER);
            let before = frame;
            let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
            assert_eq!(res, Ok(&INNER[..]), "ttl {}", ttl);
            assert_eq!(frame, before, "ttl {}: frame modified", ttl);
        }
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // ttl 2: last hop that still forwards, goes out with ttl 1
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 3, 2, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(frame[ETH_HLEN + 2], 1);

    }

    #[test]
    fn process_bcast_unknown_originator_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];

        // originator not in table: dropped, nothing sent, frame untouched
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(9), 1, BATADV_TTL, &INNER);
        let before = frame;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::UnknownOriginator));
        assert_eq!(frame, before);

        // empty table: same
        let mut empty = OriginatorTable::new();
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut empty, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::UnknownOriginator));

        assert_eq!(hardif.driver_mut().tx_calls, 0);

    }

    #[test]
    fn process_bcast_invalid_header_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];

        // own broadcast echoed back
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(MY), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::InvalidBcast));

        // wrong packet type
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        frame[ETH_HLEN] = BATADV_UNICAST;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::InvalidBcast));

        // bad compat version
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        frame[ETH_HLEN + 1] = 14;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::InvalidBcast));

        // invalid originator: zero, broadcast, multicast
        for &bad in [Mac::ZERO, Mac::BROADCAST, Mac::STP].iter() {
            let len = bcast_frame(&mut frame, mac(NEIGH), bad, 1, BATADV_TTL, &INNER);
            let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
            assert_eq!(res, Err(DropReason::InvalidBcast), "orig {:?}", bad);
        }

        // nothing sent
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // rejected headers had no side effects: seqno 1 still accepted
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));

    }

    #[test]
    fn process_bcast_interface_down_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);
        let mut frame = [0u8; 64];

        // inactive: delivered locally, failed rebroadcast counted as dropped
        hardif.set_active(false);
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 0);
        assert_eq!(hardif.tx_dropped(), 1);

        // link down: same
        hardif.set_active(true);
        hardif.driver_mut().link = false;
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 2, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 0);
        assert_eq!(hardif.tx_dropped(), 2);

        // back up: next seqno goes out
        hardif.driver_mut().link = true;
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 3, BATADV_TTL, &INNER);
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Ok(&INNER[..]));
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.tx_dropped(), 2);

    }

    #[test]
    fn process_bcast_short_frame_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);

        // valid frame to truncate: every prefix looks legit up to the cut
        let mut full = [0u8; 64];
        let full_len = bcast_frame(&mut full, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &INNER);

        // every length below eth + bcast header: dropped as InvalidBcast
        for len in 0..BCAST_INNER_OFFSET {
            let mut frame = full;
            let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
            assert_eq!(res, Err(DropReason::InvalidBcast), "{}-byte frame", len);
        }
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // rejections had no side effects: seqno 1 still accepted
        let res = process_bcast(&mut hardif, &mut table, &mut full[..full_len], NOW);
        assert_eq!(res, Ok(&INNER[..]));

    }

    #[test]
    fn process_bcast_frame_size_limit_test(){
        let mut hardif = up_if();
        let mut table = table_with_orig(ORIG);

        // frame one byte over the limit, otherwise valid
        let mut frame = [0u8; MAX_BATMAN_FRAME_SIZE + 1];
        let inner = [0x5Au8; MAX_BATMAN_FRAME_SIZE + 1 - BCAST_INNER_OFFSET];
        let len = bcast_frame(&mut frame, mac(NEIGH), mac(ORIG), 1, BATADV_TTL, &inner);
        assert_eq!(len, MAX_BATMAN_FRAME_SIZE + 1);

        // over limit: dropped, nothing sent
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..len], NOW);
        assert_eq!(res, Err(DropReason::InvalidBcast));
        assert_eq!(hardif.driver_mut().tx_calls, 0);

        // exactly at limit: accepted (same seqno, so rejection had no side effects)
        let max = MAX_BATMAN_FRAME_SIZE;
        let res = process_bcast(&mut hardif, &mut table, &mut frame[..max], NOW);
        assert_eq!(res, Ok(&inner[..max - BCAST_INNER_OFFSET]));

        // rebroadcast went out at full size
        assert_eq!(hardif.driver_mut().tx_calls, 1);
        assert_eq!(hardif.driver_mut().last_tx_len, max);
        
    }

}

