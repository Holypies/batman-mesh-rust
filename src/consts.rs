use crate::time::{Duration, micros, millis, secs};

// window size
pub const WINDOW_SIZE:usize = 64;
pub const TQ_RING_SIZE:usize = 5;
pub const TQ_MAX:usize = 255;
pub const SEQ_NO_RANGE:usize = 65536;

// timer consts
pub const RESET_PROT:Duration = secs(30);               // 30 sec
pub const PURGE_TIME_OUT:Duration = secs(200);          // 200 sec
pub const PURGE_PERIOD: Duration = secs(1);             // 1 sec
pub const ORIG_INTER:Duration = secs(1);                // 1 sec
pub const JITTER:Duration = millis(20);             // 20 ms
pub const REBORADCAST_INTER:Duration = millis(5);   // 5 ms


// BATMAN hop consts
pub const HOP_PENALTY:u8 = 30;
pub const TIME_TO_LIVE:usize = 50;

// constants for ethernet header
pub const ETH_HLEN:usize = 14;
pub const ETH_VLAN_HLEN:usize = 18; 
pub const ETH_P_8021Q:u16 = 0x8100;
pub const VLAN_VID_MASK:u16= 0x0FFF;

// const for framebuffer
pub const FRAMEBUF_HEADROOM:usize = 28;

// reboradcasts
pub const NUM_BCASTS_WIRED: u8 = 1;
pub const NUM_BCASTS_WIRELESS: u8 = 3;
pub const NUM_BCASTS_MAX: u8 = 3;

// Originator consts
pub const MAX_NEIGHBORS:usize = 8;
pub const MAX_ORIG_TABLE_SIZE: usize = 8;
pub const ORIG_TIME_OUT:Duration = secs(200);


// BATMAN MESSAGE CONSTS
pub const MIN_ETH_FRAME_SIZE:usize = 60;
pub const MAX_BATMAN_FRAME_SIZE:usize=1514;

// Unicast frame consts
pub const UNICAST_INNER_OFFSET:usize = 24;
pub const UNICAST_HEADROOM:usize= 24;
pub const UNICAST_INNER_FRAME_LEN:usize = 1490;

// Broadcast frames constats
pub const BCAST_INNER_OFFSET:usize = 28;
pub const BCAST_HEADROOM:usize=28;
pub const BCAST_INNER_FRAME_LEN:usize = 1486;
//  