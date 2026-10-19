use crate::mac::Mac;
use crate::consts::ETH_HLEN;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceError{
    InterfaceDown,
    QueueFull,
    FrameTooLarge{max: usize, got:usize},
    DriverFault(u8),
}

pub trait LinkDriver{

    fn hardware_address(&self)->Mac;

    fn payload_size(&self)->usize;

    fn link_state(&self)->bool;

    fn transmit(&mut self, buf:&[u8])->Result<(),InterfaceError>;

}

pub struct HardInterface<D>{
    driver: D,
    id:u8,
    hw_addr:Mac,
    wireless:bool,
    payload:usize,
    primary:bool,
    active:bool,
    ogm_seqno:u32,
    tx_frames: u32,
    tx_bytes: u32,
    tx_dropped:u32,
    rx_frames: u32,
    rx_bytes: u32,
    rx_dropped: u32,
}

impl <D:LinkDriver> HardInterface<D> {
    
    pub fn new(driver:D, id:u8)->Option<Self>{
        let hw_addr: Mac = driver.hardware_address();
        if hw_addr.is_zero() || hw_addr.is_multicast(){
            return None;
        }
        let payload: usize = driver.payload_size();
        Some(Self{
            driver,
            id,
            hw_addr,
            wireless:true, // always true for now
            payload,
            primary: false,
            active: false,
            ogm_seqno: 0,
            tx_frames: 0,
            tx_bytes: 0,
            tx_dropped:0,
            rx_frames: 0,
            rx_bytes: 0,
            rx_dropped: 0,

        })
    }

    // --- Getters --- 
    /// For firmware-side configuration only. Nothing in the protocol
    /// should reach the driver except through `send`.
    /// Returns driver
    pub fn driver_mut(&mut self)->&mut D{
        &mut self.driver
    }
    /// Returns the id of the hard interface as a u8
    pub fn id(&self)->u8{
        self.id
    }
    /// Returns the hardware address as a Mac Object
    pub fn hardware_addr(&self)->Mac{
        self.hw_addr
    }
    pub fn is_wireless(&self)->bool{
        self.wireless
    }
    /// Returns the payload length
    pub fn payload(&self)->usize{
        self.payload
    }
    /// Returns the primary flag
    pub fn is_primary(&self)->bool{
        self.primary
    }
    /// Returns the active flag
    pub fn is_active(&self)->bool{
        self.active
    }
    /// Returns the OGM sequnce number as u32
    pub fn ogm_seqno(&self)->u32{
        self.ogm_seqno
    }

    // --- Transiver ---
    /// Returns frames sent
    pub fn tx_frames_sent(&self)->u32{
        self.tx_frames
    }
    /// Returns bytes sent
    pub fn tx_bytes_sent(&self)->u32{
        self.tx_bytes
    }
    /// Returns how many transiver dropped
    pub fn tx_dropped(&self)->u32{
        self.tx_dropped
    }

    // --- Reciver ---
    /// Returns frames recived
    pub fn rx_frames_recived(&self)->u32{
        self.rx_frames
    }
    /// Returns bytes recived
    pub fn rx_bytes_recived(&self)->u32{
        self.rx_bytes
    }
    /// Returns how many reciver dropped
    pub fn rx_dropped(&self)->u32{
        self.rx_dropped
    }

    // --- States ---
    /// sets the active flag to 'state'
    pub fn set_active(&mut self, state: bool){
        self.active = state;
    }
    /// sets the primary flag to 'state'
    pub fn set_primary(&mut self, state: bool){
        self.primary = state
    }

    // --- Functions ---
    /// sets active flag to true, and sets the ogm 
    /// sequnce number to the timestamp
    pub fn enable(&mut self, time: u32)->Option<bool>{
        if self.active{
            return None;
        }        
        self.ogm_seqno = time;
        self.active = true;
        Some(self.active)
    }
    
    // TODO: Finish documentaton
    /// Returns the maximun frame length for ...
    pub fn max_frame_len(&self) -> usize {
        self.payload + ETH_HLEN
    }

