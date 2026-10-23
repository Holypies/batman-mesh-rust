use crate::arrayvec::ArrayVec;
use crate::bitwindow::WindowCheck::Protected;
use crate::bitwindow::{BitWindow, BitWindowResult, WindowCheck, signed_diff, window_protection};
use crate::consts::{MAX_NEIGHBORS, TQ_RING_SIZE, WINDOW_SIZE, ORIG_TIME_OUT};
use crate::mac::Mac;
use crate::time::Instant;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginatorError {
    NeighborListFull,
    ZeroTq,
    UnknownNeighbor,

}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Originator{
    mac: Mac,
    last_seen:Instant,
    router_id:Option<Mac>,
    last_seqno:u32,
    last_ttl:u8,
    // last_seqno_forwarded:Option<u32>, should not be needed here, but is saved in case
    time_seqno_reset:Option<Instant>,
    echo: BitWindow,
    bcast_bits:BitWindow,
    bcast_seqno_reset:Option<Instant>,
    last_bcast_seqno:u32,
    neighbors:ArrayVec<Neighbor,MAX_NEIGHBORS>,
    last_ttvn:u8,
}



impl Originator {
    // TODO: Implement new()
    pub const fn new(mac:Mac, now:Instant)->Self{
        Self{
            mac,
            last_seen:now,
            router_id:None,
            last_seqno:0,
            last_ttl:0,
          //  last_seqno_forwarded:None,
            time_seqno_reset:None,
            echo:BitWindow::new(),
            bcast_bits:BitWindow::new(),
            bcast_seqno_reset:None,
            last_bcast_seqno:0,
            neighbors:ArrayVec::new(),
            last_ttvn:0,
        }
    }
    // --- Getters ---
    pub fn mac(&self)->Mac{
        self.mac
    }
    pub fn last_seen(&self)->Instant{
        self.last_seen
    }
    pub fn router_id(&self)->Option<Mac>{
        self.router_id
    }
    pub fn last_seqno(&self)->u32{
        self.last_seqno
    }
    pub fn last_ttl(&self)->u8{
        self.last_ttl
    }
   // pub fn last_seqno_forwarded(&self)->Option<u32>{
   //     self.last_seqno_forwarded
   // }
    pub fn time_seqno_reset(&self)->Option<Instant>{
        self.time_seqno_reset
    }
    pub fn echo(&self)->u8{
        self.echo.count()
    }
    pub fn bcast_bits(&self)->BitWindow{
        self.bcast_bits
    }
    pub fn bcast_seqno_reset(&self)->Option<Instant>{
        self.bcast_seqno_reset
    }
    pub fn last_bcast_seqno(&self)->u32{
        self.last_bcast_seqno
    }
    pub fn neighbors(&self)->&ArrayVec<Neighbor,MAX_NEIGHBORS>{
        &self.neighbors
    }
    pub fn last_ttvn(&self)->u8{
        self.last_ttvn
    }


    // --- Setters ---
    pub fn set_router(&mut self, mac:Mac){
        self.router_id = Some(mac)
    }
    pub fn set_last_ttvn(&mut self, ttvn:u8){
        self.last_ttvn = ttvn
    }
   // pub fn set_last_seqno_forwarded(&mut self, last_seqno:u32){
   //     self.last_seqno_forwarded = Some(last_seqno);
   // }


    // --- functions ---
    // TODO: fix .unwrap(); to match for panic free code
    pub fn add_neighbor(&mut self, mut neigh:Neighbor, now:Instant)->Result<(),OriginatorError>{
        if let Some(existing) = self.find_neighbor_mut(neigh.addr()) {
            existing.touch(now);
            return Ok(());
        }

        // New: refresh it, then store it.
        neigh.touch(now);
        match self.neighbors.push(neigh) {
            Ok(_) => Ok(()),
            Err(_) => Err(OriginatorError::NeighborListFull),
        }
    }

    pub fn touch(&mut self, now:Instant){
        self.last_seen=now
    }

    pub fn find_neighbor(&self, addr:Mac)->Option<&Neighbor>{
        for neighbor in self.neighbors.iter() {
            if neighbor.addr == addr {
                return Some(neighbor);
            }
        }
        None
    }

    pub fn find_neighbor_mut(&mut self, addr:Mac)->Option<&mut Neighbor>{
        for neighbor in self.neighbors.iter_mut() {
            if neighbor.addr == addr {
                return Some(neighbor);
            }
        }
        None
    }


    pub fn slide_echo_window(&mut self) {
        self.echo.update(1, false);
    }
    pub fn record_echo(&mut self, offset:i32){
        self.echo.set(offset);
    }

