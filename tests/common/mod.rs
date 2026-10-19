use batman_mesh_rust::hardif::{HardInterface, InterfaceError, LinkDriver};
use batman_mesh_rust::mac::Mac;
use batman_mesh_rust::meshctx::{DropReason, MeshCtx};
use batman_mesh_rust::time::{Instant, add, millis, secs, INSTANT_ZERO, is_after, has_reached};


// Largest Ethernet frame
const MAX_FRAME: usize = 1514;

// Frames one fake driver can hold
const QUEUE_LEN: usize = 8;

// Most nodes a simulation may have.
const MAX_NODES: usize = 4;

// Frames in flight at once across all nodes.
const IN_FLIGHT_CAP: usize = MAX_NODES * QUEUE_LEN;

// How many drop reasons are kept for failure messages.
const KEPT_DROPS: usize = 5;

// simulated time advances in steps of this size.
const STEP_MS: u64 = 10;

// A ethernet type for testing
const TEST_ETHERTYPE: u16 = 0x88B5;

// header length for the app frame 
pub const APP_FRAME_LEN: usize = 46;

// inbox length
const INBOX_LEN: usize = 4;


// A locally administered unicast test address ending in `last
pub fn mac(last: u8) -> Mac {
    Mac::from_octets([0x02, 0x00, 0x00, 0x00, 0x00, last])
}
 
// Jitter generator
pub struct Rng(u32);
 
impl Rng {
    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}
 
// Defines Node
pub type Node = MeshCtx<FakeDriver>;

// Make node from mac
fn make_node(mac: Mac, now: Instant) -> Node {
    let iface = HardInterface::new(FakeDriver::new(mac), 0)
        .expect("test MAC must be a valid unicast address");

    let mut node = MeshCtx::new(iface);
    node.activate(now);
    node
}

// Frame 
#[derive(Clone, Copy)]
pub struct Frame {
    len: usize,
    data: [u8; MAX_FRAME],
}

impl Frame{
    pub const EMPTY: Frame = Frame {
        len: 0,
        data: [0; MAX_FRAME],
    };
 
 
    pub fn bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }
 
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }
}



// Fake driver
#[derive(Clone, Copy)]
pub struct FakeDriver{
    pub mac:Mac,
    pub mtu:usize,
    pub link_up:bool,
    sent: [Frame;QUEUE_LEN],
    sent_len: usize,
    pub overflow: bool,
    pub fail_next: Option<InterfaceError>,
}

impl FakeDriver {
    pub fn new(mac: Mac) -> Self {
        Self {
            mac,
            mtu: 1500,
            link_up: true,
            sent: [Frame::EMPTY; QUEUE_LEN],
            sent_len: 0,
            overflow: false,
            fail_next: None,
        }
    }
 
    pub fn sent(&self) -> &[Frame] {
        &self.sent[..self.sent_len]
    }
    
    pub fn clear_sent(&mut self) {
        self.sent_len = 0;
    }
}

// LinkDriver for FakeDriver
impl LinkDriver for FakeDriver{
    fn hardware_address(&self)->Mac{
        self.mac
    }
    fn payload_size(&self)->usize{
        self.mtu
    }
    fn link_state(&self)->bool{
        self.link_up
    }
     fn transmit(&mut self, frame: &[u8]) -> Result<(), InterfaceError> {
        
        if let Some(err) = self.fail_next.take() {
            return Err(err);
        }
        if frame.len() > MAX_FRAME {
            return Err(InterfaceError::FrameTooLarge {
                max: MAX_FRAME,
                got: frame.len(),
            });
        }
        if self.sent_len == QUEUE_LEN {
            self.overflow = true;
            return Err(InterfaceError::QueueFull);
        }
 
        let slot = &mut self.sent[self.sent_len];
        slot.data[..frame.len()].copy_from_slice(frame);
        slot.len = frame.len();
        self.sent_len += 1;
        Ok(())
    }

}


pub fn at_ms(ms: u64) -> Instant {
    add(INSTANT_ZERO, millis(ms))
}