    /// Returns the state of the mesh
    pub fn is_up(&self)->bool{
        self.active && self.driver.link_state()
    }

    /// Returns the link state of the driver
    pub fn driver_link_state(&self) -> bool{
        self.driver.link_state()
    }
    
    /// Transmits ...
    pub fn transmit(&mut self, frame:&[u8])->Result<(),InterfaceError> {
        if !self.active || !self.driver.link_state() {
            self.tx_dropped = self.tx_dropped.wrapping_add(1);
            return Err(InterfaceError::InterfaceDown);
        }
        let max_len: usize = self.max_frame_len();
        if frame.len()>max_len{
            self.tx_dropped = self.tx_dropped.wrapping_add(1);
            return Err(InterfaceError::FrameTooLarge { max: max_len, got: frame.len() });
        }
        match self.driver.transmit(frame) {
            Ok(()) => {
                self.tx_frames = self.tx_frames.wrapping_add(1);
                self.tx_bytes = self.tx_bytes.wrapping_add(frame.len() as u32);
                Ok(())
            }
            Err(err) => {
                self.tx_dropped = self.tx_dropped.wrapping_add(1);
                Err(err)
            }
        }
        
    }
    
    pub fn next_ogm_seqno(&mut self)->u32{
        let seqno: u32 = self.ogm_seqno;
        self.ogm_seqno = self.ogm_seqno.wrapping_add(1);
        seqno
    }
    pub fn record_rx(&mut self, len:usize){
        self.rx_frames = self.rx_frames.wrapping_add(1);
        self.rx_bytes = self.rx_bytes.wrapping_add(len as u32);
    }
    pub fn record_tx_dropped(&mut self){
        self.tx_dropped = self.tx_dropped.wrapping_add(1);
    }
    pub fn record_rx_dropped(&mut self){
        self.rx_dropped = self.rx_dropped.wrapping_add(1);
    }

}

#[cfg(test)]
mod tests{
    use super::*;

    // --- test helpers ---
    const TEST_PAYLOAD: usize = 100;
    const UNICAST: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

    struct MockDriver {
        mac: Mac,
        payload: usize,
        link: bool,
        tx_result: Result<(), InterfaceError>,
        tx_calls: u32,
        last_tx: [u8; 256],
        last_tx_len: usize,
    }

    impl LinkDriver for MockDriver {
        fn hardware_address(&self) -> Mac {
            self.mac
        }
        fn payload_size(&self) -> usize {
            self.payload
        }
        fn link_state(&self) -> bool {
            self.link
        }
        fn transmit(&mut self, buf: &[u8]) -> Result<(), InterfaceError> {
            self.tx_calls += 1;
            self.last_tx_len = buf.len();
            self.last_tx[..buf.len()].copy_from_slice(buf);
            self.tx_result
        }
    }


    fn mock_driver(addr: Mac) -> MockDriver {
        MockDriver {
            mac: addr,
            payload: TEST_PAYLOAD,
            link: true,
            tx_result: Ok(()),
            tx_calls: 0,
            last_tx: [0u8; 256],
            last_tx_len: 0,
        }
    }

    fn test_if() -> HardInterface<MockDriver> {
        HardInterface::new(
            mock_driver(
                Mac::from_octets(UNICAST)
            ), 1).expect("unicast MAC must construct")
    }

