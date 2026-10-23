use crate::PacketError::BadVersion;
use crate::ogm::send_own_ogm;
use crate::time::{self, INSTANT_ZERO, Instant};
use crate::{FrameError, HardInterface, InterfaceError, LinkDriver, PacketError, broadcast, ogm, unicast};
use crate::eth::EthHeader;
use crate::mac::Mac;
use crate::originatortable::{OriginatorTable, OriginatorTableError};
use crate::originator::OriginatorError;
use crate::packets::{ETH_P_BATMAN, peek_type};
use crate::packets::peek_version;
use crate::packets::{BATADV_COMPAT_VERSION, BATADV_IV_OGM, BATADV_BCAST, BATADV_UNICAST};
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

#[derive(Debug, Clone, Copy)]
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

    // --- getters ---
    pub fn address(&self)->Mac{
        self.orig_addr
    }
    pub fn originator_table(&self)->&OriginatorTable{
        &self.orig_table
    }
    pub fn hard_interface(&self)->&HardInterface<D>{
        &self.hard_iface
    }
    pub fn hard_interface_mut(&mut self) -> &mut HardInterface<D> {
        &mut self.hard_iface
    }
    pub fn mesh_status(&self)->bool{
        self.mesh_active
    }
    pub fn broadcast_seqno(&self)->u32{
        self.bcast_seqno
    }
    pub fn next_ogm(&self)->Instant{
        self.next_ogm
    }
    pub fn next_purge(&self)->Instant{
        self.next_purge
    }


    // --- functions ---
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PacketError::TooShort;
use crate::consts::{MAX_BATMAN_FRAME_SIZE, ORIG_TIME_OUT};
    use crate::originator::Originator;
    use crate::packets::{BatmanAdvBroadcastPacket, BatmanAdvOGMPacket, BatmanAdvUnicastPacket, Packet};
