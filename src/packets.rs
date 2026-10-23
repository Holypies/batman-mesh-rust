use crate::mac::Mac;
use crate::bytes::*;
use crate::consts::{TQ_MAX};

/// EtherType that carries batman-adv frames on the link
pub const ETH_P_BATMAN: u16 = 0x4305;

/// Compatibility version. Packets carrying anything else are dropped
pub const BATADV_COMPAT_VERSION: u8 = 15;

/// TTL put into locally generated packets
pub const BATADV_TTL: u8 = 50;

// Packet type values. The first octet of every batman-adv packet
pub const BATADV_IV_OGM: u8 = 0x00;
pub const BATADV_BCAST: u8 = 0x01;
pub const BATADV_UNICAST: u8 = 0x40;

/// Offsets shared by every batman-adv header
const OFF_TYPE: usize = 0;
const OFF_VERSION: usize = 1;

/// max value of tq
const BATADV_TQ_MAX_VALUE:u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketError {
    TooShort { need: usize, got: usize },
    TooLong { need: usize, got: usize },
    WrongType { expected: u16, got: u16 },
    UnknownType,
    BadVersion(u8),
}


/// A batman-adv header that can be written to and read from the wire
pub trait Packet: Sized {
    const WIRE_LEN: usize;
    const PACKET_TYPE: u8;

    fn write(&self, buf: &mut [u8]) -> Result<(), PacketError>;

    fn parse(buf: &[u8]) -> Result<Self, PacketError>;

    fn wire_len(&self) -> usize {
        Self::WIRE_LEN
    }
}

// ---------------------------------------------------------------------------
// Buffer helpers
// ---------------------------------------------------------------------------

fn need(got: usize, want: usize) -> Result<(), PacketError> {
    if got < want {
        Err(PacketError::TooLong {
            need: want,
            got,
        })
    } else {
        Ok(())
    }
}


pub fn peek_type(buf: &[u8]) -> Option<u8> {
    buf.get(OFF_TYPE).copied()
}

pub fn peek_version(buf: &[u8]) -> Option<u8> {
    buf.get(OFF_VERSION).copied()
}

fn check_header(buf: &[u8], want: usize, packet_type: u8) -> Result<(), PacketError> {
    need(buf.len(), want)?;
    if buf[OFF_TYPE] != packet_type {
        return Err(PacketError::WrongType {
            expected: packet_type as u16,
            got: buf[OFF_TYPE] as u16,
        });
    }
    if buf[OFF_VERSION] != BATADV_COMPAT_VERSION {
        return Err(PacketError::BadVersion(buf[OFF_VERSION]));
    }
    Ok(())
}



// ---------------------------------------------------------------------------
// Broadcast packet
// ---------------------------------------------------------------------------

mod bcast_off {
    pub const TTL: usize = 2;
    pub const RESERVED: usize = 3;
    pub const SEQNO: usize = 4;
    pub const ORIG: usize = 8;
}

/// Broadcast packet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatmanAdvBroadcastPacket {
    packet_type: u8,
    version: u8,
    ttl: u8,
    reserved: u8,
    seqno: u32,
    orig: Mac,
}

impl BatmanAdvBroadcastPacket {
    /// Builds a broadcast packet originated by this node
    pub fn new(orig: Mac, seqno: u32) -> Self {
        Self {
            packet_type: BATADV_BCAST,
            version: BATADV_COMPAT_VERSION,
            ttl: BATADV_TTL,
            reserved: 0,
            seqno,
            orig,
        }
    }
    pub fn packet_type(&self)->u8{
        self.packet_type
    }
    pub fn version(&self)->u8{
        self.version
    }
    pub fn ttl(&self)->u8{
        self.ttl
    }
    pub fn reserved(&self)->u8{
        self.reserved
    }
    pub fn seqno(&self)->u32{
        self.seqno
    }
    pub fn originator(&self)->Mac{
        self.orig
    }
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl < 2 {
            return false;
        }
        self.ttl -= 1;
        true
    }

}

impl Packet for BatmanAdvBroadcastPacket {
    const WIRE_LEN: usize = 14;
    const PACKET_TYPE: u8 = BATADV_BCAST;

    fn write(&self, buf: &mut [u8]) -> Result<(), PacketError> {
        need(buf.len(), Self::WIRE_LEN)?;
        buf[OFF_TYPE] = self.packet_type;
        buf[OFF_VERSION] = self.version;
        buf[bcast_off::TTL] = self.ttl;
        buf[bcast_off::RESERVED] = self.reserved;
        write_u32(buf, bcast_off::SEQNO, self.seqno);
        self.orig.write_at(buf, bcast_off::ORIG);
        Ok(())
    }

    fn parse(buf: &[u8]) -> Result<Self, PacketError> {
        check_header(buf, Self::WIRE_LEN, Self::PACKET_TYPE)?;
        Ok(Self {
            packet_type: buf[OFF_TYPE],
            version: buf[OFF_VERSION],
            ttl: buf[bcast_off::TTL],
            reserved: buf[bcast_off::RESERVED],
            seqno: read_u32(buf, bcast_off::SEQNO),
            orig: Mac::read_at(buf, bcast_off::ORIG),
        })
    }
}