    #[test]
    fn hard_interface_init_test(){
        let octets = UNICAST;
        let addr = Mac::from_octets(octets);
        let driver = mock_driver(addr);
        let  hardif = HardInterface::<MockDriver>::new(driver, 0).unwrap();
        assert_eq!(hardif.id, 0);
        assert_eq!(hardif.hw_addr, addr);
        assert_eq!(hardif.wireless, true);
        assert_eq!(hardif.payload, TEST_PAYLOAD);
        assert_eq!(hardif.primary, false);
        assert_eq!(hardif.active, false);
        assert_eq!(hardif.ogm_seqno, 0);
        assert_eq!(hardif.tx_frames, 0);
        assert_eq!(hardif.tx_bytes, 0);
        assert_eq!(hardif.tx_dropped, 0);
        assert_eq!(hardif.rx_frames, 0);
        assert_eq!(hardif.rx_bytes, 0);
        assert_eq!(hardif.rx_dropped, 0);
        assert_eq!(hardif.driver.tx_calls, 0);


        // zero MAC rejected
        assert!(HardInterface::new(
            mock_driver(
                Mac::from_octets([0u8; 6])
            ), 1).is_none()
        );

        // multicast rejected
        assert!(HardInterface::new(
            mock_driver(
                Mac::from_octets([0x01, 0x00, 0x5E, 0x00, 0x00, 0x01])
            ), 1).is_none()
        );

        // broadcast rejected (multicast subset)
        assert!(HardInterface::new(
            mock_driver(
                Mac::from_octets([0xFF; 6])), 1).is_none()
        );

        // globally administered unicast accepted
        assert!(HardInterface::new(
            mock_driver(
                Mac::from_octets([0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E])), 1).is_some()
        );

        // id bounds accepted
        assert!(HardInterface::new(
            mock_driver(
                Mac::from_octets(UNICAST)
            ), 0).is_some()
        );
        assert!(HardInterface::new(
            mock_driver(
                Mac::from_octets(UNICAST)
            ), u8::MAX).is_some()
        );
    }

    #[test]
    fn hard_interface_enable_test(){

        let mut hardif = test_if();

        // not enabled
        assert_eq!(hardif.active, false);
        assert_eq!(hardif.ogm_seqno, 0);

        // first enable: activates, seqno = timestamp
        assert_eq!(hardif.enable(1000), Some(true));
        assert_eq!(hardif.active, true);
        assert_eq!(hardif.ogm_seqno, 1000);

        // already enabled: None, seqno not overwritten
        assert_eq!(hardif.enable(2000), None);
        assert_eq!(hardif.active, true);
        assert_eq!(hardif.ogm_seqno, 1000);

        // deactivate then re-enable: new timestamp taken
        hardif.active = false;
        assert_eq!(hardif.enable(3000), Some(true));
        assert_eq!(hardif.active, true);
        assert_eq!(hardif.ogm_seqno, 3000);

        // TODO: check if enable should ignore link state
        /* 
        let mut down = test_if();
        down.driver.link = false;
        assert_eq!(down.enable(42), Some(true));
        assert_eq!(down.active, true);
        assert_eq!(down.ogm_seqno, 42);
        */

    }

    #[test]
    fn hard_interface_max_frame_len_test(){
        let mut hardif = test_if();
        assert_eq!(hardif.max_frame_len(), TEST_PAYLOAD + ETH_HLEN);

        hardif.payload = 1500;
        assert_eq!(hardif.max_frame_len(), 1500 + ETH_HLEN);

        hardif.payload = 0;
        assert_eq!(hardif.max_frame_len(), ETH_HLEN);

        // independent of active / link state
        hardif.payload = TEST_PAYLOAD;
        hardif.active = true;
        hardif.driver.link = false;
        assert_eq!(hardif.max_frame_len(), TEST_PAYLOAD + ETH_HLEN);
    }

    #[test]
    fn hard_interface_is_up_test(){
        let mut hardif = test_if();

        // inactive, link down
        hardif.active = false;
        hardif.driver.link = false;
        assert_eq!(hardif.is_up(), false);

        // inactive, link up
        hardif.active = false;
        hardif.driver.link = true;
        assert_eq!(hardif.is_up(), false);

        // active, link down
        hardif.active = true;
        hardif.driver.link = false;
        assert_eq!(hardif.is_up(), false);

        // active, link up
        hardif.active = true;
        hardif.driver.link = true;
        assert_eq!(hardif.is_up(), true);
    }

    #[test]
    fn hard_interface_driver_link_state_test(){
        let mut hardif = test_if();

        // reflects driver while inactive
        hardif.active = false;
        hardif.driver.link = true;
        assert_eq!(hardif.driver_link_state(), true);
        hardif.driver.link = false;
        assert_eq!(hardif.driver_link_state(), false);

        // reflects driver while active 
        hardif.active = true;
        hardif.driver.link = false;
        assert_eq!(hardif.driver_link_state(), false);
        hardif.driver.link = true;
        assert_eq!(hardif.driver_link_state(), true);
    }