pub struct Simulation<const N:usize>{
    pub nodes: [Node; N],
    pub macs: [Mac; N],
    rngs: [Rng;N],
    wakes:[Instant;N],
    links: &'static[(usize,usize)],
    in_flight: [(usize,Frame); IN_FLIGHT_CAP],
    in_flight_len:usize,
    pub drop_count:usize,
    pub first_drops:[Option<(usize,DropReason)>;KEPT_DROPS],
    pub last_drops:[Option<(usize,DropReason)>; KEPT_DROPS],
    inboxes: [Inbox;N],
    now:Instant,
}

impl<const N:usize> Simulation<N>{
    pub fn new(macs:[Mac;N], links:&'static[(usize,usize)])->Self{
        assert!(N <= MAX_NODES, "raise MAX_NODES to simulate {} nodes", N);
        let start = at_ms(0);
        Self{
            nodes: core::array::from_fn(|n| make_node(macs[n], start)),
            macs,
            rngs: core::array::from_fn(
                |i| {Rng(0x1234_5678u32.wrapping_add(
                    (i as u32).wrapping_mul(0x9E37_79B9)) | 1)}),
            wakes: [start;N],
            links,
            in_flight: [(0, Frame::EMPTY); IN_FLIGHT_CAP],
            in_flight_len: 0,
            drop_count: 0,
            first_drops: [None; KEPT_DROPS],
            last_drops: [None; KEPT_DROPS],
            inboxes:[Inbox::EMPTY;N],
            now: INSTANT_ZERO,
        }
    }

    pub fn run_for_seconds(&mut self, seconds: u64) {
        let end = add(self.now, secs(seconds));

        while !is_after(self.now, end) {
            for i in 0..N {
                if has_reached(self.now, self.wakes[i]) {
                    let rand = self.rngs[i].next();
                    self.wakes[i] = self.nodes[i].poll(self.now, rand);
                }
            }
            self.exchange(self.now);
            self.now = add(self.now, millis(STEP_MS));
        }
    }
    
    fn hears(&self, from: usize, to: usize) -> bool {
        for &(a, b) in self.links {
            let same_direction = a == from && b == to;
            let reverse_direction = a == to && b == from;

            if same_direction || reverse_direction {
                return true;
            }
        }
        false
    }

    fn record_drop(&mut self, node: usize, reason: DropReason) {
        if self.drop_count < KEPT_DROPS {
            self.first_drops[self.drop_count] = Some((node, reason));
        }
        self.last_drops.rotate_left(1);
        self.last_drops[KEPT_DROPS - 1] = Some((node, reason));
        self.drop_count += 1;
    }

    fn exchange(&mut self, now: Instant) {
        for _round in 0..20 {
            self.in_flight_len = 0;
            for i in 0..N {
                let driver = self.nodes[i].hard_interface_mut().driver_mut();
                assert!(
                    !driver.overflow,
                    "node {} sent more than QUEUE_LEN frames in one round; raise QUEUE_LEN",
                    i
                );
                for frame in driver.sent() {
                    self.in_flight[self.in_flight_len] = (i, *frame);
                    self.in_flight_len += 1;
                }
                driver.clear_sent();
            }
 
            if self.in_flight_len == 0 {
                return;
            }
 
            for k in 0..self.in_flight_len {
                let (from, frame) = self.in_flight[k];
                for to in 0..N {
                    if to != from && self.hears(from, to) {
                        // Each receiver gets its own copy, since handling
                        // may edit the frame in place.
                        let mut copy = frame;
                        let dropped = match self.nodes[to].handle_frame(copy.bytes_mut(), now) {
                            Ok(Some(inner)) => {
                                self.inboxes[to].push(inner);
                                None
                            }
                            Ok(None) => None,
                            Err(reason) => Some(reason),
                        };
                        if let Some(reason) = dropped {
                            self.record_drop(to, reason);
                        }
                    }
                }
            }
        }
        panic!("frames still in flight after 20 rounds: something forwards in a loop");
    }