// ---------------------------------------------------------------------------
// Unicast packet
// ---------------------------------------------------------------------------

mod unicast_off {
    pub const TTL: usize = 2;
    pub const TTVN: usize = 3;
    pub const DEST: usize = 4;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatmanAdvUnicastPacket {
    packet_type: u8,
    version: u8,
    ttl: u8,
	ttvn: u8,
    dest: Mac,
}

impl BatmanAdvUnicastPacket {
    // TODO: add ttvn later
    pub fn new(dest: Mac) -> Self {
        Self {
            packet_type: BATADV_UNICAST,
            version: BATADV_COMPAT_VERSION,
            ttl: BATADV_TTL,
            ttvn:0,
            dest,
        }
    }
    pub fn packet_type(&self)->u8{
        self.packet_type
    }
    pub fn version(&self)->u8{
        self.version
    }

    pub fn ttl(&self) -> u8 {
        self.ttl
    }

    pub fn ttvn(&self) -> u8 {
        self.ttvn
    }
    pub fn dest(&self) -> Mac {
        self.dest
    }

    /// Decrements the TTL for forwarding
    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl < 2 {
            return false;
        }
        self.ttl -= 1;
        true
    }
}

impl Packet for BatmanAdvUnicastPacket {
    const WIRE_LEN: usize = 10;
    const PACKET_TYPE: u8 = BATADV_UNICAST;

    fn write(&self, buf: &mut [u8]) -> Result<(), PacketError> {
        need(buf.len(), Self::WIRE_LEN)?;
        buf[OFF_TYPE] = self.packet_type;
        buf[OFF_VERSION] = self.version;
        buf[unicast_off::TTL] = self.ttl;
        buf[unicast_off::TTVN] = self.ttvn;
        self.dest.write_at(buf, unicast_off::DEST);
        Ok(())
    }

    fn parse(buf: &[u8]) -> Result<Self, PacketError> {
        check_header(buf, Self::WIRE_LEN, Self::PACKET_TYPE)?;
        Ok(Self {
            packet_type: buf[OFF_TYPE],
            version: buf[OFF_VERSION],
            ttl: buf[unicast_off::TTL],
            ttvn: buf[unicast_off::TTVN],
            dest: Mac::read_at(buf, unicast_off::DEST),
        })
    }
}

// ---------------------------------------------------------------------------
// OGM packet (B.A.T.M.A.N. IV)
// ---------------------------------------------------------------------------
pub const BATADV_NOT_BEST_NEXT_HOP:u8 = 0x01;
pub const BATADV_PRIMARIES_FIRST_HOP: u8 = 0x02;
pub const BATADV_DIRECTLINK:u8 = 0x04;
//pub const BATADV_TQ_MAX_VALUE: u8 = 255;


mod ogm_off {
    pub const TTL: usize = 2;
    pub const FLAGS: usize = 3;
    pub const SEQNO: usize = 4;
    pub const ORIG: usize = 8;
    pub const PREV_SENDER: usize = 14;
    pub const RESERVED: usize = 20;
    pub const TQ: usize = 21;
    pub const TVLV_LEN: usize = 22;
}

#[derive(Debug, Clone, Copy,PartialEq, Eq)]
pub struct BatmanAdvOGMPacket {
    packet_type: u8,
    version: u8,
    ttl: u8,
    flags: u8,
    seqno: u32,
    orig: Mac,
    prev_sender: Mac,
    reserved: u8,
    tq: u8,
    tvlv_len: u16,
}
/// OGM Packet struct that will always be eth_header + OGM_header
impl BatmanAdvOGMPacket {
    /// Builds a OGM packet originated by this node
    pub fn new(orig: Mac, seqno: u32) -> Self {
        Self {
            packet_type: BATADV_IV_OGM,
            version: BATADV_COMPAT_VERSION,
            ttl: BATADV_TTL,
            flags: BATADV_PRIMARIES_FIRST_HOP,
            seqno,
            orig: orig,
            prev_sender: orig,
            reserved: 0,
            tq: BATADV_TQ_MAX_VALUE,
            tvlv_len: 0,
        }
    }

    pub fn packet_type(&self)->u8{
        self.packet_type
    }
    pub fn version(&self)->u8{
        self.version
    }
    pub fn ttl(&self)->u8{
        self.ttl
    }
    pub fn flags(&self)->u8{
        self.flags
    }
    pub fn seqno(&self)->u32{
        self.seqno
    }
    pub fn originator(&self)->Mac{
        self.orig
    }
    pub fn prev_sender(&self)->Mac{
        self.prev_sender
    }
    pub fn reserved(&self)->u8{
        self.reserved
    }
    pub fn tq(&self)->u8{
        self.tq
    }
    /// The TVLV data following the header, or `None` when `buf` holds fewer
    /// octets than `tvlv_len` promises.
   // pub fn tvlv<'a>(&self, buf: &'a [u8]) -> Option<&'a [u8]> {
   //     buf.get(Self::WIRE_LEN..self.total_len())
    //}

    pub fn tvlv_len(&self)->u16{
        self.tvlv_len
    }

