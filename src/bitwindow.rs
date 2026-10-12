use crate::consts::{SEQ_NO_RANGE,RESET_PROT, WINDOW_SIZE};
use crate::time::{Instant, has_timed_out};

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
    let diff = signed_diff(a,b);
    if diff >= 0{
        return false
    }
    return true
}

pub fn seq_after(a:u32,b:u32)-> bool{
    let diff = signed_diff(a,b);
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
        if !has_timed_out(reset_at,RESET_PROT, now) {
            return WindowCheck::Protected;
        }
    }
 
    *last_reset = Some(now);
    WindowCheck::Reset
}




#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitwindow_bits_test(){
        let window = BitWindow(0x0000FFFF);
        let val = window.bits();
        assert_eq!(val,0x0000FFFF)
    }

    #[test]
    fn bitwindow_new_test(){
        let window = BitWindow::new();
        assert_eq!(window.0,0);
    }

    #[test]
    fn bitwindow_contains_test(){
        let window = BitWindow(0x0000000F);
        let last = 0xFFFF;
        let seqno=0xFFF0;
        assert_eq!(window.contains(last, seqno), true);

        let seqno=0xFFFF;
        assert_eq!(window.contains(last, seqno), false);
    }
    #[test]
    fn bitwindow_set_test(){
        let mut window = BitWindow(0);
        window.set(0xFFFF);
        assert_eq!(window.0,0xFFFF);
    }
    #[test]
    fn window_protection_test() {
        let ws = WINDOW_SIZE as i32;
        let t0 = Instant::from_ticks(1_000u64);
        let mut last = None;

        assert_eq!(window_protection(0, ws, &mut last, t0), WindowCheck::Accept);
        assert_eq!(last, None);

        assert_eq!(window_protection(-ws, ws, &mut last, t0), WindowCheck::Reset);
        assert_eq!(last, Some(t0));

        assert_eq!(window_protection(-ws, ws, &mut last, t0), WindowCheck::Protected);

        let t1 = t0 + RESET_PROT * 2;
        assert_eq!(window_protection(-ws, ws, &mut last, t1), WindowCheck::Reset);
        assert_eq!(last, Some(t1));
    }
    #[test]
    fn bitwindow_count_test(){
        let window = BitWindow(0);
        let a = window.count();
        assert_eq!(a,0);
        let window1 = BitWindow(0x000F00F0);
        let b = window1.count();
        assert_eq!(b,8);

    }
    #[test]
    fn bitwindow_clear_test(){
        let mut window = BitWindow(0xFFF0FFFA);
        window.clear();
        assert_eq!(window.0,0);
    }
    

    #[test]
    fn update_test() {
        let mut window = BitWindow::new();

        assert_eq!(window.update(0, true), false);
        assert_eq!(window.bits(), 0x1);

        assert_eq!(window.update(1, true), true);
        assert_eq!(window.bits(), 0x3);

        assert_eq!(window.update(-3, true), false);
        assert_eq!(window.bits(), 0xB);

        assert_eq!(window.update(WINDOW_SIZE as i32, true), true);
        assert_eq!(window.bits(), 0x1);
    }

}

