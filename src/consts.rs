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
pub const HOP_PENALTY:usize = 30;
pub const TIME_TO_LIVE:usize = 50;

// constants for ethernet header
pub const ETH_HLEN:usize = 14;
pub const ETH_VLAN_HLEN:usize = 18; 
pub const ETH_P_8021Q:u16 = 0x8100;
pub const VLAN_VID_MASK:u16= 0x0FFF;

// const for framebuffer
pub const HEADROOM:usize = 28;

// reboradcasts
pub const NUM_BCASTS_WIRED: u8 = 1;
pub const NUM_BCASTS_WIRELESS: u8 = 3;
pub const NUM_BCASTS_MAX: u8 = 3;