    #[test]
    fn hard_interface_transmit_test(){
        let mut hardif = test_if();
        let max = TEST_PAYLOAD + ETH_HLEN;
        let too_big = [0xABu8; TEST_PAYLOAD + ETH_HLEN + 1];
        let full = [0xCDu8; TEST_PAYLOAD + ETH_HLEN];
        let small: [u8; 4] = [0x01, 0x02, 0x03, 0x04];

        // inactive, link up: InterfaceDown, dropped, driver not called
        hardif.active = false;
        hardif.driver.link = true;
        assert_eq!(hardif.transmit(&small), Err(InterfaceError::InterfaceDown));
        assert_eq!(hardif.tx_dropped, 1);
        assert_eq!(hardif.driver.tx_calls, 0);

        // active, link down: InterfaceDown, dropped, driver not called
        hardif.active = true;
        hardif.driver.link = false;
        assert_eq!(hardif.transmit(&small), Err(InterfaceError::InterfaceDown));
        assert_eq!(hardif.tx_dropped, 2);
        assert_eq!(hardif.driver.tx_calls, 0);

        // down check comes before size check
        hardif.active = false;
        hardif.driver.link = true;
        assert_eq!(hardif.transmit(&too_big), Err(InterfaceError::InterfaceDown));
        assert_eq!(hardif.tx_dropped, 3);

        // up, oversize: FrameTooLarge, dropped, driver not called
        hardif.active = true;
        assert_eq!(
            hardif.transmit(&too_big),
            Err(InterfaceError::FrameTooLarge { max, got: max + 1 })
        );
        assert_eq!(hardif.tx_dropped, 4);
        assert_eq!(hardif.driver.tx_calls, 0);

        // failures never touch success counters
        assert_eq!(hardif.tx_frames, 0);
        assert_eq!(hardif.tx_bytes, 0);

        // exact max: sent, bytes passed to driver unchanged
        assert_eq!(hardif.transmit(&full), Ok(()));
        assert_eq!(hardif.tx_frames, 1);
        assert_eq!(hardif.tx_bytes, max as u32);
        assert_eq!(hardif.tx_dropped, 4);
        assert_eq!(hardif.driver.tx_calls, 1);
        assert_eq!(hardif.driver.last_tx_len, max);
        assert_eq!(&hardif.driver.last_tx[..max], &full[..]);

        // small frame: counters accumulate
        assert_eq!(hardif.transmit(&small), Ok(()));
        assert_eq!(hardif.tx_frames, 2);
        assert_eq!(hardif.tx_bytes, (max + 4) as u32);
        assert_eq!(hardif.driver.last_tx_len, 4);
        assert_eq!(&hardif.driver.last_tx[..4], &small[..]);

        // driver error: propagated as-is, counted as dropped
        hardif.driver.tx_result = Err(InterfaceError::QueueFull);
        assert_eq!(hardif.transmit(&small), Err(InterfaceError::QueueFull));
        assert_eq!(hardif.tx_dropped, 5);
        assert_eq!(hardif.tx_frames, 2);
        assert_eq!(hardif.tx_bytes, (max + 4) as u32);
        assert_eq!(hardif.driver.tx_calls, 3);

        hardif.driver.tx_result = Err(InterfaceError::DriverFault(7));
        assert_eq!(hardif.transmit(&small), Err(InterfaceError::DriverFault(7)));
        assert_eq!(hardif.tx_dropped, 6);

        // driver recovers
        hardif.driver.tx_result = Ok(());
        assert_eq!(hardif.transmit(&small), Ok(()));
        assert_eq!(hardif.tx_frames, 3);

        // counters wrap instead of panicking
        hardif.tx_dropped = u32::MAX;
        hardif.driver.link = false;
        assert_eq!(hardif.transmit(&small), Err(InterfaceError::InterfaceDown));
        assert_eq!(hardif.tx_dropped, 0);

        hardif.driver.link = true;
        hardif.tx_frames = u32::MAX;
        hardif.tx_bytes = u32::MAX - 1;
        assert_eq!(hardif.transmit(&small), Ok(()));
        assert_eq!(hardif.tx_frames, 0);
        assert_eq!(hardif.tx_bytes, 2);

        // rx side untouched throughout
        assert_eq!(hardif.rx_frames, 0);
        assert_eq!(hardif.rx_bytes, 0);
        assert_eq!(hardif.rx_dropped, 0);
    }