use crate::time::earliest;

    // --- test helpers ---

    const NOW: Instant = Instant::from_ticks(0);
    const INNER: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];

    // node ids for mac()
    const MY: u8 = 1;      // this node
    const N: u8 = 2;       // direct neighbor
    const O: u8 = 3;       // remote originator
    const R: u8 = 4;       // next hop towards O
    const HOST: u8 = 5;    // some other station
    const UNKNOWN: u8 = 9; // in no table

    fn mac(n: u8) -> Mac {
        Mac::from_octets([0x02, 0x00, 0x00, 0x00, 0x00, n])
    }

    struct MockDriver {
        mac: Mac,
        link: bool,
        tx_calls: u32,
        last_tx: [u8; MAX_BATMAN_FRAME_SIZE],
        last_tx_len: usize,
    }

    impl LinkDriver for MockDriver {
        fn hardware_address(&self) -> Mac {
            self.mac
        }
        fn payload_size(&self) -> usize {
            MAX_BATMAN_FRAME_SIZE - ETH_HLEN
        }
        fn link_state(&self) -> bool {
            self.link
        }
        fn transmit(&mut self, buf: &[u8]) -> Result<(), InterfaceError> {
            self.tx_calls += 1;
            self.last_tx_len = buf.len();
            self.last_tx[..buf.len()].copy_from_slice(buf);
            Ok(())
        }
    }

    /// inactive mesh on an interface owned by mac(MY)
    fn ctx() -> MeshCtx<MockDriver> {
        let drv = MockDriver {
            mac: mac(MY),
            link: true,
            tx_calls: 0,
            last_tx: [0u8; MAX_BATMAN_FRAME_SIZE],
            last_tx_len: 0,
        };
        MeshCtx::new(HardInterface::new(drv, 0).expect("unicast MAC must construct"))
    }

    /// mesh activated at NOW
    fn active_ctx() -> MeshCtx<MockDriver> {
        let mut c = ctx();
        c.activate(NOW);
        c
    }

    fn tx_calls(c: &mut MeshCtx<MockDriver>) -> u32 {
        c.hard_interface_mut().driver_mut().tx_calls
    }

    /// route: `dest` reachable via next hop `router`
    fn add_route(c: &mut MeshCtx<MockDriver>, dest: Mac, router: Mac) {
        let mut orig = Originator::new(dest, NOW);
        orig.set_router(router);
        c.orig_table.add(orig, NOW).unwrap();
    }

    fn eth(buf: &mut [u8], dst: Mac, src: Mac, ethertype: u16) {
        assert!(EthHeader::new(dst, src, ethertype).write(&mut buf[..ETH_HLEN]).is_ok());
    }

    /// host frame: eth header + 0xAB payload filling the rest of buf
    fn host_frame(buf: &mut [u8], dst: Mac, src: Mac, ethertype: u16) {
        eth(buf, dst, src, ethertype);
        buf[ETH_HLEN..].fill(0xAB);
    }

    /// inner ethernet frame addressed to `dst`, 6 payload bytes
    fn inner_to(dst: Mac) -> [u8; ETH_HLEN + 6] {
        let mut f = [0u8; ETH_HLEN + 6];
        host_frame(&mut f, dst, mac(HOST), 0x0800);
        f
    }

    /// eth(dst, src) + OGM originated by src; returns len
    fn ogm_frame(buf: &mut [u8], dst: Mac, src: Mac, seqno: u32, tq: u8) -> usize {
        eth(buf, dst, src, ETH_P_BATMAN);
        let mut p = BatmanAdvOGMPacket::new(src, seqno);
        p.set_tq(tq);
        p.write(&mut buf[ETH_HLEN..]).unwrap();
        ETH_HLEN + BatmanAdvOGMPacket::WIRE_LEN
    }

    /// eth(broadcast, src) + bcast(orig, seqno) + inner; returns len
    fn bcast_frame(buf: &mut [u8], src: Mac, orig: Mac, seqno: u32, inner: &[u8]) -> usize {
        eth(buf, Mac::BROADCAST, src, ETH_P_BATMAN);
        BatmanAdvBroadcastPacket::new(orig, seqno).write(&mut buf[ETH_HLEN..]).unwrap();
        let off = ETH_HLEN + BatmanAdvBroadcastPacket::WIRE_LEN;
        buf[off..off + inner.len()].copy_from_slice(inner);
        off + inner.len()
    }

    /// eth(dst, src) + unicast(dest) + inner; returns len
    fn unicast_frame(buf: &mut [u8], dst: Mac, src: Mac, dest: Mac, inner: &[u8]) -> usize {
        eth(buf, dst, src, ETH_P_BATMAN);
        BatmanAdvUnicastPacket::new(dest).write(&mut buf[ETH_HLEN..]).unwrap();
        let off = ETH_HLEN + BatmanAdvUnicastPacket::WIRE_LEN;
        buf[off..off + inner.len()].copy_from_slice(inner);
        off + inner.len()
    }

    #[test]
    fn new_test(){
        let c = ctx();
        assert_eq!(c.address(), mac(MY));
        assert_eq!(c.mesh_status(), false);
        assert_eq!(c.broadcast_seqno(), 0);
        assert_eq!(c.next_ogm(), INSTANT_ZERO);
        assert_eq!(c.next_purge(), INSTANT_ZERO);
        assert!(!c.hard_interface().is_active());
        assert!(c.originator_table().find_mac(mac(MY)).is_none());
    }

    // TODO: Implement init() method    
    #[test]
    fn init_test(){
        
    }

    #[test]
    fn activate_test(){
        let mut c = ctx();
        let t = Instant::from_ticks(5_000);
        c.activate(t);

        assert!(c.mesh_status());
        assert!(c.hard_interface().is_active());

        // first OGM due immediately, first purge one period later
        assert_eq!(c.next_ogm(), t);
        assert_eq!(c.next_purge(), t + PURGE_PERIOD);
    }

    #[test]
    fn deactivate_test(){
        let mut c = active_ctx();
        add_route(&mut c, mac(O), mac(R));

        c.deactivate();
        assert!(!c.mesh_status());
        assert!(!c.hard_interface().is_active());

        // all routing state dropped
        assert!(c.originator_table().find_mac(mac(O)).is_none());

        // re-activate: fresh schedule
        let t = Instant::from_ticks(9_000);
        c.activate(t);
        assert!(c.mesh_status());
        assert_eq!(c.next_ogm(), t);

    }

        #[test]
    fn handle_frame_inactive_test(){
        let mut c = ctx();
        let mut frame = [0u8; 64];
        let len = ogm_frame(&mut frame, Mac::BROADCAST, mac(N), 1, 255);

        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::NotActive));

        // counted as received and dropped, nothing learned
        assert_eq!(c.hard_interface().rx_frames_recived(), 1);
        assert_eq!(c.hard_interface().rx_bytes_recived(), len as u32);
        assert_eq!(c.hard_interface().rx_dropped(), 1);
        assert!(c.originator_table().find_mac(mac(N)).is_none());

        // make sure it did not send anything
        assert_eq!(c.hard_interface().tx_frames_sent(), 0);
        assert_eq!(c.hard_interface().tx_bytes_sent(),0);

    }

    #[test]
    fn handle_frame_filter_test(){
        let mut c = active_ctx();
        let mut frame = [0u8; 64];
        let mut drops: u32 = 0;

        // shorter than eth header
        let res = c.handle_frame(&mut frame[..ETH_HLEN - 1], NOW);
        assert!(matches!(res, Err(DropReason::Packet(TooShort{need:ETH_HLEN, got:13}))));
        drops += 1;

        // not batman
        eth(&mut frame, Mac::BROADCAST, mac(N), 0x0800);
        assert_eq!(c.handle_frame(&mut frame[..ETH_HLEN + 4], NOW), Err(DropReason::NotBatman(0x0800)));
        drops += 1;

        // invalid source: zero, broadcast, multicast
        for &src in [Mac::ZERO, Mac::BROADCAST, Mac::STP].iter() {
            let len = ogm_frame(&mut frame, Mac::BROADCAST, src, 1, 255);
            assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::InvalidSource), "src {:?}", src);
            drops += 1;
        }


        // our own frame heard back
        let len = ogm_frame(&mut frame, Mac::BROADCAST, mac(MY), 1, 255);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::OwnFrame));
        drops += 1;

        // batman ethertype, payload too short for type + version
        eth(&mut frame, Mac::BROADCAST, mac(N), ETH_P_BATMAN);
        frame[ETH_HLEN] = BATADV_IV_OGM;
        assert_eq!(c.handle_frame(&mut frame[..ETH_HLEN], NOW), Err(DropReason::TooShort));
        assert_eq!(c.handle_frame(&mut frame[..ETH_HLEN + 1], NOW), Err(DropReason::TooShort));
        drops += 2;

        // unsupported compat version
        let len = ogm_frame(&mut frame, Mac::BROADCAST, mac(N), 1, 255);
        frame[ETH_HLEN + 1] = 14;
        assert_eq!(
            c.handle_frame(&mut frame[..len], NOW),
            Err(DropReason::Packet(PacketError::BadVersion(14)))
        );
        drops += 1;

        // unknown packet type
        let len = ogm_frame(&mut frame, Mac::BROADCAST, mac(N), 1, 255);
        frame[ETH_HLEN] = 0x99;
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::UnknownType(0x99)));
        drops += 1;

        // OGM not sent to broadcast
        let len = ogm_frame(&mut frame, mac(MY), mac(N), 1, 255);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::WrongDestination));
        drops += 1;

        // bcast not sent to broadcast
        let len = bcast_frame(&mut frame, mac(N), mac(O), 1, &INNER);
        eth(&mut frame, mac(MY), mac(N), ETH_P_BATMAN);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::WrongDestination));
        drops += 1;

        // unicast addressed to another station
        let len = unicast_frame(&mut frame, mac(R), mac(N), mac(O), &inner_to(mac(O)));
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::WrongDestination));
        drops += 1;

        // every frame counted and dropped, nothing learned or sent
        assert_eq!(c.hard_interface().rx_frames_recived(), drops);
        assert_eq!(c.hard_interface().rx_dropped(), drops);
        assert!(c.originator_table().find_mac(mac(N)).is_none());
        assert_eq!(tx_calls(&mut c), 0);
    }

    #[test]
    fn handle_frame_ogm_test(){
        let mut c = active_ctx();
        let mut frame = [0u8; 64];

        // OGM from new direct neighbor: learned, rebroadcast, nothing delivered
        let len = ogm_frame(&mut frame, Mac::BROADCAST, mac(N), 1, 255);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Ok(None));
        assert!(c.originator_table().find_mac(mac(N)).is_some());
        assert_eq!(tx_calls(&mut c), 1);
        assert_eq!(c.hard_interface().rx_dropped(), 0);

        // processing error passed through and counted
        let len = ogm_frame(&mut frame, Mac::BROADCAST, mac(N), 2, 0);
        assert_eq!(
            c.handle_frame(&mut frame[..len], NOW),
            Err(DropReason::Originator(OriginatorError::ZeroTq))
        );
        assert_eq!(c.hard_interface().rx_frames_recived(), 2);
        assert_eq!(c.hard_interface().rx_dropped(), 1);

    }

    #[test]
    fn handle_frame_bcast_test(){
        let mut c = active_ctx();
        c.orig_table.add(Originator::new(mac(O), NOW), NOW).unwrap();
        let mut frame = [0u8; 64];

        // bcast from known originator: inner delivered, rebroadcast
        let len = bcast_frame(&mut frame, mac(N), mac(O), 1, &INNER);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Ok(Some(&INNER[..])));
        assert_eq!(tx_calls(&mut c), 1);

        // duplicate: dropped and counted, not rebroadcast
        let len = bcast_frame(&mut frame, mac(N), mac(O), 1, &INNER);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::Duplicate));
        assert_eq!(c.hard_interface().rx_dropped(), 1);
        assert_eq!(tx_calls(&mut c), 1);
    }

    #[test]
    fn handle_frame_unicast_test(){
        let mut c = active_ctx();
        add_route(&mut c, mac(O), mac(R));
        let mut frame = [0u8; 64];

        // for us: inner delivered, nothing sent
        let to_me = inner_to(mac(MY));
        let len = unicast_frame(&mut frame, mac(MY), mac(N), mac(MY), &to_me);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Ok(Some(&to_me[..])));
        assert_eq!(tx_calls(&mut c), 0);

        // for O, we are next hop: forwarded to R, nothing delivered
        let to_o = inner_to(mac(O));
        let len = unicast_frame(&mut frame, mac(MY), mac(N), mac(O), &to_o);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Ok(None));
        assert_eq!(tx_calls(&mut c), 1);

        let mut fwd_eth = [0u8; ETH_HLEN];
        eth(&mut fwd_eth, mac(R), mac(MY), ETH_P_BATMAN);
        assert_eq!(&c.hard_interface_mut().driver_mut().last_tx[..ETH_HLEN], &fwd_eth[..]);

        // unknown destination: NoRoute, counted
        let to_x = inner_to(mac(UNKNOWN));
        let len = unicast_frame(&mut frame, mac(MY), mac(N), mac(UNKNOWN), &to_x);
        assert_eq!(c.handle_frame(&mut frame[..len], NOW), Err(DropReason::NoRoute));
        assert_eq!(c.hard_interface().rx_dropped(), 1);
        assert_eq!(tx_calls(&mut c), 1);

    }

    #[test]
    fn send_frame_reject_test(){
        let mut frame = [0u8; 64];
        host_frame(&mut frame, Mac::BROADCAST, mac(MY), 0x0800);

        // mesh inactive
        let mut idle = ctx();
        assert!(matches!(idle.send_frame(&frame), Err(TransmitionError::MeshNotActive)));

        let mut c = active_ctx();

        // shorter than eth header
        assert!(matches!(c.send_frame(&frame[..ETH_HLEN - 1]), Err(TransmitionError::TooShort)));

        // VLAN tagged
        let mut vlan = frame;
        eth(&mut vlan, Mac::BROADCAST, mac(MY), ETH_P_8021Q);
        assert!(matches!(c.send_frame(&vlan), Err(TransmitionError::VlanTagged)));

        // not from us
        let mut foreign = frame;
        eth(&mut foreign, Mac::BROADCAST, mac(HOST), 0x0800);
        assert!(matches!(c.send_frame(&foreign), Err(TransmitionError::InvalidSource)));

        // link-local control destinations
        for &dst in [Mac::STP, Mac::ECTP].iter() {
            let mut ctl = frame;
            eth(&mut ctl, dst, mac(MY), 0x0800);
            assert!(
                matches!(c.send_frame(&ctl), Err(TransmitionError::InvalidDestination)),
                "dst {:?}", dst
            );
        }

        // oversize broadcast
        let mut big = [0u8; BCAST_INNER_FRAME_LEN + 1];
        eth(&mut big, Mac::BROADCAST, mac(MY), 0x0800);
        assert!(matches!(c.send_frame(&big), Err(TransmitionError::TooLong)));

        // oversize unicast (size checked before route)
        let mut big = [0u8; UNICAST_INNER_FRAME_LEN + 1];
        eth(&mut big, mac(O), mac(MY), 0x0800);
        assert!(matches!(c.send_frame(&big), Err(TransmitionError::TooLong)));

        // unicast without route
        let mut lost = frame;
        eth(&mut lost, mac(UNKNOWN), mac(MY), 0x0800);
        assert!(matches!(c.send_frame(&lost), Err(TransmitionError::NoRoute)));

        // unicast to ourselves
        let mut loopback = frame;
        eth(&mut loopback, mac(MY), mac(MY), 0x0800);
        assert!(matches!(c.send_frame(&loopback), Err(TransmitionError::InvalidDestination)));

        // link down
        c.hard_interface_mut().driver_mut().link = false;
        assert!(matches!(
            c.send_frame(&frame),
            Err(TransmitionError::HardInterface(InterfaceError::InterfaceDown))
        ));

        // nothing sent, no broadcast seqno used
        assert_eq!(tx_calls(&mut c), 0);
        assert_eq!(c.broadcast_seqno(), 0);

    }

    #[test]
    fn send_frame_broadcast_test(){
        let mut c = active_ctx();
        let mut frame = [0u8; 64];
        host_frame(&mut frame, Mac::BROADCAST, mac(MY), 0x0800);
        let mut bcast_eth = [0u8; ETH_HLEN];
        eth(&mut bcast_eth, Mac::BROADCAST, mac(MY), ETH_P_BATMAN);
        let off = ETH_HLEN + BatmanAdvBroadcastPacket::WIRE_LEN;

        // broadcast: wrapped with our seqno, sent once
        assert!(c.send_frame(&frame[..40]).is_ok());
        assert_eq!(c.broadcast_seqno(), 1);
        assert_eq!(tx_calls(&mut c), 1);
        let drv = c.hard_interface_mut().driver_mut();
        assert_eq!(&drv.last_tx[..ETH_HLEN], &bcast_eth[..]);
        assert_eq!(
            BatmanAdvBroadcastPacket::parse(&drv.last_tx[ETH_HLEN..]),
            Ok(BatmanAdvBroadcastPacket::new(mac(MY), 0))
        );
        assert_eq!(&drv.last_tx[off..off + 40], &frame[..40]);

        // multicast destination takes the broadcast path, next seqno
        let mut mcast = frame;
        eth(&mut mcast, Mac::from_octets([0x33, 0x33, 0x00, 0x00, 0x00, 0x01]), mac(MY), 0x86DD);
        assert!(c.send_frame(&mcast[..40]).is_ok());
        assert_eq!(c.broadcast_seqno(), 2);
        let drv = c.hard_interface_mut().driver_mut();
        assert_eq!(BatmanAdvBroadcastPacket::parse(&drv.last_tx[ETH_HLEN..]).unwrap().seqno(), 1);

        // exactly at the size limit: accepted
        let mut max = [0xABu8; BCAST_INNER_FRAME_LEN];
        eth(&mut max, Mac::BROADCAST, mac(MY), 0x0800);
        assert!(c.send_frame(&max).is_ok());
        assert_eq!(tx_calls(&mut c), 3);

    }

    #[test]
    fn send_frame_unicast_test(){
        let mut c = active_ctx();
        add_route(&mut c, mac(O), mac(R));
        let mut frame = [0u8; 64];
        host_frame(&mut frame, mac(O), mac(MY), 0x0800);
        let mut next_hop_eth = [0u8; ETH_HLEN];
        eth(&mut next_hop_eth, mac(R), mac(MY), ETH_P_BATMAN);
        let off = ETH_HLEN + BatmanAdvUnicastPacket::WIRE_LEN;

        // routed: unicast header for O, eth to next hop R
        assert!(c.send_frame(&frame[..40]).is_ok());
        assert_eq!(tx_calls(&mut c), 1);
        let drv = c.hard_interface_mut().driver_mut();
        assert_eq!(&drv.last_tx[..ETH_HLEN], &next_hop_eth[..]);
        assert_eq!(
            BatmanAdvUnicastPacket::parse(&drv.last_tx[ETH_HLEN..]),
            Ok(BatmanAdvUnicastPacket::new(mac(O)))
        );
        assert_eq!(&drv.last_tx[off..off + 40], &frame[..40]);

        // no broadcast seqno used
        assert_eq!(c.broadcast_seqno(), 0);

        // exactly at the size limit: accepted
        let mut max = [0xABu8; UNICAST_INNER_FRAME_LEN];
        eth(&mut max, mac(O), mac(MY), 0x0800);
        assert!(c.send_frame(&max).is_ok());
        assert_eq!(tx_calls(&mut c), 2);

    }

    #[test]
    fn poll_test(){
        // inactive: nothing sent, check back after one OGM interval
        let mut idle = ctx();
        let t = Instant::from_ticks(1_000);
        assert_eq!(idle.poll(t, 0), t + ORIG_INTER);
        assert_eq!(tx_calls(&mut idle), 0);

        // first poll after activate: OGM sent right away, next one scheduled
        let mut c = active_ctx();
        let next = c.poll(NOW, 1234);
        assert_eq!(tx_calls(&mut c), 1);
        assert_eq!(c.next_ogm(), time::ogm_send_time(NOW, ORIG_INTER, 1234));

        // returns whichever deadline comes first
        let soonest = earliest(Some(c.next_ogm()), Some(c.next_purge()));

        assert_eq!(Some(next), soonest);

        // polled again before due: nothing sent, same deadline
        assert_eq!(c.poll(NOW, 99), next);
        assert_eq!(tx_calls(&mut c), 1);

        // due again: next OGM sent
        let due = c.next_ogm();
        c.poll(due, 0);
        assert_eq!(tx_calls(&mut c), 2);

    }

    #[test]
    fn poll_purge_test(){
        let mut c = active_ctx();
        let late = NOW + ORIG_TIME_OUT + PURGE_PERIOD + time::micros(1);

        // stale: last seen at NOW; fresh: last seen at `late`
        c.orig_table.add(Originator::new(mac(O), NOW), NOW).unwrap();
        c.orig_table.add(Originator::new(mac(N), late), late).unwrap();

        // purge not due yet: nothing removed
        c.poll(NOW, 0);
        assert!(c.originator_table().find_mac(mac(O)).is_some());

        // purge due: timed-out originator removed, fresh kept, next purge rescheduled
        c.poll(late, 0);
        assert!(c.originator_table().find_mac(mac(O)).is_none());
        assert!(c.originator_table().find_mac(mac(N)).is_some());
        assert_eq!(c.next_purge(), late + PURGE_PERIOD);

    }
    
}