    pub fn set_flag(&mut self, flag: u8) {
        self.flags |= flag;
    }

    pub fn clear_flag(&mut self, flag: u8) {
        self.flags &= !flag;
    }
    pub fn clear_flags(&mut self){
        self.flags=0
    }
    pub fn has_flag(&self, flag: u8) -> bool {
        (self.flags & flag) == flag
    }

    pub fn decrement_ttl(&mut self) -> bool {
        if self.ttl < 2 {
            return false;
        }
        self.ttl -= 1;
        true
    }

    pub fn set_tq(&mut self, tq:u8){
        self.tq=tq
    }

    pub fn apply_hop_penalty(&mut self, penalty: u8) {
        let max = TQ_MAX as u32;
        self.tq = (self.tq as u32 * (max - penalty as u32) / max) as u8;
    }

    pub fn set_prev_sender(&mut self, sender:Mac){
        self.prev_sender=sender
    }

}

impl Packet for BatmanAdvOGMPacket {
    const WIRE_LEN: usize = 24;
    const PACKET_TYPE: u8 = BATADV_IV_OGM;

    fn write(&self, buf: &mut [u8]) -> Result<(), PacketError> {
        need(buf.len(), Self::WIRE_LEN)?;
        buf[OFF_TYPE] = self.packet_type;
        buf[OFF_VERSION] = self.version;
        buf[ogm_off::TTL] = self.ttl;
        buf[ogm_off::FLAGS] = self.flags;
        write_u32(buf, ogm_off::SEQNO, self.seqno);
        self.orig.write_at(buf, ogm_off::ORIG);
        self.prev_sender.write_at(buf, ogm_off::PREV_SENDER);
        buf[ogm_off::RESERVED] = self.reserved;
        buf[ogm_off::TQ] = self.tq;
        write_u16(buf, ogm_off::TVLV_LEN, self.tvlv_len);
        Ok(())
    }

