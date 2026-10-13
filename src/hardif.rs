use crate::mac::Mac;
use crate::eth::ETH_HLEN;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterfaceError{
    InterfaceDown,
    QueueFull,
    FrameTooLarge{max: usize, got:usize},
    DriverFault(u8),
}

pub trait LinkDriver{

    fn hardware_addr(&self)->Mac;

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
        let hw_addr: Mac = driver.hardware_addr();
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


    // --- Functions ---
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

