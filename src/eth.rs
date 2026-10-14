use crate::mac::Mac;
use crate::packets::PacketError;
use crate::bytes::*;
use crate::consts::{ETH_HLEN};
    //ETH_VLAN_HLEN, ETH_P_8021Q,VLAN_VID_MASK}; used in EthInfo which is not implemented


pub const BATADV_VLAN_HAS_TAG:u16 = 1 << 15;

pub enum EthError {
    NotValidSource,
    WrongType{expected: u16, got:u16},
}

// the index of the destination, source, and type in ethernet header
mod offset{
    pub const ETH_DEST: usize = 0;
    pub const ETH_SOURCE: usize = 6;
    pub const ETH_TYPE: usize = 12;
  //  pub const VLAN_TCI: usize = 14;       used in EthInfo which is not implemented
  //  pub const VLAN_ETH_TYPE: usize = 16;
}
/* 
#[derive(Debug, Clone, Copy)]
pub struct EthInfo{
    header: EthHeader,
    inner_type: u16,
    vid: Option<u16>,
    // either 14 or 18 depeniding on VLAN or not
    hlen: usize,
}


impl EthInfo{
    pub fn payload<'a>(&self, buf: &'a [u8]) -> Result<&'a [u8], PacketError> {
        if buf.len() < self.hlen {
            return Err(PacketError::TooShort { need: self.hlen, got: buf.len() });
        }
        Ok(&buf[self.hlen..])
    }

    pub fn batadv_vid(&self) -> u16 {
        match self.vid {
            Some(vid) => vid | BATADV_VLAN_HAS_TAG,
            None => 0,
        }
    }


}*/


#[derive(Debug, Copy, Clone)]
pub struct EthHeader{
    dest: Mac,
    source: Mac,
    eth_type: u16,
}

impl EthHeader{

    /// Create a new ethernet header
    /// 
    /// Input:
    ///     dest:       48 bit MAC adress of the destination node
    ///     source:     48 bit MAC adress source node
    ///     eth_type:   u16 int that represent the Ethernet type for the header
    /// 
    /// Returns:
    ///     Self:       New EthHeader with source, dest, and eth_type 
    ///                 defined from the input
    pub const fn new(dest:Mac,source: Mac, eth_type:u16)->Self{
        Self{
            
            dest:dest,
            source:source,
            eth_type:eth_type,
        }

    }

    // --- getters ---

    /// Returns imutable ethernet header destination piont (MAC address)
    pub fn dest(&self)->Mac{
        self.dest
    }  
    /// Returns imutable ethernet header source point, (MAC address)
    pub fn source(&self)->Mac{
        self.source
    }  
    /// Returns imutable ethernet type
    pub fn eth_type(&self)->u16{
        self.eth_type
    }  


    // --- functions ---

    /// Parses a Ethernet Header that was not created on this node
    ///    
    /// Input:
    ///     buf: MAC buffer
    /// 
    /// Returns:
    ///     - if buf length is valid: gives back a EthHeader with data from buf
    ///     - otherwise: Returns PacketError::TooShort
    pub fn parse(buf: &[u8])->Result<Self,PacketError>{
        if buf.len()<ETH_HLEN{
            return Err(PacketError::TooShort{need:ETH_HLEN, got: buf.len()});
        }
        Ok(Self{
            dest: Mac::read_at(buf, offset::ETH_DEST),
            source: Mac::read_at(buf, offset::ETH_SOURCE),
            eth_type: read_u16(buf, offset::ETH_TYPE),
        })
    }

    /// Writes the source, destination, and Ethetnet type into a borrowed [u8]
    /// 
    /// Input:
    ///     buf: mutable borrowed MAC... 
    /// 
    /// Returns:
    ///     - if 'buf' length is valid: Ok()
    ///     - otherwise: PacketError::TooShort
    pub fn write(&self, buf:&mut [u8])->Result<(),PacketError>{
        if buf.len()<ETH_HLEN{
            return Err(PacketError::TooShort{need:ETH_HLEN, got: buf.len()});
        }
        self.source.write_at(buf, offset::ETH_SOURCE);
        self.dest.write_at(buf, offset::ETH_DEST);
        write_u16(buf, offset::ETH_TYPE, self.eth_type);
        Ok(())
    }
/* 
    /// Reads front of client frame looking one level past an 802.1Q
    /// tag when there is one. ONLY USE ON INNER HEADERS
    /// 
    /// Input:
    ///     buf: MAC buffer
    /// 
    /// Returns:
    
    pub fn inspect(buf: &[u8])-> Result<EthInfo,PacketError>{
        if buf.len()<ETH_HLEN{
            return Err(PacketError::TooShort{need:ETH_HLEN, got: buf.len()});
        }
        let header = EthHeader::parse(buf)?;
        if header.eth_type()!=ETH_P_8021Q{
            return Ok(EthInfo {
                header,
                inner_type: header.eth_type(),
                vid: None,
                hlen: ETH_HLEN,
            });
        }

        if buf.len()<ETH_VLAN_HLEN{
            return Err(PacketError::TooShort{need: ETH_VLAN_HLEN, got: buf.len()});
        }

        let tci = read_u16(buf, offset::VLAN_TCI);

        Ok(EthInfo {
            header: header,
            inner_type: read_u16(buf, offset::VLAN_ETH_TYPE),
            vid: Some(tci & VLAN_VID_MASK),
            hlen: ETH_VLAN_HLEN,
        })

    }*/

}

#[cfg(test)]
mod test{
    use super::*;

    //const _buf:&[u8] =&[0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF]; 

    const DEST: Mac = Mac::from_octets([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    const SOURCE: Mac = Mac::from_octets([0x02, 0x11, 0x22, 0x33, 0x44, 0x55]);
    

    #[test]
    fn eth_header_constructor_test(){
        let header: EthHeader = EthHeader::new(DEST, SOURCE, 1);

        assert_eq!(header.dest, DEST);
        assert_eq!(header.source, SOURCE);
        assert_eq!(header.eth_type, 1);
    }

    // --- getters ---
    #[test]
    fn dest_getter_test(){
        let header: EthHeader = EthHeader::new(DEST, SOURCE, 1);
        let a: Mac = header.dest();
        assert_eq!(header.dest, a);
    }
    #[test]
    fn source_getter_test(){
        let header: EthHeader = EthHeader::new(DEST, SOURCE, 1);
        let a: Mac = header.source();
        assert_eq!(header.source, a);
    }
    #[test]
    fn eth_type_getter_test(){
        let header: EthHeader = EthHeader::new(DEST, SOURCE, 1);
        let a: u16 = header.eth_type();
        assert_eq!(header.eth_type, a);
    }

    // --- functions ---
    // TODO: implement tests
    #[test]
    fn parse_test(){

        
    }

    #[test]
    fn write_test(){

    }

    #[test]
    fn inspect_test(){

    }
    
}