    #[test]
    fn hard_interface_next_ogm_seqno_test(){
        let mut hardif = test_if();

        // returns current, then increments (post-increment)
        assert_eq!(hardif.next_ogm_seqno(), 0);
        assert_eq!(hardif.ogm_seqno, 1);
        assert_eq!(hardif.next_ogm_seqno(), 1);
        assert_eq!(hardif.ogm_seqno, 2);

        // continues from arbitrary start (e.g. enable timestamp)
        hardif.ogm_seqno = 500;
        assert_eq!(hardif.next_ogm_seqno(), 500);
        assert_eq!(hardif.ogm_seqno, 501);

        // wraps at u32::MAX
        hardif.ogm_seqno = u32::MAX;
        assert_eq!(hardif.next_ogm_seqno(), u32::MAX);
        assert_eq!(hardif.ogm_seqno, 0);
        assert_eq!(hardif.next_ogm_seqno(), 0);
        assert_eq!(hardif.ogm_seqno, 1);
    }

    #[test]
    fn hard_interface_record_rx_test(){
        let mut hardif = test_if();

        hardif.record_rx(64);
        assert_eq!(hardif.rx_frames, 1);
        assert_eq!(hardif.rx_bytes, 64);

        hardif.record_rx(100);
        assert_eq!(hardif.rx_frames, 2);
        assert_eq!(hardif.rx_bytes, 164);

        // zero-length frame still counts as frame
        hardif.record_rx(0);
        assert_eq!(hardif.rx_frames, 3);
        assert_eq!(hardif.rx_bytes, 164);

        // other counters untouched
        assert_eq!(hardif.rx_dropped, 0);
        assert_eq!(hardif.tx_frames, 0);
        assert_eq!(hardif.tx_bytes, 0);
        assert_eq!(hardif.tx_dropped, 0);

        // wraps instead of panicking
        hardif.rx_frames = u32::MAX;
        hardif.rx_bytes = u32::MAX - 1;
        hardif.record_rx(4);
        assert_eq!(hardif.rx_frames, 0);
        assert_eq!(hardif.rx_bytes, 2);
    }

        #[test]
    fn hard_interface_record_tx_dropped_test(){
        let mut hardif = test_if();

        hardif.record_tx_dropped();
        assert_eq!(hardif.tx_dropped, 1);
        hardif.record_tx_dropped();
        assert_eq!(hardif.tx_dropped, 2);

        // other counters untouched
        assert_eq!(hardif.tx_frames, 0);
        assert_eq!(hardif.tx_bytes, 0);
        assert_eq!(hardif.rx_frames, 0);
        assert_eq!(hardif.rx_bytes, 0);
        assert_eq!(hardif.rx_dropped, 0);

        // wraps instead of panicking
        hardif.tx_dropped = u32::MAX;
        hardif.record_tx_dropped();
        assert_eq!(hardif.tx_dropped, 0);
    }

    #[test]
    fn hard_interface_record_rx_dropped_test(){
        let mut hardif = test_if();

        hardif.record_rx_dropped();
        assert_eq!(hardif.rx_dropped, 1);
        hardif.record_rx_dropped();
        assert_eq!(hardif.rx_dropped, 2);

        // other counters untouched
        assert_eq!(hardif.rx_frames, 0);
        assert_eq!(hardif.rx_bytes, 0);
        assert_eq!(hardif.tx_frames, 0);
        assert_eq!(hardif.tx_bytes, 0);
        assert_eq!(hardif.tx_dropped, 0);

        // wraps instead of panicking
        hardif.rx_dropped = u32::MAX;
        hardif.record_rx_dropped();
        assert_eq!(hardif.rx_dropped, 0);
    }

}

