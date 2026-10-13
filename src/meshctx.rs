use crate::PacketError::BadVersion;
use crate::ogm::send_own_ogm;
use crate::time::{self, INSTANT_ZERO, Instant};
use crate::{FrameError, HardInterface, InterfaceError, LinkDriver, PacketError, broadcast, ogm, unicast};
use crate::eth::{EthHeader, EthError};
use crate::mac::Mac;
use crate::originatortable::{OriginatorTable, OriginatorTableError};
use crate::originator::{OriginatorError, Originator};
use crate::packets::{ETH_P_BATMAN, peek_type};
use crate::packets::peek_version;
use crate::packets::{BATADV_COMPAT_VERSION, BATADV_IV_OGM, BATADV_BCAST, BATADV_UNICAST};
use crate::broadcast::process_bcast;
use crate::consts::{BCAST_INNER_FRAME_LEN, ETH_HLEN, ETH_P_8021Q, ORIG_INTER, PURGE_PERIOD, UNICAST_INNER_FRAME_LEN};

pub const MAX_MESH_NODES:usize=8;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    InvalidBcast,
    NotActive,
    TooShort,
    NotBatman(u16),
    InvalidSource,
    OwnFrame,
    WrongDestination,
    UnknownType(u8),
    Packet(PacketError),
    InvalidOgm,
    Duplicate,
    UnknownOriginator,
    NoRoute,
    TtlExceeded,
    InvalidUnicast,
    //Interface(InterfaceError),
    OriginatorTable(OriginatorTableError),
    Originator(OriginatorError),
    Protected,
}
impl From<PacketError> for DropReason {
    fn from(e: PacketError) -> Self {
        DropReason::Packet(e)
    }
}
impl From<OriginatorTableError> for DropReason {
    fn from(e: OriginatorTableError) -> Self {
        DropReason::OriginatorTable(e)
    }
}
impl From<OriginatorError> for DropReason {
    fn from(e: OriginatorError) -> Self {
        DropReason::Originator(e)
    }
}


pub enum TransmitionError{
    FrameBuffer(FrameError),
    Packet(PacketError),
    HardInterface(InterfaceError),
    InvalidDestination,
    NoRoute,
    MeshNotActive,
    TooShort,
    VlanTagged,
    InvalidSource,
    TooLong

}
impl From<PacketError> for TransmitionError {
    fn from(e: PacketError) -> Self {
        TransmitionError::Packet(e)
    }
}
impl From<FrameError> for TransmitionError {
    fn from(e: FrameError) -> Self {
        TransmitionError::FrameBuffer(e)
    }
}
impl From<InterfaceError> for TransmitionError {
    fn from(e: InterfaceError) -> Self {
        TransmitionError::HardInterface(e)
    }
}


pub struct MeshCtx<D:LinkDriver>{
    orig_addr: Mac,
   // software_mac:Mac,                 Needed for TranslationTable
    orig_table: OriginatorTable,
    hard_iface:HardInterface<D>,
  //  trans_table: TranslationTable,    Needed for TranslationTable
  //  ttvn:u8,                          Needed for TranslationTable
    mesh_active: bool,
    bcast_seqno: u32,
    next_ogm:Instant,
    next_purge:Instant,
}

impl<D:LinkDriver> MeshCtx<D> {
    pub fn new(hardif:HardInterface<D>)->Self{
        Self{
            orig_addr: hardif.hardware_addr(),
            orig_table: OriginatorTable::new(),
            hard_iface:hardif,
            //trans_table: TranslationTable::new(),
            //ttvn:0,
            mesh_active: false,
            bcast_seqno: 0,
            next_ogm:INSTANT_ZERO,
            next_purge:INSTANT_ZERO,
        }
    }

    pub fn init(&mut self, orig_hardware_addr: [u8;6]){
        self.orig_addr = Mac::from_octets(orig_hardware_addr);
        //self.software_mac = create_software_addr(orig_hardware_addr);
        //self.org_table

    } 

    pub fn activate(&mut self, now:Instant){
        self.mesh_active = true;
        self.next_ogm=now;
        self.next_purge= now+PURGE_PERIOD;
        self.hard_iface.set_active(true);
    }

    pub fn deactivate(&mut self){
        self.mesh_active = false;
        self.orig_table.purge_all();
        self.hard_iface.set_active(false);
    }

