use crate::eth::EthHeader;
use crate::framebuffer::FrameBuffer;
use crate::originatortable::OriginatorTable;
use crate::packets::{BatmanAdvOGMPacket, ETH_P_BATMAN, Packet, PacketError};
use crate::mac::Mac;
use crate::time::{Instant, ogm_send_time};
use crate::{FrameError, HardInterface, consts::*};
use crate::hardif::LinkDriver;

// temp constants
pub const OGM_HEADROOM:usize =14;
pub const OGM_FRAME_LEN:usize =60;
pub const MIN_ETH_FRAME_SIZE:usize=60;


pub enum MeshError {
    Frame(FrameError),
    Packet(PacketError),
    //Interface(InterfaceError)

}

impl From<PacketError> for MeshError {
    fn from(e: PacketError) -> Self { MeshError::Packet(e) }
}
impl From<FrameError> for MeshError {
    fn from(e: FrameError) -> Self { MeshError::Frame(e) }
}


pub fn build_ogm_frame(
    ogm_header: BatmanAdvOGMPacket, 
    source_mac: Mac, 
    buffer: &mut FrameBuffer<OGM_FRAME_LEN, OGM_HEADROOM>
)->Result<(), MeshError>{
    buffer.reset_cursors();

    let buf: &mut [u8] = buffer.append(BatmanAdvOGMPacket::WIRE_LEN)?;
    ogm_header.write(buf)?;

    let eth_slot = buffer.prepend(ETH_HLEN)?;
    let eth_header: EthHeader = EthHeader::new(
        Mac::BROADCAST, 
        source_mac, 
        ETH_P_BATMAN
    );
    eth_header.write(eth_slot)?;

    buffer.append(MIN_ETH_FRAME_SIZE.saturating_sub(buffer.len()))?;
    Ok(())
}


pub fn send_own_ogm<D: LinkDriver>(
    hardif:&mut HardInterface<D>,
    orig_table:&mut OriginatorTable, 
    now:Instant,
    rand:u32
)->Instant{
    let seqno = hardif.next_ogm_seqno();
    let orig_mac = hardif.hardware_addr();
    let batman_header: BatmanAdvOGMPacket = BatmanAdvOGMPacket::new(orig_mac, seqno);



    let mut buffer = FrameBuffer::<OGM_FRAME_LEN, OGM_HEADROOM>::new();
    if build_ogm_frame(batman_header, orig_mac, &mut buffer).is_ok() {
        let _ = hardif.transmit(buffer.bytes());
    }

    orig_table.slide_echo_windows();

    return ogm_send_time(now, ORIG_INTER, rand);

}

