use crate::packets::{Packet, BatmanAdvUnicastPacket, ETH_P_BATMAN};
use crate::meshctx::DropReason;
use crate::hardif::{HardInterface, LinkDriver};
use crate::originatortable::OriginatorTable;
use crate::time::Instant;
use crate::mac::Mac;
use crate::consts::ETH_HLEN;
use crate::eth::EthHeader;


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
    now: Instant,
) -> Result<Option<&'f [u8]>, DropReason> {
    let my_mac: Mac = hardif.hardware_addr();
 
    match classify_unicast(my_mac, &frame[ETH_HLEN..]) {
        
        UnicastType::Invalid => {
            hardif.record_rx_dropped();
            return Err(DropReason::InvalidUnicast);
        }
        UnicastType::ForMe => {
            // return only the ethernet frame playload
            let frame: &'f [u8] = frame;
            Ok(Some(&frame[UNICAST_INNER_OFFSET..]))
        },
        UnicastType::Forward { mut unicast } => {
            
            if !unicast.decrement_ttl() {
                hardif.record_rx_dropped();
                return Err(DropReason::TtlExceeded);
            }

            // find 
            let dest_orig = match orig_table.find_mac(unicast.dest()) {
                Some(orig) => orig,
                Option::None => {
                    hardif.record_rx_dropped();
                    return Err(DropReason::NoRoute);
                }
            };

            // find router addrss
            let router_mac = match dest_orig.router_id() {
                Some(mac) => mac,
                Option::None => {
                    hardif.record_rx_dropped();
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
    if payload.len()<14{
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



pub fn send_unicast(){}