    pub fn update_seqno(&mut self,seqno:u32, sender:Mac, ttl:u8, now:Instant)->BitWindowResult{

        let diff: i32 = signed_diff(seqno, self.last_seqno);
        let window_check:WindowCheck = window_protection(
            diff, 
            WINDOW_SIZE as i32,
            &mut self.time_seqno_reset,
            now
        );

        if window_check == Protected{
            return BitWindowResult::Protected;
        }

        let mut status: BitWindowResult = BitWindowResult::New;
        let mut window_moved: bool = false;

        for neighbor in self.neighbors.iter_mut() {

            let is_sender = neighbor.addr() == sender;

            if neighbor.is_duplicate(self.last_seqno, seqno){

                if is_sender {
                    status = BitWindowResult::NeighDup;

                } else if status != BitWindowResult::NeighDup {
                    status = BitWindowResult::OrigDup;

                }
            }
            window_moved |= neighbor.slide_window(diff, is_sender);
        }

        if window_moved {
            self.last_seqno = seqno;
            self.last_ttl = ttl;
            
        } 

        status
    }

    pub fn check_bcast(&mut self, seqno:u32, now:Instant)->bool{
        let is_doup: bool = self.bcast_bits.contains(self.last_bcast_seqno, seqno);

        if is_doup{
            return false;
        }

        let diff: i32 = signed_diff(seqno, self.last_bcast_seqno);
        let window_check:WindowCheck = window_protection(
            diff, 
            WINDOW_SIZE as i32,
            &mut self.bcast_seqno_reset,
            now
        );

        if window_check==Protected{
            return false;
        }

        if self.bcast_bits.update(diff, true){
            self.last_bcast_seqno = seqno;
        }
        
        return true;

    }


    pub fn router_neighbor(&self)->Option<&Neighbor>{
        for neighbor in self.neighbors.iter() {
            if Some(neighbor.addr) == self.router_id {
                return Some(neighbor);
            }
        }
        None
    }

    pub fn best_neighbor(&self)->Option<Mac>{
        let mut best_neighbor: &Neighbor = match self.neighbors.get(0) {
            Some(neighbor) => neighbor,
            None => return None,
        };

        for neighbor in self.neighbors.iter() {
            if neighbor.tq.avg() > best_neighbor.tq.avg() {
                best_neighbor = neighbor;
            }
        }
        if best_neighbor.tq.avg() == 0{
            return None;
        }
        Some(best_neighbor.addr())
    }

    pub fn purge_timed_out_neighbors(&mut self, now:Instant)->bool{
        let mut removed_router: bool=false;
        
        let mut i: usize = 0;
        while i < self.neighbors.len() {
            let timed_out: bool = match self.neighbors.get(i) {
                Some(neighbor) => neighbor.has_timed_out(now),
                None => false,
            };

            if timed_out {
                match self.neighbors.swap_remove(i) {
                    Some(removed) => {
                        if Some(removed.addr()) == self.router_id {
                            removed_router = true;
                        }
                    }
                    None => {}
                }
            
            } else {
                i += 1;
            }
        }
        if removed_router{
            self.router_id = self.best_neighbor();
        }
        removed_router
    }

    pub fn has_timed_out(&self, now:Instant)->bool{
        let time_out = self.last_seen+ORIG_TIME_OUT;
        if now>time_out{
            return true;
        }        
        return false;
    }

    pub fn update_new_ogm_tq(&mut self, mac:Mac){
        for neigh in self.neighbors.iter_mut(){
            if neigh.addr()!=mac{
                neigh.record_transmit_quality(0);
            }
        }
    }


}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neighbor{
    addr:Mac,
    last_seen:Instant,
    real_bits:BitWindow,
    tq: TqRing,
    last_ttl: u8,
}
impl Neighbor {
    pub fn new(addr:Mac, now:Instant)->Self{
        Self{
            addr,
            last_seen: now,
            real_bits:BitWindow::new(),
            tq:TqRing::new(),
            last_ttl:0
        }    
    }

    // --- Getters --- 
    pub fn addr(&self)->Mac{
        self.addr
    }
    pub fn last_seen(&self)->Instant{
        self.last_seen
    }
    pub fn real_bits(&self)->BitWindow{
        self.real_bits
    }
    pub fn tq(&self)-> TqRing{
        self.tq
    }
    pub fn last_ttl(&self)-> u8{
        self.last_ttl
    }

    // --- setters ---
    pub fn set_last_ttl(&mut self, ttl: u8){
        self.last_ttl=ttl
    }

    // --- functions---
    pub fn tq_avg(&self)->u8{
        self.tq.avg()
    }

    pub fn real_bits_count(&self)->u8{
        self.real_bits.count()
    }

    pub fn touch(&mut self, now:Instant){
        self.last_seen=now
    }
    pub fn record_transmit_quality(&mut self, tq:u8){
        self.tq.push(tq);
        self.tq.compute_avg();
    }

    pub fn slide_window(&mut self, diff:i32, mark:bool)->bool {
        self.real_bits.update(diff, mark)
    }

