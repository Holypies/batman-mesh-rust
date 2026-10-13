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