    pub fn handle_frame<'f>(&mut self, frame: &'f mut [u8], now:Instant)->Result<Option<&'f[u8]>,DropReason>{
        self.hard_iface.record_rx(frame.len());


        // if mesh is not active error
        if !self.mesh_active{
            self.hard_iface.record_rx_dropped();
            return Err(DropReason::NotActive);
        } 

        // parse ethernet header, if errors return error
        let eth_header = match EthHeader::parse(frame){
            Ok(header) => header,
            Err(err) => {
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::Packet(err));
            },
        };

        // check if batman
        if eth_header.eth_type() != ETH_P_BATMAN{
            self.hard_iface.record_rx_dropped();
            return Err(DropReason::NotBatman(eth_header.eth_type()));

        }
        // check if frame comes from valid source
        if !eth_header.source().is_valid(){
            self.hard_iface.record_rx_dropped();
            return Err(DropReason::InvalidSource);
        }
        // check if original sender was itself
        if eth_header.source()==self.orig_addr{
            self.hard_iface.record_rx_dropped();
            return Err(DropReason::OwnFrame);

        }

            
        let payload = &frame[ETH_HLEN..];
            
        match peek_version(payload) {
        // if option fails
            Option::None => {
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::TooShort);
            }
            // if version isnt supported
            Some(v) if v != BATADV_COMPAT_VERSION => {
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::Packet(BadVersion(v)));
            }
            Some(_)=>{}
        }

        match peek_type(payload){
            Option::None=>{
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::TooShort)
            },
            
            Some(BATADV_IV_OGM)=>{
                if !eth_header.dest().is_broadcast(){
                    self.hard_iface.record_rx_dropped();
                    return Err(DropReason::WrongDestination);
                }
                match ogm::process_ogm(
                    &mut self.hard_iface,
                    &mut self.orig_table,
                    eth_header.source(),
                    payload,
                    now,
                ) {
                    Ok(()) => {}
                    Err(e) => {
                        self.hard_iface.record_rx_dropped();
                        return Err(e);
                    }
                }
                return Ok(None);

            },
            Some(BATADV_BCAST) =>{
                if !eth_header.dest().is_broadcast(){
                    self.hard_iface.record_rx_dropped();
                    return Err(DropReason::WrongDestination);
                }
                let ethernet_payload = match broadcast::process_bcast(
                    &mut self.hard_iface,
                    &mut self.orig_table,
                    frame,
                    now,
                ) {
                    Ok(payload) => payload,
                    Err(e) => {
                        self.hard_iface.record_rx_dropped();
                        return Err(e);
                    }
                };
                return Ok(Some(ethernet_payload));
            },
            Some(BATADV_UNICAST) => {
                if eth_header.dest() != self.orig_addr {
                    self.hard_iface.record_rx_dropped();
                    return Err(DropReason::WrongDestination);
                }
            
                let ethernet_payload = match unicast::process_unicast(
                    &mut self.hard_iface,
                    &mut self.orig_table,
                    frame,
                ) {
                    Ok(payload) => payload,
                    Err(e) => {
                        self.hard_iface.record_rx_dropped();
                        return Err(e);
                    }
                };
                return Ok(ethernet_payload);


            }
            Some(t)=>{
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::UnknownType(t));
            }

        }

    }

    pub fn send_frame<'f>(&mut self, frame: &'f[u8])->Result<(),TransmitionError>{

        if !self.mesh_active{
            return Err(TransmitionError::MeshNotActive);
        }

        if frame.len() < ETH_HLEN{
            return Err(TransmitionError::TooShort);
        }
        


        let eth_header = match EthHeader::parse(frame){
            Ok(header)=>header,
            Err(e)=>{
                return Err(TransmitionError::Packet(e));
            }
        };

        if eth_header.eth_type() == ETH_P_8021Q{
            return Err(TransmitionError::VlanTagged);
        }
        if eth_header.source() != self.orig_addr{
            return Err(TransmitionError::InvalidSource);
        }
        if eth_header.dest() == Mac::STP || eth_header.dest() == Mac::ECTP{
            return Err(TransmitionError::InvalidDestination);
        }

        if eth_header.dest().is_multicast() || eth_header.dest().is_broadcast(){
            // Unicast frame is a bit larger so we use it instead of unicast
            if frame.len()>BCAST_INNER_FRAME_LEN{
                return Err(TransmitionError::TooLong);
            }
            match broadcast::send_broadcast(&mut self.hard_iface, &mut self.bcast_seqno, frame){
                Ok(())=>return Ok(()),
                Err(e)=>{
                    return Err(e)
                },
            };
        }
        // Unicast frame is a bit larger so we use it instead of unicast
        if frame.len()>UNICAST_INNER_FRAME_LEN{
            return Err(TransmitionError::TooLong);
        }
        match unicast::send_unicast(&mut self.hard_iface, &mut self.orig_table, frame){
            Ok(())=>return Ok(()),
            Err(e)=>{
                return Err(e)
            },
        };

    }   



    pub fn poll(&mut self,now:Instant, rand:u32)->Instant{
        if !self.mesh_active{
            return now+ORIG_INTER;
        }

        if time::has_reached(now, self.next_ogm){
            self.next_ogm = send_own_ogm(
                &mut self.hard_iface, 
                &mut self.orig_table, 
                now, 
                rand
            );
        }

        if time::has_reached(now, self.next_purge){
            self.orig_table.purge(now);
            self.next_purge = now + PURGE_PERIOD;
        }

        return time::earliest(
            Some(self.next_ogm),
            Some(self.next_purge)).unwrap_or(self.next_ogm);

    }
}