    fn parse(buf: &[u8]) -> Result<Self, PacketError> {
        check_header(buf, Self::WIRE_LEN, Self::PACKET_TYPE)?;
        // check for non zero TVLV_LEN 
        let tvlv_len = read_u16(buf, ogm_off::TVLV_LEN);
        if tvlv_len != 0{
            return Err(PacketError::UnknownType);
        }
        Ok(Self {
            packet_type: buf[OFF_TYPE],
            version: buf[OFF_VERSION],
            ttl: buf[ogm_off::TTL],
            flags: buf[ogm_off::FLAGS],
            seqno: read_u32(buf, ogm_off::SEQNO),
            orig: Mac::read_at(buf, ogm_off::ORIG),
            prev_sender: Mac::read_at(buf, ogm_off::PREV_SENDER),
            reserved: buf[ogm_off::RESERVED],
            tq: buf[ogm_off::TQ],
            tvlv_len,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::consts::HOP_PENALTY;

    use super::*;

    // --- test helpers ---

    const ORIG: [u8; 6] = [0x02, 0x11, 0x22, 0x33, 0x44, 0x55];
    const PREV: [u8; 6] = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    const DEST: [u8; 6] = [0x02, 0x66, 0x77, 0x88, 0x99, 0x00];
    const SEQNO: u32 = 0x0102_0304;


    fn mac(bytes: [u8; 6]) -> Mac {
        Mac::from_octets(bytes)
    }

    /// wire bytes of BatmanAdvBroadcastPacket::new(mac(ORIG), SEQNO)
    fn bcast_bytes() -> [u8; 14] {
        [
            BATADV_BCAST, BATADV_COMPAT_VERSION, BATADV_TTL, 0x00,
            0x01, 0x02, 0x03, 0x04,
            ORIG[0], ORIG[1], ORIG[2], ORIG[3], ORIG[4], ORIG[5],
        ]
    }

    /// wire bytes of BatmanAdvUnicastPacket::new(mac(DEST))
    fn unicast_bytes() -> [u8; 10] {
        [
            BATADV_UNICAST, BATADV_COMPAT_VERSION, BATADV_TTL, 0x00,
            DEST[0], DEST[1], DEST[2], DEST[3], DEST[4], DEST[5],
        ]
    }

    /// wire bytes of BatmanAdvOGMPacket::new(mac(ORIG), SEQNO)
    fn ogm_bytes() -> [u8; 24] {
        [
            BATADV_IV_OGM, BATADV_COMPAT_VERSION, BATADV_TTL, BATADV_PRIMARIES_FIRST_HOP,
            0x01, 0x02, 0x03, 0x04,
            ORIG[0], ORIG[1], ORIG[2], ORIG[3], ORIG[4], ORIG[5],
            ORIG[0], ORIG[1], ORIG[2], ORIG[3], ORIG[4], ORIG[5], // prev_sender = orig
            0x00,                                                 // reserved
            BATADV_TQ_MAX_VALUE,                                  // tq
            0x00, 0x00,                                           // tvlv_len
        ]
    }

    #[test]
    fn constants_test(){
        // batman-adv v15 wire values
        assert_eq!(ETH_P_BATMAN, 0x4305);
        assert_eq!(BATADV_COMPAT_VERSION, 15);
        assert_eq!(BATADV_IV_OGM, 0x00);
        assert_eq!(BATADV_BCAST, 0x01);
        assert_eq!(BATADV_UNICAST, 0x40);
        assert_eq!(BATADV_NOT_BEST_NEXT_HOP, 0x01);
        assert_eq!(BATADV_PRIMARIES_FIRST_HOP, 0x02);
        assert_eq!(BATADV_DIRECTLINK, 0x04);

        // PACKET_TYPE matches type consts
        assert_eq!(BatmanAdvBroadcastPacket::PACKET_TYPE, BATADV_BCAST);
        assert_eq!(BatmanAdvUnicastPacket::PACKET_TYPE, BATADV_UNICAST);
        assert_eq!(BatmanAdvOGMPacket::PACKET_TYPE, BATADV_IV_OGM);

        // header sizes
        assert_eq!(BatmanAdvBroadcastPacket::WIRE_LEN, 14);
        assert_eq!(BatmanAdvUnicastPacket::WIRE_LEN, 10);
        assert_eq!(BatmanAdvOGMPacket::WIRE_LEN, 24);

        // last field ends exactly at WIRE_LEN
        assert_eq!(bcast_off::ORIG + 6, BatmanAdvBroadcastPacket::WIRE_LEN);
        assert_eq!(unicast_off::DEST + 6, BatmanAdvUnicastPacket::WIRE_LEN);
        assert_eq!(ogm_off::TVLV_LEN + 2, BatmanAdvOGMPacket::WIRE_LEN);

        // OGM fields contiguous, no gaps / overlaps
        assert_eq!(ogm_off::SEQNO + 4, ogm_off::ORIG);
        assert_eq!(ogm_off::ORIG + 6, ogm_off::PREV_SENDER);
        assert_eq!(ogm_off::PREV_SENDER + 6, ogm_off::RESERVED);
        assert_eq!(ogm_off::RESERVED + 1, ogm_off::TQ);
        assert_eq!(ogm_off::TQ + 1, ogm_off::TVLV_LEN);

        //TODO: Fix so ever function uses the same const
        // new() uses BATADV_TQ_MAX_VALUE, apply_hop_penalty uses TQ_MAX: must agree
        assert_eq!(TQ_MAX as u32, BATADV_TQ_MAX_VALUE as u32);

    }

    #[test]
    fn need_test(){
        assert_eq!(need(0, 0), Ok(()));
        assert_eq!(need(5, 5), Ok(()));
        assert_eq!(need(6, 5), Ok(()));
        assert_eq!(need(4, 5), Err(PacketError::TooShort { need: 5, got: 4 }));
        assert_eq!(need(0, 1), Err(PacketError::TooShort { need: 1, got: 0 }));

    }

    #[test]
    fn peek_type_test(){
        assert_eq!(peek_type(&[]), None);
        assert_eq!(peek_type(&[BATADV_UNICAST]), Some(BATADV_UNICAST));
        assert_eq!(peek_type(&ogm_bytes()), Some(BATADV_IV_OGM));
        assert_eq!(peek_type(&bcast_bytes()), Some(BATADV_BCAST));

        // no validation: any byte returned
        assert_eq!(peek_type(&[0xFF, 0x00]), Some(0xFF));

    }

    #[test]
    fn peek_version_test(){
        assert_eq!(peek_version(&[]), None);
        assert_eq!(peek_version(&[BATADV_BCAST]), None);
        assert_eq!(peek_version(&bcast_bytes()), Some(BATADV_COMPAT_VERSION));

        // no validation: any byte returned
        assert_eq!(peek_version(&[0x00, 0xFF]), Some(0xFF));

    }

    #[test]
    fn check_header_test(){
        let mut buf = [0u8; 10];
        buf[OFF_TYPE] = BATADV_UNICAST;
        buf[OFF_VERSION] = BATADV_COMPAT_VERSION;

        // exact len wanted
        assert_eq!(check_header(&buf, 10, BATADV_UNICAST), Ok(()));
        // TODO: see if we need to limit this functionality
        // shorter then wanted
        assert_eq!(check_header(&buf, 8, BATADV_UNICAST), Ok(()));

        // longer than wanted
        assert_eq!(
            check_header(&buf, 11, BATADV_UNICAST),
            Err(PacketError::TooLong { need: 11, got: 10 })
        );

        // length checked before type
        assert_eq!(
            check_header(&buf, 11, BATADV_BCAST),
            Err(PacketError::TooLong { need: 11, got: 10 })
        );

        // wrong type
        assert_eq!(
            check_header(&buf, 10, BATADV_BCAST),
            Err(PacketError::WrongType {
                expected: BATADV_BCAST as u16,
                got: BATADV_UNICAST as u16,
            })
        );

        // type checked before version
        buf[OFF_VERSION] = 14;
        assert_eq!(
            check_header(&buf, 10, BATADV_BCAST),
            Err(PacketError::WrongType {
                expected: BATADV_BCAST as u16,
                got: BATADV_UNICAST as u16,
            })
        );

        // bad version
        assert_eq!(check_header(&buf, 10, BATADV_UNICAST), Err(PacketError::BadVersion(14)));

    }

    #[test]
    fn wire_len_test(){
        assert_eq!(
            BatmanAdvBroadcastPacket::new(mac(ORIG), 0).wire_len(),
            BatmanAdvBroadcastPacket::WIRE_LEN
        );
        assert_eq!(
            BatmanAdvUnicastPacket::new(mac(DEST)).wire_len(),
            BatmanAdvUnicastPacket::WIRE_LEN
        );
        assert_eq!(
            BatmanAdvOGMPacket::new(mac(ORIG), 0).wire_len(),
            BatmanAdvOGMPacket::WIRE_LEN
        );

    }

    #[test]
    fn parse_cross_type_test(){
        // bcast bytes long enough for unicast: type mismatch
        assert_eq!(
            BatmanAdvUnicastPacket::parse(&bcast_bytes()),
            Err(PacketError::WrongType {
                expected: BATADV_UNICAST as u16,
                got: BATADV_BCAST as u16,
            })
        );

        // unicast bytes too short for bcast: length wins
        assert_eq!(
            BatmanAdvBroadcastPacket::parse(&unicast_bytes()),
            Err(PacketError::TooShort { need: 14, got: 10 })
        );

        // ogm bytes into bcast parser
        assert_eq!(
            BatmanAdvBroadcastPacket::parse(&ogm_bytes()),
            Err(PacketError::WrongType {
                expected: BATADV_BCAST as u16,
                got: BATADV_IV_OGM as u16,
            })
        );

    }

    // --- Broadcast tests ---
    #[test]
    fn bcast_new_test(){
        let p = BatmanAdvBroadcastPacket::new(mac(ORIG), SEQNO);
        assert_eq!(p.packet_type(), BATADV_BCAST);
        assert_eq!(p.version(), BATADV_COMPAT_VERSION);
        assert_eq!(p.ttl(), BATADV_TTL);
        assert_eq!(p.reserved(), 0);
        assert_eq!(p.seqno(), SEQNO);
        assert_eq!(p.originator(), mac(ORIG));

    }

        #[test]
    fn bcast_decrement_ttl_test(){
        let base = BatmanAdvBroadcastPacket::new(mac(ORIG), SEQNO);
        let mut p = base;

        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), BATADV_TTL - 1);

        // 2 -> 1 allowed
        p.ttl = 2;
        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), 1);

