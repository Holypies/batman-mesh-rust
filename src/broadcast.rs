
use crate::FrameError::TooLarge;
use crate::InterfaceError::InterfaceDown;
use crate::framebuffer::{FrameBuffer,FrameError};
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
    

    #[test]
    fn pad_to_min_frame_test(){

    }
    
    #[test]
    fn build_bcast_frame_test(){

    }

    #[test]
    fn classify_bcast_test(){

    }

    #[test]
    fn process_bcast_test(){

    }

}
