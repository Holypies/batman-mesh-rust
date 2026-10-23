use core::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Mac([u8; 6]);

impl Mac {
    /// Octets in an Ethernet address
    pub const LEN: usize = 6;
    pub const BROADCAST: Mac = Mac([0xFF; 6]);
    pub const ZERO: Mac = Mac([0x00; 6]);

    /// Spanning tree protocol address. batman-adv refuses to carry frames
    /// sent to it
    pub const STP: Mac = Mac([0x01, 0x80, 0xC2, 0x00, 0x00, 0x00]);

    /// Ethernet Configuration Testing Protocol address, refused for the
    /// same reason as `STP`
    pub const ECTP: Mac = Mac([0xCF, 0x00, 0x00, 0x00, 0x00, 0x00]);

    
    /// Low bit of the first octet: set for multicast and broadcast.
    pub const GROUP_BIT: u8 = 0x01;
    /// Second bit of the first octet: set for locally administered addresses.
    pub const LOCAL_BIT: u8 = 0x02;

    /// Builds an address from six octets in wire order
    pub const fn from_octets(octets: [u8; 6]) -> Self {
        Mac(octets)
    }

    pub const fn from_random_bytes(bytes: [u8; 6]) -> Self {
        Mac([
            (bytes[0] & !Self::GROUP_BIT) | Self::LOCAL_BIT,
            bytes[1],
            bytes[2],
            bytes[3],
            bytes[4],
            bytes[5],
        ])
    }
 /* 
    pub const fn derive_local(&self) -> Self {
        let o = &self.0;
        Mac([
            ((o[0] & !Self::GROUP_BIT) | Self::LOCAL_BIT) ^ Self::DERIVE_BIT,
            o[1],
            o[2],
            o[3],
            o[4],
            o[5],
        ])
    }

*/
    /// Copies an address out of the front of `buf`.
    pub fn from_slice(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::LEN {
            return None;
        }
        let mut octets = [0u8; Self::LEN];
        octets.copy_from_slice(&buf[..Self::LEN]);
        Some(Mac(octets))
    }

    /// The octets in wire order.
    pub const fn as_octets(&self) -> &[u8; 6] {
        &self.0
    }

    /// Copies an address out of `buf` starting at `off`.
    pub fn read_at(buf: &[u8], off: usize) -> Self {
        let mut octets = [0u8; Self::LEN];
        octets.copy_from_slice(&buf[off..off + Self::LEN]);
        Mac(octets)
    }

    /// Writes the address into `buf` starting at `off`, in wire order.
    /// Same precondition as `read_at`
    pub fn write_at(&self, buf: &mut [u8], off: usize) {
        buf[off..off + Self::LEN].copy_from_slice(&self.0);
    }

    /// True when every octet equals `value`
    const fn all_octets(&self, value: u8) -> bool {
        let o = &self.0;
        o[0] == value
            && o[1] == value
            && o[2] == value
            && o[3] == value
            && o[4] == value
            && o[5] == value
    }

    /// True for group addresses. The I/G bit is the low bit of the first
    /// octet, so this also covers broadcast
    pub const fn is_multicast(&self) -> bool {
        self.0[0] & 0x01 != 0
    }

    pub const fn is_unicast(&self) -> bool {
        !self.is_multicast()
    }

    pub const fn is_broadcast(&self) -> bool {
        self.all_octets(0xFF)
    }

    pub const fn is_zero(&self) -> bool {
        self.all_octets(0x00)
    }

    pub const fn is_valid(&self) -> bool {
        !self.is_multicast() && !self.is_zero()
    }
/* 
    pub const fn is_locally_administered(&self) -> bool {
        self.0[0] & Self::LOCAL_BIT != 0
    }

*/
}

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let o = &self.0;
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            o[0], o[1], o[2], o[3], o[4], o[5]
        )
    }
}

impl fmt::Debug for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::Mac;
  
    #[test]
    fn from_octets_test(){
        let buf = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let mac =Mac::from_octets(buf);
        assert_eq!(buf,mac.0);
    }

    #[test]
    fn from_random_bytes_test(){
        let mut buf = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let mac =Mac::from_random_bytes(buf);
        buf[0]=(buf[0] & !Mac::GROUP_BIT) | Mac::LOCAL_BIT;
        assert_eq!(buf,mac.0);
    }

    #[test]
    fn from_slice_test(){
        let buf = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let slice = match Mac::from_slice(&buf){
            Some(val)=>val,
            Option::None => Mac::ZERO,
        };
        assert_eq!(slice,  Mac([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]));


        let buf1= [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x00];
        let slice1 = match Mac::from_slice(&buf1){
            Some(val)=>val,
            Option::None => Mac::ZERO,
        };
        assert_eq!(slice1,  Mac::ZERO);

    }

    #[test]
    fn as_octets_test(){
        let mac = Mac::BROADCAST;
        assert_eq!(mac.as_octets(), [0xFF; 6])
    }

    #[test]
    fn read_at_test(){
        // tests offest 0
        let buf = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let read = Mac::read_at(&buf, 0);
        assert_eq!(read,  Mac([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]));

        // tests offset 1
        let buf1 = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];
        let read1 = Mac::read_at(&buf1, 1);
        assert_eq!(read1,  Mac([0x22, 0x33, 0x44, 0x55, 0x66,0x77]));
    }

    #[test]
    fn write_at_test(){
        // tests offest 0
        let mut buf = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let mac = Mac::ZERO;
        mac.write_at(&mut buf, 0);
        assert_eq!(mac, Mac([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]));

        // tests offset 1
        let mut buf1 = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];
        let mac1 = Mac::ZERO;
        mac1.write_at(&mut buf1, 1);
        assert_eq!(mac1,  Mac([0x22, 0x33, 0x44, 0x55, 0x66,0x77]));

    }

    #[test]
    fn all_octets_test(){
        let mac = Mac::ZERO;
        assert_eq!(mac.all_octets(0x00), true);

        let mac2 =Mac::STP;
        assert_eq!(mac2.all_octets(0x00), false);
    }

    #[test]
    fn is_multicast_test(){
        let bcast = Mac::BROADCAST;
        assert_eq!(bcast.is_multicast(),true);

        let non_multicast_bcast = Mac([0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(non_multicast_bcast.is_multicast(), false);
    }

    #[test]
    fn is_unicast_test(){
        let bcast = Mac::BROADCAST;
        assert_eq!(bcast.is_unicast(),false);

        let unicast = Mac([0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(unicast.is_unicast(), true);
    }

    #[test]
    fn is_broadcast_test(){
        let bcast = Mac::BROADCAST;
        assert_eq!(bcast.is_multicast(),true);

        let non_multicast_bcast = Mac([0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(non_multicast_bcast.is_multicast(), false);
    }

    #[test]
    fn is_zero_test(){
        let zero = Mac::ZERO;
        assert_eq!(zero.is_zero(), true);

        let non_zero = Mac([0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(non_zero.is_zero(), false);
    }
    
    #[test]
    fn is_valid_test(){
        let bcast = Mac::BROADCAST;
        assert_eq!(bcast.is_valid(),false);
        let zero = Mac::ZERO;
        assert_eq!(zero.is_valid(), false);

        let non_multicast_bcast = Mac([0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(non_multicast_bcast.is_valid(), true);
    }

}