        // 1: refused, unchanged
        assert!(!p.decrement_ttl());
        assert_eq!(p.ttl(), 1);

        // 0: refused, no underflow
        p.ttl = 0;
        assert!(!p.decrement_ttl());
        assert_eq!(p.ttl(), 0);

        p.ttl = u8::MAX;
        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), u8::MAX - 1);

        // only ttl touched
        let mut q = base;
        q.decrement_ttl();
        q.ttl = base.ttl;
        assert_eq!(q, base);

    }

    #[test]
    fn bcast_write_test(){
        let p = BatmanAdvBroadcastPacket::new(mac(ORIG), SEQNO);

        // exact layout, seqno big-endian
        let mut buf = [0xEEu8; 16];
        assert_eq!(p.write(&mut buf), Ok(()));
        assert_eq!(&buf[..14], &bcast_bytes()[..]);
        // bytes past header untouched
        assert_eq!(&buf[14..], &[0xEEu8; 2]);

        // exact-size buffer
        let mut exact = [0u8; 14];
        assert_eq!(p.write(&mut exact), Ok(()));
        assert_eq!(exact, bcast_bytes());

        // too short: error, buffer untouched
        let mut short = [0u8; 13];
        assert_eq!(p.write(&mut short), Err(PacketError::TooShort { need: 14, got: 13 }));
        assert_eq!(short, [0u8; 13]);

        let mut empty: [u8; 0] = [];
        assert_eq!(p.write(&mut empty), Err(PacketError::TooShort { need: 14, got: 0 }));

    }

    #[test]
    fn bcast_parse_test(){
        let expected = BatmanAdvBroadcastPacket::new(mac(ORIG), SEQNO);
        assert_eq!(BatmanAdvBroadcastPacket::parse(&bcast_bytes()), Ok(expected));

        // trailing payload ignored
        let mut long = [0xEEu8; 20];
        long[..14].copy_from_slice(&bcast_bytes());
        assert_eq!(BatmanAdvBroadcastPacket::parse(&long), Ok(expected));

        // non-default fields carried through
        let mut raw = bcast_bytes();
        raw[bcast_off::TTL] = 7;
        raw[bcast_off::RESERVED] = 0x5A;
        let p = BatmanAdvBroadcastPacket::parse(&raw).unwrap();
        assert_eq!(p.ttl(), 7);
        assert_eq!(p.reserved(), 0x5A);

        // too short
        assert_eq!(
            BatmanAdvBroadcastPacket::parse(&bcast_bytes()[..13]),
            Err(PacketError::TooShort { need: 14, got: 13 })
        );
        assert_eq!(
            BatmanAdvBroadcastPacket::parse(&[]),
            Err(PacketError::TooShort { need: 14, got: 0 })
        );

        // wrong type
        let mut raw = bcast_bytes();
        raw[OFF_TYPE] = BATADV_UNICAST;
        assert_eq!(
            BatmanAdvBroadcastPacket::parse(&raw),
            Err(PacketError::WrongType {
                expected: BATADV_BCAST as u16,
                got: BATADV_UNICAST as u16,
            })
        );

        // bad version
        let mut raw = bcast_bytes();
        raw[OFF_VERSION] = 14;
        assert_eq!(BatmanAdvBroadcastPacket::parse(&raw), Err(PacketError::BadVersion(14)));

    }

    #[test]
    fn bcast_round_trip_test(){
        for &seqno in [0u32, 1, SEQNO, u32::MAX].iter() {
            let mut p = BatmanAdvBroadcastPacket::new(mac(ORIG), seqno);
            p.decrement_ttl();
            let mut buf = [0u8; 14];
            assert_eq!(p.write(&mut buf), Ok(()));
            assert_eq!(BatmanAdvBroadcastPacket::parse(&buf), Ok(p));
        }
    }

    // --- Unicast tests ---
    #[test]
    fn unicast_new_test(){
        let p = BatmanAdvUnicastPacket::new(mac(DEST));
        assert_eq!(p.packet_type(), BATADV_UNICAST);
        assert_eq!(p.version(), BATADV_COMPAT_VERSION);
        assert_eq!(p.ttl(), BATADV_TTL);
        assert_eq!(p.ttvn(), 0);
        assert_eq!(p.dest(), mac(DEST));

    }

    #[test]
    fn unicast_decrement_ttl_test(){
        let base = BatmanAdvUnicastPacket::new(mac(DEST));
        let mut p = base;

        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), BATADV_TTL - 1);

        p.ttl = 2;
        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), 1);

        assert!(!p.decrement_ttl());
        assert_eq!(p.ttl(), 1);

        p.ttl = 0;
        assert!(!p.decrement_ttl());
        assert_eq!(p.ttl(), 0);

        // only ttl touched
        let mut q = base;
        q.decrement_ttl();
        q.ttl = base.ttl;
        assert_eq!(q, base);
    }

    #[test]
    fn unicast_write_test(){
        let p = BatmanAdvUnicastPacket::new(mac(DEST));

        let mut buf = [0xEEu8; 12];
        assert_eq!(p.write(&mut buf), Ok(()));
        assert_eq!(&buf[..10], &unicast_bytes()[..]);
        assert_eq!(&buf[10..], &[0xEEu8; 2]);

        // ttvn written at its offset
        let mut q = p;
        q.ttvn = 9;
        let mut buf = [0u8; 10];
        assert_eq!(q.write(&mut buf), Ok(()));
        assert_eq!(buf[unicast_off::TTVN], 9);

        // too short: error, buffer untouched
        let mut short = [0u8; 9];
        assert_eq!(p.write(&mut short), Err(PacketError::TooShort { need: 10, got: 9 }));
        assert_eq!(short, [0u8; 9]);
    }

    #[test]
    fn unicast_parse_test(){
        let expected = BatmanAdvUnicastPacket::new(mac(DEST));
        assert_eq!(BatmanAdvUnicastPacket::parse(&unicast_bytes()), Ok(expected));

        // trailing payload ignored
        let mut long = [0xEEu8; 64];
        long[..10].copy_from_slice(&unicast_bytes());
        assert_eq!(BatmanAdvUnicastPacket::parse(&long), Ok(expected));

        // non-default fields carried through
        let mut raw = unicast_bytes();
        raw[unicast_off::TTL] = 3;
        raw[unicast_off::TTVN] = 9;
        let p = BatmanAdvUnicastPacket::parse(&raw).unwrap();
        assert_eq!(p.ttl(), 3);
        assert_eq!(p.ttvn(), 9);

        // too short
        assert_eq!(
            BatmanAdvUnicastPacket::parse(&unicast_bytes()[..9]),
            Err(PacketError::TooShort { need: 10, got: 9 })
        );

        // wrong type
        let mut raw = unicast_bytes();
        raw[OFF_TYPE] = BATADV_BCAST;
        assert_eq!(
            BatmanAdvUnicastPacket::parse(&raw),
            Err(PacketError::WrongType {
                expected: BATADV_UNICAST as u16,
                got: BATADV_BCAST as u16,
            })
        );

        // bad version
        let mut raw = unicast_bytes();
        raw[OFF_VERSION] = 16;
        assert_eq!(BatmanAdvUnicastPacket::parse(&raw), Err(PacketError::BadVersion(16)));
    }

    #[test]
    fn unicast_round_trip_test(){
        let mut p = BatmanAdvUnicastPacket::new(mac(DEST));
        p.ttvn = 0xFF;
        p.decrement_ttl();
        let mut buf = [0u8; 10];
        assert_eq!(p.write(&mut buf), Ok(()));
        assert_eq!(BatmanAdvUnicastPacket::parse(&buf), Ok(p));
    }

    // --- OGM tests ---
    #[test]
    fn ogm_new_test(){
        let p = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        assert_eq!(p.packet_type(), BATADV_IV_OGM);
        assert_eq!(p.version(), BATADV_COMPAT_VERSION);
        assert_eq!(p.ttl(), BATADV_TTL);
        assert_eq!(p.flags(), BATADV_PRIMARIES_FIRST_HOP);
        assert_eq!(p.seqno(), SEQNO);
        assert_eq!(p.originator(), mac(ORIG));
        assert_eq!(p.prev_sender(), mac(ORIG));
        assert_eq!(p.reserved(), 0);
        assert_eq!(p.tq(), BATADV_TQ_MAX_VALUE);
        assert_eq!(p.tvlv_len(), 0);
    }

    #[test]
    fn ogm_flags_test(){
        let mut p = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        assert!(p.has_flag(BATADV_PRIMARIES_FIRST_HOP));
        assert!(!p.has_flag(BATADV_DIRECTLINK));
        assert!(!p.has_flag(BATADV_NOT_BEST_NEXT_HOP));

        // set adds bit, keeps others
        p.set_flag(BATADV_DIRECTLINK);
        assert_eq!(p.flags(), BATADV_PRIMARIES_FIRST_HOP | BATADV_DIRECTLINK);

        // set idempotent
        p.set_flag(BATADV_DIRECTLINK);
        assert_eq!(p.flags(), BATADV_PRIMARIES_FIRST_HOP | BATADV_DIRECTLINK);

        // multi-bit mask needs every bit
        assert!(p.has_flag(BATADV_PRIMARIES_FIRST_HOP | BATADV_DIRECTLINK));
        assert!(!p.has_flag(BATADV_DIRECTLINK | BATADV_NOT_BEST_NEXT_HOP));

        // clear removes bit, keeps others
        p.clear_flag(BATADV_PRIMARIES_FIRST_HOP);
        assert_eq!(p.flags(), BATADV_DIRECTLINK);

        // clearing unset flag: no-op
        p.clear_flag(BATADV_NOT_BEST_NEXT_HOP);
        assert_eq!(p.flags(), BATADV_DIRECTLINK);

        // clear_flags wipes all
        p.set_flag(BATADV_NOT_BEST_NEXT_HOP);
        p.clear_flags();
        assert_eq!(p.flags(), 0);
        assert!(!p.has_flag(BATADV_DIRECTLINK));

    }

    #[test]
    fn ogm_decrement_ttl_test(){
        let base = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        let mut p = base;

        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), BATADV_TTL - 1);

        p.ttl = 2;
        assert!(p.decrement_ttl());
        assert_eq!(p.ttl(), 1);

        assert!(!p.decrement_ttl());
        assert_eq!(p.ttl(), 1);

        p.ttl = 0;
        assert!(!p.decrement_ttl());
        assert_eq!(p.ttl(), 0);

        // only ttl touched
        let mut q = base;
        q.decrement_ttl();
        q.ttl = base.ttl;
        assert_eq!(q, base);
    }

    #[test]
    fn ogm_set_tq_test(){
        let base = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        let mut p = base;

        p.set_tq(0);
        assert_eq!(p.tq(), 0);
        p.set_tq(128);
        assert_eq!(p.tq(), 128);

        // only tq touched
        p.set_tq(base.tq());
        assert_eq!(p, base);
    }

    #[test]
    fn ogm_set_prev_sender_test(){
        let base = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        let mut p = base;

        p.set_prev_sender(mac(PREV));
        assert_eq!(p.prev_sender(), mac(PREV));
        // originator unaffected
        assert_eq!(p.originator(), mac(ORIG));

        // only prev_sender touched
        p.set_prev_sender(mac(ORIG));
        assert_eq!(p, base);
    }

    #[test]
    fn apply_hop_penalty_test(){
        let orig = Mac::ZERO;
        let mut ogm = BatmanAdvOGMPacket::new(orig, 0);
        assert_eq!(ogm.tq(),255);

        ogm.apply_hop_penalty(HOP_PENALTY);
        assert_eq!(ogm.tq(),225);

        // second hop compounds: 225 * 225 / 255 = 198.5 -> floor
        ogm.apply_hop_penalty(HOP_PENALTY);
        assert_eq!(ogm.tq(), 198);

        // zero penalty: unchanged
        ogm.set_tq(255);
        ogm.apply_hop_penalty(0);
        assert_eq!(ogm.tq(), 255);

        // max penalty: zero
        ogm.set_tq(255);
        ogm.apply_hop_penalty(255);
        assert_eq!(ogm.tq(), 0);

        // zero tq stays zero
        ogm.set_tq(0);
        ogm.apply_hop_penalty(30);
        assert_eq!(ogm.tq(), 0);

        // integer floor: 100 * 225 / 255 = 88.2
        ogm.set_tq(100);
        ogm.apply_hop_penalty(30);
        assert_eq!(ogm.tq(), 88);

        // small tq rounds to zero: 1 * 254 / 255
        ogm.set_tq(1);
        ogm.apply_hop_penalty(1);
        assert_eq!(ogm.tq(), 0);

        // product 255 * 254 must not overflow
        ogm.set_tq(255);
        ogm.apply_hop_penalty(1);
        assert_eq!(ogm.tq(), 254);

        // only tq touched
        let base = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        let mut p = base;
        p.apply_hop_penalty(30);
        p.tq = base.tq;
        assert_eq!(p, base);
    }

    #[test]
    fn ogm_write_test(){
        // default packet matches reference bytes
        let p = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        let mut buf = [0u8; 24];
        assert_eq!(p.write(&mut buf), Ok(()));
        assert_eq!(buf, ogm_bytes());

        // every field distinct, checks offsets + byte order
        let mut q = p;
        q.set_prev_sender(mac(PREV));
        q.set_flag(BATADV_DIRECTLINK);
        q.set_tq(0xAB);
        q.reserved = 0x5A;
        q.tvlv_len = 0x0102; // no setter; checks u16 big-endian
        let expected: [u8; 24] = [
            BATADV_IV_OGM, BATADV_COMPAT_VERSION, BATADV_TTL,
            BATADV_PRIMARIES_FIRST_HOP | BATADV_DIRECTLINK,
            0x01, 0x02, 0x03, 0x04,
            ORIG[0], ORIG[1], ORIG[2], ORIG[3], ORIG[4], ORIG[5],
            PREV[0], PREV[1], PREV[2], PREV[3], PREV[4], PREV[5],
            0x5A,
            0xAB,
            0x01, 0x02,
        ];
        let mut buf = [0xEEu8; 26];
        assert_eq!(q.write(&mut buf), Ok(()));
        assert_eq!(&buf[..24], &expected[..]);
        // bytes past header untouched
        assert_eq!(&buf[24..], &[0xEEu8; 2]);

        // too short: error, buffer untouched
        let mut short = [0u8; 23];
        assert_eq!(p.write(&mut short), Err(PacketError::TooShort { need: 24, got: 23 }));
        assert_eq!(short, [0u8; 23]);

    }

    #[test]
    fn ogm_parse_test(){
        let expected = BatmanAdvOGMPacket::new(mac(ORIG), SEQNO);
        assert_eq!(BatmanAdvOGMPacket::parse(&ogm_bytes()), Ok(expected));

        // trailing payload ignored
        let mut long = [0xEEu8; 40];
        long[..24].copy_from_slice(&ogm_bytes());
        assert_eq!(BatmanAdvOGMPacket::parse(&long), Ok(expected));

        // non-default fields carried through
        let mut raw = ogm_bytes();
        raw[ogm_off::FLAGS] = BATADV_DIRECTLINK;
        raw[ogm_off::PREV_SENDER..ogm_off::PREV_SENDER + 6].copy_from_slice(&PREV);
        raw[ogm_off::RESERVED] = 0x5A;
        raw[ogm_off::TQ] = 0x10;
        let p = BatmanAdvOGMPacket::parse(&raw).unwrap();
        assert_eq!(p.flags(), BATADV_DIRECTLINK);
        assert_eq!(p.prev_sender(), mac(PREV));
        assert_eq!(p.originator(), mac(ORIG));
        assert_eq!(p.reserved(), 0x5A);
        assert_eq!(p.tq(), 0x10);

        // nonzero tvlv_len (either byte): rejected
        let mut raw = ogm_bytes();
        raw[ogm_off::TVLV_LEN + 1] = 0x01;
        assert_eq!(BatmanAdvOGMPacket::parse(&raw), Err(PacketError::UnknownType));
        let mut raw = ogm_bytes();
        raw[ogm_off::TVLV_LEN] = 0x01;
        assert_eq!(BatmanAdvOGMPacket::parse(&raw), Err(PacketError::UnknownType));

        // header checks before tvlv check
        raw[OFF_VERSION] = 14;
        assert_eq!(BatmanAdvOGMPacket::parse(&raw), Err(PacketError::BadVersion(14)));

        // too short
        assert_eq!(
            BatmanAdvOGMPacket::parse(&ogm_bytes()[..23]),
            Err(PacketError::TooShort { need: 24, got: 23 })
        );

        // wrong type
        let mut raw = ogm_bytes();
        raw[OFF_TYPE] = BATADV_BCAST;
        assert_eq!(
            BatmanAdvOGMPacket::parse(&raw),
            Err(PacketError::WrongType {
                expected: BATADV_IV_OGM as u16,
                got: BATADV_BCAST as u16,
            })
        );

        // bad version
        let mut raw = ogm_bytes();
        raw[OFF_VERSION] = 14;
        assert_eq!(BatmanAdvOGMPacket::parse(&raw), Err(PacketError::BadVersion(14)));

    }
        
    #[test]
    fn ogm_round_trip_test(){
        for &seqno in [0u32, SEQNO, u32::MAX].iter() {
            let mut p = BatmanAdvOGMPacket::new(mac(ORIG), seqno);
            p.set_prev_sender(mac(PREV));
            p.set_flag(BATADV_DIRECTLINK);
            p.apply_hop_penalty(HOP_PENALTY);
            p.decrement_ttl();
            let mut buf = [0u8; 24];
            assert_eq!(p.write(&mut buf), Ok(()));
            assert_eq!(BatmanAdvOGMPacket::parse(&buf), Ok(p));
        }
    }

}