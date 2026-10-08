use crate::PacketError::BadVersion;
use crate::consts::{};
use crate::bitwindow::WindowCheck::Accept;
use crate::time::Instant;
use crate::{HardInterface, LinkDriver, PacketError, ogm};
use crate::eth::{EthHeader, EthError};
use crate::mac::Mac;
use crate::originatortable::OriginatorTable;
use crate::packets::{ETH_P_BATMAN, peek_type};
use crate::framebuffer::FrameError;
use crate::packets::peek_version;
use crate::packets::{BATADV_COMPAT_VERSION, BATADV_IV_OGM, BATADV_BCAST, BATADV_UNICAST};


pub const MAX_MESH_NODES:usize=8;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
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
    //Interface(InterfaceError),
}




pub struct MeshCtx<D:LinkDriver>{
    orig_addr: Mac,
    software_mac:Mac,
    orig_table: OriginatorTable,
    hard_iface:HardInterface<D>,
  //  trans_table: TranslationTable,
    ttvn:u8,
    mesh_active: bool,
    bcast_seqno: u32,
}

impl<D:LinkDriver> MeshCtx<D> {
    pub const fn new()->Self{
        let zero_octets:[u8;6] = [0u8;6];
        Self{
            orig_addr: Mac::from_octets(zero_octets),
            software_mac:Mac::from_octets(zero_octets),
            orig_table: OriginatorTable::new(),
            hard_iface:HardInterface,
            //trans_table: TranslationTable::new(),
            ttvn:0,
            mesh_active: false,
            bcast_seqno: 0,
        }
    }

    pub fn init(&mut self, orig_hardware_addr: [u8;6]){
        self.orig_addr = Mac::from_octets(orig_hardware_addr);
        //self.software_mac = create_software_addr(orig_hardware_addr);
        //self.org_table

    } 

    pub fn activate(&mut self, time:u32){
        self.mesh_active = true;

    }

    pub fn deactivate(&mut self){
        self.mesh_active = false;

    }

    pub fn handle_frame<'f>(&mut self, interface_id:u8, frame: &'f[u8], now:Instant)->Result<(),DropReason>{
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

            
        let payload = &frame[&ETH_HLEN..];
            
        match peek_version(payload) {
        // if option fails
            Option::None => {
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::Packet(BadVersion(0)));
            }
            // if version isnt supported
            Some(v) if v != BATADV_COMPAT_VERSION => {
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::Packet(BadVersion(v)));
            }
            Some(_)=>{}
        }

        match peek_type(payload){
            Option::None=>Err(DropReason::TooShort),
            
            Some(BATADV_IV_OGM)=>{
                if !eth_header.dest().is_broadcast(){
                    self.hard_iface.record_rx_dropped();
                    return Err(DropReason::WrongDestination);
                }
                ogm::process_ogm(
                    &mut self.hard_iface,
                    &mut self.orig_table,
                    eth_header.source(),
                    payload,
                    now
                );

                return Ok(());

            },
            Some(BATADV_BCAST) =>{
                if !eth_header.dest().is_broadcast(){
                    self.hard_iface.record_rx_dropped();
                    return Err(DropReason::WrongDestination);
                }
                bcast::process_bcast(
                    
                );

                return Ok(());
            },
            Some(BATADV_UNICAST) => {
                if eth_header.dest() != self.orig_addr {
                    self.hard_iface.record_rx_dropped();
                    return Err(DropReason::WrongDestination);
                }
            
                unicast::process_unicast(
                    
                );

                return Ok(());


            }
            Some(_)=>{
                self.hard_iface.record_rx_dropped();
                return Err(DropReason::NoRoute);
            }

        }

    }


}




pub struct TranslationTable{
    placeholder: u8,
}

impl TranslationTable {
    pub const fn new()->Self{
        Self{
            placeholder:0,
        }
    }
}