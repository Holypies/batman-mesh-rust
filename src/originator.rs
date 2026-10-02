use crate::arrayvec::ArrayVec;
use crate::bitwindow::WindowCheck::Protected;
use crate::bitwindow::{BitWindow, BitWindowResult, WindowCheck, signed_diff, window_protection};
use crate::consts::{MAX_NEIGHBORS, TQ_RING_SIZE, WINDOW_SIZE, ORIG_TIME_OUT};
use crate::mac::Mac;
use crate::time::Instant;

pub struct Originator{
    mac: Mac,
    last_seen:Instant,
    router_id:Option<Mac>,
    last_seqno:u32,
    last_ttl:u8,
    last_seqno_forwarded:Option<u32>,
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
            last_seqno_forwarded:None,
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
    pub fn last_seqno_forwarded(&self)->Option<u32>{
        self.last_seqno_forwarded
    }
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
    pub fn set_last_seqno_forwarded(&mut self, last_seqno:u32){
        self.last_seqno_forwarded = Some(last_seqno);
    }


    // --- functions ---
    pub fn add_neighbor(&mut self, neigh:Neighbor, now:Instant){
         if self.neighbors.is_full(){
            return;
        }
        let new_mac = neigh.addr();
        for slot in &self.neighbors[..self.neighbors.len()]{
            if let Some(item) = slot {
                if item.mac == new_mac{
                    return;
                }
            }
        }
        neigh.touch(now);
        self.neighbors.push(neigh);
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
    pub fn avg_tq(&self)->u8{
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


