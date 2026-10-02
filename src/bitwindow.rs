use crate::consts::{SEQ_NO_RANGE,RESET_PROT, WINDOW_SIZE};
use crate::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitWindowResult {
    New, 
    OrigDup, 
    NeighDup, 
    Protected
}


// --- sequnce window ---
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BitWindow(u64);

impl BitWindow {
    
    pub const fn new() -> Self {
        BitWindow(0)
    }
 
    /// The raw bitmap, bit 0 = newest. For debugging and stats output.
    pub fn bits(&self) -> u64 {
        self.0
    }
    
    pub fn contains(&self, last: u32, seqno: u32) -> bool {
        let diff = signed_diff(last, seqno);
        if diff < 0 || diff >= WINDOW_SIZE as i32 {
            return false;
        }
        self.0 & (1u64 << (diff as u32)) != 0
    }
 
    pub fn set(&mut self, n: i32) {
        if n < 0 || n >= WINDOW_SIZE as i32 {
            return;
        }
        self.0 |= 1u64 << (n as u32);
    }
    /// Expects diff to be new-last
    pub fn update(&mut self, diff: i32, mark: bool) -> bool {
        if diff <= 0 && diff > -(WINDOW_SIZE as i32){
            // -diff cannot overflow: diff is in -63..=0 here.
            if mark {
                self.set(-diff);
            }
            return false;
        }
 
        if diff > 0 && diff < WINDOW_SIZE as i32 {
            // diff is 1..=63, so the shift never reaches the width of u64.
            self.0 <<= diff as u32;
        } else {
            self.0 = 0;
        }
 
        if mark {
            self.set(0);
        }
        true
    }

    pub fn count(&self) -> u8 {
        self.0.count_ones() as u8
    }
 
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}


// --- Sequnce number helper functions ---
/// Wrapping diff
pub fn signed_diff(a:u32, b:u32)->i32{
    let diff:i32 = a.wrapping_sub(b) as i32;
    diff
}

pub fn seq_before(a:u32,b:u32)-> bool{
    let diff = seq_signed_diff(a,b);
    if diff >= 0{
        return false
    }
    return true
}

pub fn seq_after(a:u32,b:u32)-> bool{
    let diff = seq_signed_diff(a,b);
    if diff > 0{
        return true
    }
    return false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCheck {
    Accept,
    Reset,
    Protected,
}
/// Expects diff to be new - last (possitive is newer)
pub fn window_protection(
    diff:i32,
    window_size:i32, 
    last_reset: &mut Option<Instant>,
    now: Instant, 
)->WindowCheck{
    let too_old: bool = diff <= -window_size;
    let too_new: bool = diff >= SEQ_NO_RANGE as i32;
 
    if !too_old && !too_new {
        return WindowCheck::Accept;
    }
 
    if let Some(reset_at) = *last_reset {
        if !reset_at.has_timed_out(RESET_PROT, now) {
            return WindowCheck::Protected;
        }
    }
 
    *last_reset = Some(now);
    WindowCheck::Reset
}

