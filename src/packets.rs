use crate::mac::Mac;
use crate::bytes::*;

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
const BATADV_TQ_MAX_VALUE:u32 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketError {
    TooShort { need: usize, got: usize },
    WrongType { expected: u8, got: u8 },
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
        Err(PacketError::TooShort {
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
            expected: packet_type,
            got: buf[OFF_TYPE],
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
pub const BATADV_TQ_MAX_VALUE: u8 = 255;


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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn set_flag(&mut self, flags:u8){
        self.flags=flags;
    }

    pub fn clear_flag(&mut self){
        self.flags=0
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

    pub fn apply_hop_penalty(&mut self, penalty:u8){
        self.tq = ((self.tq as u32)*((BATADV_TQ_MAX_VALUE-(penalty as u32)) as u32)/BATADV_TQ_MAX_VALUE) as u8;
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
    use super::*;

}