use crate::meshctx::DropReason;
use crate::hardif::{HardInterface, LinkDriver};
use crate::originatortable::OriginatorTable;
use crate::mac::Mac;
use crate::packets::{BatmanAdvBroadcastPacket, Packet, ETH_P_BATMAN};
use crate::time::Instant;
use crate::consts::ETH_HLEN;
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
 
    match classify_bcast(my_mac, &frame[ETH_HLEN..]) {
        BroadcastType::Invalid => {
            hardif.record_rx_dropped();
            return Err(DropReason::InvalidBcast);
        }
        BroadcastType::Normal { mut bcast } => {
            let sender_orig = match orig_table.find_mac_mut(bcast.originator()) {
                Some(orig) => orig,
                Option::None => {
                    hardif.record_rx_dropped();
                    return Err(DropReason::UnknownOriginator);
                }
            };
 
            let is_not_dup = sender_orig.check_bcast(bcast.seqno(), now);
 
            if !is_not_dup {
                hardif.record_rx_dropped();
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
    // check if vaild (not multicast or all 0)
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