    pub fn is_duplicate(&self, last_seqno:u32, curr_seqno:u32)-> bool{
        self.real_bits.contains(last_seqno, curr_seqno)
    }

    pub fn has_timed_out(&self, now:Instant)->bool{
        // TODO: implement Time out const for neighbor
        let time_out = self.last_seen+ORIG_TIME_OUT;

        if now>time_out{
            return true;
        }        
        return false;
    }
}




#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TqRing{
    values: [u8;TQ_RING_SIZE],
    index: u8,
    avg:u8,
}

    
impl TqRing {
    pub const fn new() -> Self {
        TqRing {
            values: [0; TQ_RING_SIZE],
            index: 0,
            avg: 0,
        }
    }
 
    pub fn push(&mut self, tq: u8) {
        self.values[self.index as usize] = tq;
        self.index = ((self.index as usize + 1) % TQ_RING_SIZE) as u8;
        self.avg = self.compute_avg();
    }
 
    pub const fn avg(&self) -> u8 {
        self.avg
    }
 
    pub const fn values(&self) -> &[u8; TQ_RING_SIZE] {
        &self.values
    }
 
    fn compute_avg(&self) -> u8 {
        let mut sum: u16 = 0;
        let mut count: u16 = 0;
        for &v in self.values.iter() {
            if v != 0 {
                sum += v as u16;
                count += 1;
            }
        }
        if count == 0 {
            return 0;
        }
        (sum / count) as u8
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::time::micros;

    const STP_MAC:Mac = Mac::STP;
    const BROADCAST_MAC: Mac = Mac::BROADCAST;
    const ZERO_MAC: Mac = Mac::ZERO;
    const NOW:Instant = Instant::from_ticks(0);
    const THEN:Instant = Instant::from_ticks(100);

    // neighbor with tq avg forced, bypasses TqRing logic
    fn neigh_tq(addr: Mac, avg: u8) -> Neighbor {
        let mut n = Neighbor::new(addr, NOW);
        n.tq.avg = avg;
        n
    }

    // originator with neighbors pushed directly, bypasses add_neighbor
    fn orig_with(neighs: &[Neighbor]) -> Originator {
        let mut orig = Originator::new(STP_MAC, NOW);
        for n in neighs {
            orig.neighbors.push(*n).unwrap();
        }
        orig
    }

    fn full_ring(addr: Mac) -> Neighbor {
        let mut neigh = Neighbor::new(addr, NOW);
        neigh.tq.values = [100; TQ_RING_SIZE];
        neigh.tq.avg = 100;
        neigh
    }
    

    fn mac(n: u8) -> Mac {
        Mac::from_octets([0x02, 0x00, 0x00, 0x00, 0x00, n])
    }
    
    // --- Originator tests ---
    #[test]
    fn add_neighbor_test(){
        let mut orig = Originator::new(STP_MAC, NOW);

        // new neighbor: Ok, stored once
        assert_eq!(orig.add_neighbor(Neighbor::new(BROADCAST_MAC, NOW), NOW), Ok(()));
        assert_eq!(orig.neighbors.len(), 1);
        assert_eq!(orig.neighbors.get(0).unwrap().addr, BROADCAST_MAC);

        // same addr again: Ok, no duplicate entry
        assert_eq!(orig.add_neighbor(Neighbor::new(BROADCAST_MAC, NOW), THEN), Ok(()));
        assert_eq!(orig.neighbors.len(), 1);
        assert_eq!(orig.neighbors.get(0).unwrap().last_seen, THEN); 

        // second distinct addr: appended
        let mac2 = Mac::from_octets([0x11;6]);
        assert_eq!(orig.add_neighbor(Neighbor::new(mac2, NOW), NOW), Ok(()));
        assert_eq!(orig.neighbors.len(), 2);
        assert_eq!(orig.neighbors.get(1).unwrap().addr, mac2);

        // fill to capacity
        let mut full = Originator::new(STP_MAC, NOW);
        for i in 0..MAX_NEIGHBORS {
            assert_eq!(full.add_neighbor(Neighbor::new(mac(i as u8 + 1), NOW), NOW), Ok(()));
        }
        assert_eq!(full.neighbors.len(), MAX_NEIGHBORS);

        // one more distinct: NeighborListFull, list unchanged
        let extra = mac(MAX_NEIGHBORS as u8 + 1);
        assert_eq!(
            full.add_neighbor(Neighbor::new(extra, NOW), NOW),
            Err(OriginatorError::NeighborListFull)
        );
        assert_eq!(full.neighbors.len(), MAX_NEIGHBORS);
        assert!(full.find_neighbor(extra).is_none());

    }

    #[test]
    fn touch_test(){
        let mut orig = orig_with(&[Neighbor::new(mac(1), NOW)]);
        let t = Instant::from_ticks(5_000);

        orig.touch(t);
        assert_eq!(orig.last_seen, t);

        // neighbors untouched
        assert_eq!(orig.neighbors.get(0).unwrap().last_seen, NOW);
    }

    #[test]
    fn find_neighbor_test(){
        // empty
        let empty = Originator::new(STP_MAC, NOW);
        assert!(empty.find_neighbor(mac(1)).is_none());

        
        let first = Neighbor::new(mac(2), NOW);
        let second = Neighbor::new(mac(3), NOW);
        let orig = orig_with(&[Neighbor::new(mac(1), NOW), first, second]);

        assert_eq!(orig.find_neighbor(mac(1)).unwrap().addr, mac(1));
        assert_eq!(orig.find_neighbor(mac(2)).unwrap(), &first);
        assert_eq!(orig.find_neighbor(mac(3)).unwrap(), &second);

        // unknown addr
        assert!(orig.find_neighbor(ZERO_MAC).is_none());
    }

    #[test]
    fn find_neighbor_mut_test(){
       // empty
        let mut empty = Originator::new(STP_MAC, NOW);
        assert!(empty.find_neighbor_mut(mac(1)).is_none());

        
        let first = Neighbor::new(mac(2), NOW);
        let second = Neighbor::new(mac(3), NOW);
        let mut orig = orig_with(&[Neighbor::new(mac(1), NOW), first, second]);

        let one = orig.find_neighbor_mut(mac(1)).unwrap();
        assert_eq!(one.addr, mac(1));
        one.touch(THEN);

        let two = orig.find_neighbor_mut(mac(2)).unwrap();
        assert_eq!(two.addr, mac(2));
        two.touch(THEN);

        let three = orig.find_neighbor_mut(mac(3)).unwrap();
        assert_eq!(three.addr, mac(3));
        three.touch(THEN);
        
        // Re-fetch to verify mutations persisted inside `orig`
        assert_eq!(orig.find_neighbor_mut(mac(1)).unwrap().last_seen, THEN);
        assert_eq!(orig.find_neighbor_mut(mac(2)).unwrap().last_seen, THEN);
        assert_eq!(orig.find_neighbor_mut(mac(3)).unwrap().last_seen, THEN);


        // unknown addr
        assert!(orig.find_neighbor_mut(ZERO_MAC).is_none());

    }

    #[test]
    fn slide_echo_window_test() {
        let mut orig = Originator::new(STP_MAC, NOW);
        assert_eq!(orig.echo(), 0);

        // slide shifts existing bit, does not drop it
        orig.record_echo(0);
        orig.slide_echo_window();
        assert_eq!(orig.echo(), 1);

        // slot 0 freed by slide: can mark again
        orig.record_echo(0);
        assert_eq!(orig.echo(), 2);

        // slide never marks
        orig.slide_echo_window();
        assert_eq!(orig.echo(), 2);

        // full window of slides clears everything
        for _ in 0..WINDOW_SIZE {
            orig.slide_echo_window();
        }
        assert_eq!(orig.echo(), 0);

        // other windows / seqno untouched
        assert_eq!(orig.bcast_bits.count(), 0);
        assert_eq!(orig.last_seqno, 0);
    }

    #[test]
    fn record_echo_test(){
        let mut orig = Originator::new(STP_MAC, NOW);
        assert_eq!(orig.echo(), 0);

        orig.record_echo(0);
        assert_eq!(orig.echo(), 1);

        // same offset twice: idempotent
        orig.record_echo(0);
        assert_eq!(orig.echo(), 1);

        orig.record_echo(3);
        assert_eq!(orig.echo(), 2);

        // last slot in window
        orig.record_echo(WINDOW_SIZE as i32 - 1);
        assert_eq!(orig.echo(), 3);

        // bcast window untouched
        assert_eq!(orig.bcast_bits.count(), 0);
    }

    #[test]
    fn update_seqno_test(){
        let a = mac(1);
        let b = mac(2);
        let mut orig = orig_with(&[Neighbor::new(a, NOW), Neighbor::new(b, NOW)]);

        // newer seqno via A: New, window moves, seqno + ttl stored
        assert_eq!(orig.update_seqno(1, a, 50, NOW), BitWindowResult::New);
        assert_eq!(orig.last_seqno, 1);
        assert_eq!(orig.last_ttl, 50);
        assert_eq!(orig.find_neighbor(a).unwrap().real_bits_count(), 1);
        assert_eq!(orig.find_neighbor(b).unwrap().real_bits_count(), 0);

        // same seqno again via A: NeighDup, nothing stored
        assert_eq!(orig.update_seqno(1, a, 40, NOW), BitWindowResult::NeighDup);
        assert_eq!(orig.last_seqno, 1);
        assert_eq!(orig.last_ttl, 50);

        // same seqno via B, first time from B: OrigDup, B now marked
        assert_eq!(orig.update_seqno(1, b, 40, NOW), BitWindowResult::OrigDup);
        assert_eq!(orig.find_neighbor(b).unwrap().real_bits_count(), 1);
        assert_eq!(orig.last_ttl, 50);

        // sender first in list + other dup: NeighDup not downgraded
        assert_eq!(orig.update_seqno(1, a, 40, NOW), BitWindowResult::NeighDup);

        // sender second in list + earlier OrigDup: NeighDup overrides
        assert_eq!(orig.update_seqno(1, b, 40, NOW), BitWindowResult::NeighDup);

        // jump ahead: New, all windows slide, only sender marked
        assert_eq!(orig.update_seqno(3, a, 30, NOW), BitWindowResult::New);
        assert_eq!(orig.last_seqno, 3);
        assert_eq!(orig.last_ttl, 30);
        assert_eq!(orig.find_neighbor(a).unwrap().real_bits_count(), 2);
        assert_eq!(orig.find_neighbor(b).unwrap().real_bits_count(), 1);

        // older unseen seqno inside window: New, seqno + ttl unchanged
        assert_eq!(orig.update_seqno(2, b, 20, NOW), BitWindowResult::New);
        assert_eq!(orig.last_seqno, 3);
        assert_eq!(orig.last_ttl, 30);
        assert_eq!(orig.find_neighbor(b).unwrap().real_bits_count(), 2);

        // that older seqno now dup: via B NeighDup, via A OrigDup
        assert_eq!(orig.update_seqno(2, b, 20, NOW), BitWindowResult::NeighDup);
        assert_eq!(orig.update_seqno(2, a, 20, NOW), BitWindowResult::OrigDup);
    }
    
    #[test]
    fn check_bcast_test(){
        let mut orig = Originator::new(STP_MAC, NOW);

        // seqno 0 on fresh window: accepted, then dup
        assert_eq!(orig.check_bcast(0, NOW), true);
        assert_eq!(orig.check_bcast(0, NOW), false);
        assert_eq!(orig.last_bcast_seqno, 0);

        // newer: accepted, window moves
        assert_eq!(orig.check_bcast(1, NOW), true);
        assert_eq!(orig.last_bcast_seqno, 1);
        assert_eq!(orig.check_bcast(1, NOW), false);

        // older already seen: rejected
        assert_eq!(orig.check_bcast(0, NOW), false);

        // jump ahead: accepted
        assert_eq!(orig.check_bcast(5, NOW), true);
        assert_eq!(orig.last_bcast_seqno, 5);

        // older unseen inside window: accepted, last seqno unchanged
        assert_eq!(orig.check_bcast(3, NOW), true);
        assert_eq!(orig.last_bcast_seqno, 5);
        assert_eq!(orig.check_bcast(3, NOW), false);

        // ogm seqno state untouched
        assert_eq!(orig.last_seqno, 0);
        assert_eq!(orig.echo(), 0);
    }

    #[test]
    fn router_neighbor_test(){
        let mut orig = orig_with(&[Neighbor::new(mac(1), NOW), Neighbor::new(mac(2), NOW)]);

        // no router set
        assert!(orig.router_neighbor().is_none());

        // router not in list
        orig.router_id = Some(BROADCAST_MAC);
        assert!(orig.router_neighbor().is_none());

        // router in list
        orig.router_id = Some(mac(2));
        assert_eq!(orig.router_neighbor().unwrap().addr, mac(2));
        assert_ne!(orig.router_neighbor().unwrap().addr, mac(1));

        orig.router_id = Some(mac(1));
        assert_eq!(orig.router_neighbor().unwrap().addr, mac(1));
        assert_ne!(orig.router_neighbor().unwrap().addr, mac(2));

        // router set, empty list
        let mut empty = Originator::new(STP_MAC, NOW);
        empty.router_id = Some(mac(1));
        assert!(empty.router_neighbor().is_none());
    }

    #[test]
    fn best_neighbor_test(){
        // empty list
        assert_eq!(Originator::new(STP_MAC, NOW).best_neighbor(), None);

        // all tq 0
        let orig = orig_with(
            &[
                neigh_tq(mac(1), 0), 
                neigh_tq(mac(2), 0)
            ]
        );
        assert_eq!(orig.best_neighbor(), None);

        // highest in middle
        let orig = orig_with(
            &[
                neigh_tq(mac(1), 50), 
                neigh_tq(mac(2), 200), 
                neigh_tq(mac(3), 100)
            ]
        );
        assert_eq!(orig.best_neighbor(), Some(mac(2)));

        // highest first
        let orig = orig_with(
            &[
                neigh_tq(mac(1), 200),
                neigh_tq(mac(2), 50)
            ]
        );
        assert_eq!(orig.best_neighbor(), Some(mac(1)));

        // highest last
        let orig = orig_with(
            &[
                neigh_tq(mac(1), 10), 
                neigh_tq(mac(2), 20)
            ]
        );
        assert_eq!(orig.best_neighbor(), Some(mac(2)));

        // tie: first wins
        let orig = orig_with(
            &[
                neigh_tq(mac(1), 100), 
                neigh_tq(mac(2), 100)
            ]
        );
        assert_eq!(orig.best_neighbor(), Some(mac(1)));

        // TODO: DOUBLE CHECK THAT IT NEEDS TO IGNORE current router
        let mut orig = orig_with(
            &[
                neigh_tq(mac(1), 10), 
                neigh_tq(mac(2), 20)
            ]
        );
        orig.router_id = Some(mac(1));
        assert_eq!(orig.best_neighbor(), Some(mac(2)));

    }

    #[test]
    fn purge_timed_out_neighbors_test(){
        let past = NOW + ORIG_TIME_OUT + micros(1);

        // nothing timed out: false, all kept
        let mut orig = orig_with(&[Neighbor::new(mac(1), past), Neighbor::new(mac(2), past)]);
        orig.router_id = Some(mac(1));
        assert_eq!(orig.purge_timed_out_neighbors(past), false);
        assert_eq!(orig.neighbors.len(), 2);
        assert_eq!(orig.router_id, Some(mac(1)));

        // non-router timed out: removed, false, router kept
        let mut orig = orig_with(&[Neighbor::new(mac(1), past), Neighbor::new(mac(2), NOW)]);
        orig.router_id = Some(mac(1));
        assert_eq!(orig.purge_timed_out_neighbors(past), false);
        assert_eq!(orig.neighbors.len(), 1);
        assert!(orig.find_neighbor(mac(2)).is_none());
        assert_eq!(orig.router_id, Some(mac(1)));

        // router timed out: removed, true, router -> best survivor
        let mut stale_router = Neighbor::new(mac(1), NOW);
        stale_router.tq.avg = 250;
        let mut weak = Neighbor::new(mac(2), past);
        weak.tq.avg = 50;
        let mut strong = Neighbor::new(mac(3), past);
        strong.tq.avg = 200;
        let mut orig = orig_with(&[stale_router, weak, strong]);
        orig.router_id = Some(mac(1));
        assert_eq!(orig.purge_timed_out_neighbors(past), true);
        assert_eq!(orig.neighbors.len(), 2);
        assert!(orig.find_neighbor(mac(1)).is_none());
        assert_eq!(orig.router_id, Some(mac(3)));

        // router timed out, survivors all tq 0: router None
        let mut orig = orig_with(&[Neighbor::new(mac(1), NOW), Neighbor::new(mac(2), past)]);
        orig.router_id = Some(mac(1));
        assert_eq!(orig.purge_timed_out_neighbors(past), true);
        assert_eq!(orig.neighbors.len(), 1);
        assert_eq!(orig.router_id, None);

        // router timed out, list empty after: router None
        let mut orig = orig_with(&[Neighbor::new(mac(1), NOW)]);
        orig.router_id = Some(mac(1));
        assert_eq!(orig.purge_timed_out_neighbors(past), true);
        assert_eq!(orig.neighbors.len(), 0);
        assert_eq!(orig.router_id, None);

        // consecutive + trailing stale (swap_remove path): only fresh survive
        let mut orig = orig_with(&[
            Neighbor::new(mac(1), NOW),
            Neighbor::new(mac(2), NOW),
            Neighbor::new(mac(3), past),
            Neighbor::new(mac(4), NOW),
        ]);
        assert_eq!(orig.purge_timed_out_neighbors(past), false);
        assert_eq!(orig.neighbors.len(), 1);
        assert!(orig.find_neighbor(mac(3)).is_some());
    }

    #[test]
    fn has_timed_out_test(){
        let mut orig = Originator::new(STP_MAC, NOW);
        assert!(!orig.has_timed_out(NOW));

        // exactly at boundary: not timed out (strict >)
        let boundary = NOW + ORIG_TIME_OUT;
        assert!(!orig.has_timed_out(boundary));

        // just past boundary
        let past = boundary + micros(1);
        assert!(orig.has_timed_out(past));

        // touch resets
        orig.touch(past);
        assert!(!orig.has_timed_out(past));

        // neighbors' last_seen irrelevant
        let mut orig = orig_with(&[Neighbor::new(mac(1), past)]);
        orig.last_seen = NOW;
        assert!(orig.has_timed_out(past));
    }

    #[test]
    fn update_new_ogm_tq_test(){
        assert!(TQ_RING_SIZE >= 2, "test assumes TQ_RING_SIZE >= 2");


        let mut orig = orig_with(
            &[
                full_ring(mac(1)), 
                full_ring(mac(2)), 
                full_ring(mac(3))
            ]
        );

        // sender's ring untouched, others get one 0 pushed
        orig.update_new_ogm_tq(mac(1));
        let a = orig.find_neighbor(mac(1)).unwrap();
        assert_eq!(a.tq.values, [100u8; TQ_RING_SIZE]);
        assert_eq!(a.tq.index, 0);

        for &addr in [mac(2), mac(3)].iter() {
            let n = orig.find_neighbor(addr).unwrap();
            assert_eq!(n.tq.values[0], 0);
            assert_eq!(n.tq.index, 1);
            // zeros ignored by avg: unchanged while nonzero slots remain
            assert_eq!(n.tq.avg, 100);
        }

        // repeat until rings all zero: avg 0, sender keeps 100
        for _ in 1..TQ_RING_SIZE {
            orig.update_new_ogm_tq(mac(1));
        }
        assert_eq!(orig.find_neighbor(mac(1)).unwrap().tq.avg, 100);
        assert_eq!(orig.find_neighbor(mac(2)).unwrap().tq.avg, 0);
        assert_eq!(orig.find_neighbor(mac(3)).unwrap().tq.avg, 0);

        // unknown mac: every neighbor penalised
        let mut orig = orig_with(&[full_ring(mac(1)), full_ring(mac(2))]);
        orig.update_new_ogm_tq(ZERO_MAC);
        assert_eq!(orig.find_neighbor(mac(1)).unwrap().tq.index, 1);
        assert_eq!(orig.find_neighbor(mac(2)).unwrap().tq.index, 1);
    }


    // --- Neighbor tests ---
    #[test]
    fn tq_avg_test(){
        let mut n = Neighbor::new(mac(1), NOW);
        assert_eq!(n.tq_avg(), 0);

        // reads ring's cached avg
        n.tq.avg = 123;
        assert_eq!(n.tq_avg(), 123);
    }

    #[test]
    fn real_bits_count_test(){
        let mut n = Neighbor::new(mac(1), NOW);
        assert_eq!(n.real_bits_count(), 0);

        n.real_bits.set(0);
        assert_eq!(n.real_bits_count(), 1);

        n.real_bits.set(5);
        assert_eq!(n.real_bits_count(), 2);

        assert_eq!(n.real_bits_count(), n.real_bits.count());
    }

    #[test]
    fn touch_neighbor_test(){
        let mut n = Neighbor::new(mac(1), NOW);
        let t = Instant::from_ticks(5_000);

        n.touch(t);
        assert_eq!(n.last_seen, t);

        // other fields untouched
        assert_eq!(n.addr, mac(1));
        assert_eq!(n.tq, TqRing::new());
        assert_eq!(n.real_bits, BitWindow::new());
        assert_eq!(n.last_ttl, 0);
    }

    #[test]
    fn record_transmit_quality_test(){
        assert!(TQ_RING_SIZE >= 2, "test assumes TQ_RING_SIZE >= 2");
        let mut neigh = Neighbor::new(mac(1), NOW);

        neigh.record_transmit_quality(100);
        assert_eq!(neigh.tq.values[0], 100);
        assert_eq!(neigh.tq_avg(), 100);

        neigh.record_transmit_quality(50);
        assert_eq!(neigh.tq.values[1], 50);
        assert_eq!(neigh.tq_avg(), 75);

        // zeros fill rest: stored but ignored by avg
        for _ in 2..TQ_RING_SIZE {
            neigh.record_transmit_quality(0);
        }
        assert_eq!(neigh.tq_avg(), 75);

        // wrap: overwrites slot 0 (100 -> 0)
        neigh.record_transmit_quality(0);
        assert_eq!(neigh.tq.values[0], 0);
        assert_eq!(neigh.tq_avg(), 50);

        // overwrite slot 1: all zero
        neigh.record_transmit_quality(0);
        assert_eq!(neigh.tq_avg(), 0);
    }

    #[test]
    fn slide_window_test() {
        let mut neigh = Neighbor::new(mac(1), NOW);

        // forward, marked: moved
        assert_eq!(neigh.slide_window(1, true), true);
        assert_eq!(neigh.real_bits_count(), 1);

        // same seqno, already marked: not moved, no change
        assert_eq!(neigh.slide_window(0, true), false);
        assert_eq!(neigh.real_bits_count(), 1);

        // forward, unmarked: moved, nothing new marked
        assert_eq!(neigh.slide_window(1, false), true);
        assert_eq!(neigh.real_bits_count(), 1);

        // current slot now free: marks without moving
        assert_eq!(neigh.slide_window(0, true), false);
        assert_eq!(neigh.real_bits_count(), 2);

        // older seqno inside window: marks without moving
        assert_eq!(neigh.slide_window(-3, true), false);
        assert_eq!(neigh.real_bits_count(), 3);

        // jump >= WINDOW_SIZE: moved, window reset
        assert_eq!(neigh.slide_window(WINDOW_SIZE as i32, true), true);
        assert_eq!(neigh.real_bits_count(), 1);
    }

    #[test]
    fn is_duplicate_test(){
        let mut neigh = Neighbor::new(mac(1), NOW);

        // empty window: nothing dup
        assert!(!neigh.is_duplicate(0, 0));

        // mark current (last = 10)
        neigh.real_bits.set(0);
        assert!(neigh.is_duplicate(10, 10));
        // older, unmarked
        assert!(!neigh.is_duplicate(10, 9));
        // future
        assert!(!neigh.is_duplicate(10, 11));

        // older, marked
        neigh.real_bits.set(2);
        assert!(neigh.is_duplicate(10, 8));

        // exactly WINDOW_SIZE behind: outside window (catches modulo bug, bit 0 is set)
        assert!(!neigh.is_duplicate(10 + WINDOW_SIZE as u32, 10));

        // seqno wraparound: last = 0, curr = u32::MAX is 1 behind
        let mut wrapped = Neighbor::new(mac(2), NOW);
        wrapped.real_bits.set(1);
        assert!(wrapped.is_duplicate(0, u32::MAX));
    }

    #[test]
    fn has_timed_out_nighbor_test(){
        let mut neigh = Neighbor::new(mac(1), NOW);
        assert!(!neigh.has_timed_out(NOW));

        // exactly at boundary: not timed out (strict >)
        let boundary = NOW + ORIG_TIME_OUT;
        assert!(!neigh.has_timed_out(boundary));

        // just past boundary
        let past = boundary + micros(1);
        assert!(neigh.has_timed_out(past));

        // touch resets
        neigh.touch(past);
        assert!(!neigh.has_timed_out(past));
    }


    // --- Tq ring tests ---
    #[test]
    fn new_tq_ring_test(){
        let ring = TqRing::new();
        assert_eq!(ring.values, [0u8; TQ_RING_SIZE]);
        assert_eq!(ring.index, 0);
        assert_eq!(ring.avg, 0);
        assert_eq!(ring, TqRing::default());

    }

    #[test]
    fn push_test() {
        assert!(TQ_RING_SIZE >= 2, "test assumes TQ_RING_SIZE >= 2");

        let mut ring = TqRing::new();
        ring.push(10);
        assert_eq!(ring.values[0], 10);
        assert_eq!(ring.index, 1);
        assert_eq!(ring.avg, 10);

        ring.push(20);
        assert_eq!(ring.values[1], 20);
        assert_eq!(ring.index, (2 % TQ_RING_SIZE) as u8);
        assert_eq!(ring.avg, 15);

        // fill whole ring: index wraps to 0
        let mut ring = TqRing::new();
        for i in 0..TQ_RING_SIZE {
            ring.push((i + 1) as u8);
        }
        assert_eq!(ring.index, 0);
        for i in 0..TQ_RING_SIZE {
            assert_eq!(ring.values[i], (i + 1) as u8);
        }
        let expected = ((1..=TQ_RING_SIZE).sum::<usize>() / TQ_RING_SIZE) as u8;
        assert_eq!(ring.avg, expected);

        // next push overwrites oldest (slot 0), rest untouched
        ring.push(200);
        assert_eq!(ring.values[0], 200);
        assert_eq!(ring.values[1], 2);
        assert_eq!(ring.index, 1);

    }
    
    #[test]
    fn avg_test(){
        let mut ring = TqRing::new();
        assert_eq!(ring.avg(), 0);

        ring.avg = 42;
        assert_eq!(ring.avg(), 42);

        // returns cached value, does not recompute
        ring.values[0] = 200;
        assert_eq!(ring.avg(), 42);

    }

    #[test]
    fn values_test(){
        let mut ring = TqRing::new();
        assert_eq!(ring.values(), &[0u8; TQ_RING_SIZE]);

        ring.values[0] = 7;
        ring.values[TQ_RING_SIZE - 1] = 9;
        assert_eq!(ring.values()[0], 7);
        assert_eq!(ring.values()[TQ_RING_SIZE - 1], 9);
        assert_eq!(ring.values().len(), TQ_RING_SIZE);
    }
    
    #[test]
    fn compute_avg_test() {
        assert!(TQ_RING_SIZE >= 2, "test assumes TQ_RING_SIZE >= 2");
        let mut ring = TqRing::new();

        // all zero: 0, no div by zero
        assert_eq!(ring.compute_avg(), 0);

        // zeros ignored
        ring.values[0] = 100;
        assert_eq!(ring.compute_avg(), 100);

        ring.values[1] = 50;
        assert_eq!(ring.compute_avg(), 75);

        // integer floor: 151 / 2
        ring.values[1] = 51;
        assert_eq!(ring.compute_avg(), 75);

        // all max: u16 sum no overflow
        ring.values = [255; TQ_RING_SIZE];
        assert_eq!(ring.compute_avg(), 255);

        // pure: cached avg field untouched
        assert_eq!(ring.avg, 0);
    }
}