    pub fn route_tq(&self, i: usize, dest: Mac) -> u8 {
        let me = self.macs[i];
 
        let orig = match self.nodes[i].originator_table().find_mac(dest) {
            Some(orig) => orig,
            None => panic!("{} never learned {}", me, dest),
        };
 
        let via = match orig.router_id() {
            Some(via) => via,
            None => panic!("{} knows {} but has no router to it", me, dest),
        };
 
        match orig.find_neighbor(via) {
            Some(neighbor) => neighbor.tq_avg(),
            None => panic!("{}'s router {} for {} is not a neighbor", me, via, dest),
        }
    }

    pub fn delivered(&self, i: usize) -> &[Frame] {
        self.inboxes[i].frames()
    }
 
    pub fn send(&mut self, i: usize, frame: &[u8]) {
        if let Err(e) = self.nodes[i].send_frame(frame) {
            panic!("node {} could not send: {:?}", i, e);
        }
        self.exchange(self.now);
    }
 
    pub fn try_send(&mut self, i: usize, frame: &[u8]) -> bool {
        let accepted = self.nodes[i].send_frame(frame).is_ok();
        self.exchange(self.now);
        accepted
    }

    pub fn tx_count(&mut self, i: usize) -> u32 {
        self.nodes[i].hard_interface_mut().tx_frames_sent()
    }  


    // --- asserts for simulation ---
    pub fn assert_route(&self, i: usize, dest: Mac, via: Mac) {
        let me = self.macs[i];
 
        let orig = match self.nodes[i].originator_table().find_mac(dest) {
            Some(orig) => orig,
            None => panic!(
                "{} never learned {}; {} drops, first: {:?}",
                me, dest, self.drop_count, self.first_drops
            ),
        };
 
        assert_eq!(
            orig.router_id(),
            Some(via),
            "{} should route to {} via {}; echo count {}, {} drops, first: {:?}, lasts: {:?}",
            me,
            dest,
            via,
            orig.echo(),
            self.drop_count,
            self.first_drops,
            self.last_drops
        );
 
        let neighbor = match orig.find_neighbor(via) {
            Some(neighbor) => neighbor,
            None => panic!("{} has no neighbor {} for {}", me, via, dest),
        };
 
        assert!(
            neighbor.tq_avg() > 0,
            "{}: route to {} via {} has TQ 0; echo count {}",
            me,
            dest,
            via,
            orig.echo()
        );
    }
 
    /// Asserts that no node dropped anything.
    pub fn assert_no_drops(&self) {
        assert_eq!(
            self.drop_count, 0,
            "{} frames dropped, first: {:?}",
            self.drop_count, self.first_drops
        );
    }

    pub fn assert_tq_at_least(&self, i: usize, dest: Mac, min: u8) {
        let tq = self.route_tq(i, dest);
        assert!(
            tq >= min,
            "{}: TQ to {} is {}, expected at least {}",
            self.macs[i],
            dest,
            tq,
            min
        );
    }



}

// Create a ethernet frame to send throgh lib
pub fn create_frame(dest: Mac, src: Mac) -> [u8; APP_FRAME_LEN] {
    let mut frame = [0u8; APP_FRAME_LEN];
    frame[0..6].copy_from_slice(dest.as_octets());
    frame[6..12].copy_from_slice(src.as_octets());
    frame[12..14].copy_from_slice(&TEST_ETHERTYPE.to_be_bytes());
    for (i, byte) in frame[14..].iter_mut().enumerate() {
        *byte = i as u8;
    }
    frame
}

// inbox, to save frames for debugging
#[derive(Clone, Copy)]
pub struct Inbox {
    frames: [Frame; INBOX_LEN],
    len: usize,
}
 
impl Inbox {
    pub const EMPTY: Inbox = Inbox {
        frames: [Frame::EMPTY; INBOX_LEN],
        len: 0,
    };
 
    fn push(&mut self, bytes: &[u8]) {
        assert!(self.len < INBOX_LEN, "inbox full; raise INBOX_LEN");
        let slot = &mut self.frames[self.len];
        slot.data[..bytes.len()].copy_from_slice(bytes);
        slot.len = bytes.len();
        self.len += 1;
    }
 
    pub fn frames(&self) -> &[Frame] {
        &self.frames[..self.len]
    }
}

