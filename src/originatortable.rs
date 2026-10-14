use crate::arrayvec::ArrayVec;
use crate::originator::{Originator};
use crate::mac::Mac;
use crate::time::Instant;
use crate::consts::MAX_NEIGHBORS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginatorTableError {
    TableFull,
    Protected,
    NotBestNextHop,
    AlreadyInTable
}


pub struct OriginatorTable{
    list:ArrayVec<Originator,MAX_NEIGHBORS>

}

impl OriginatorTable{
    pub const fn new()->Self{
        Self{
            list:ArrayVec::new(),
        }
    }
    pub fn find_mac(&self, mac: Mac) -> Option<&Originator> {
        for originator in self.list.iter() {
            if originator.mac() == mac {
                return Some(originator);
            }
        }
        None
    }
     pub fn find_mac_mut(&mut self, mac: Mac) -> Option<&mut Originator> {
        for originator in self.list.iter_mut() {
            if originator.mac() == mac {
                return Some(originator);
            }
        }
        None
    }
    
    pub fn add(&mut self, mut orig:Originator, now: Instant)->Result<(), OriginatorTableError>{
        if self.list.is_full(){
            return Err(OriginatorTableError::TableFull);
        }
       
        if self.find_mac(orig.mac()).is_some() {
            return Err(OriginatorTableError::AlreadyInTable);
        }

        orig.touch(now);
        match self.list.push(orig) {
            Ok(_) => Ok(()),
            Err(_) => Err(OriginatorTableError::TableFull),
        }

    }
    
    // TODO: fix remove
    pub fn remove_mac(&mut self, mac: Mac) -> Option<Originator> {
        let i: usize = self.list.iter().position(|o| o.mac() == mac)?;
        self.list.swap_remove(i)
    }

    pub fn select_router(&self, orig_mac: Mac,candidate_mac: Mac)->bool{
        let originator = match self.find_mac(orig_mac) {
            Some(entry) => entry,
            Option::None => return false,
        };
        
        let candidate_path = match originator.find_neighbor(candidate_mac) {
            Some(path) => path,
            Option::None => return false,
        };
        
        let router_mac = match originator.router_id() {
            Some(mac) => mac,
            Option::None => return true,
        };
        if router_mac == candidate_mac {
            return false;
        }
        let router_path = match originator.find_neighbor(router_mac) {
            Some(path) => path,
            Option::None => return true,
        };
       
        
 
        // Compare path quality, higher wins
        if candidate_path.tq_avg() == 0 && router_path.tq_avg() == 0 {
            return false;
        }
        if candidate_path.tq_avg() > router_path.tq_avg() {
            return true;
        }
        if candidate_path.tq_avg() < router_path.tq_avg() {
            return false;
        }

        self.echo_sum(candidate_mac) > self.echo_sum(router_mac)



    }

    fn echo_sum(&self, mac:Mac)->u8{
        let originator = match self.find_mac(mac) {
            Some(entry) => entry,
            Option::None => return 0,
        };
        originator.echo()
    }

    pub fn next_hop(&self, dest_mac: Mac) -> Option<Mac> {
        self.find_mac(dest_mac)?.router_id()
    }

    pub fn purge_all(&mut self){
        self.list.clear();
    }

    pub fn purge(&mut self, now: Instant) {
        let mut i: usize = 0;
        while i < self.list.len() {
            let timed_out: bool = self.list.get(i).is_some_and(|orig| orig.has_timed_out(now));

            if timed_out {
                // swap_remove moves the last entry into slot i, so don't
                // advance
                let _ = self.list.swap_remove(i);
            } else {
                if let Some(orig) = self.list.get_mut(i) {
                    orig.purge_timed_out_neighbors(now);
                }
                i += 1;
            }
        }
    }

    pub fn slide_echo_windows(&mut self) {
        for orig in self.list.iter_mut() {
            orig.slide_echo_window();
        }
    }

}
