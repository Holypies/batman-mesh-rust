
pub struct OriginatorTable{
    list:ArrayVec<Originator,N>

}

impl OriginatorTable{
    pub const fn new()->Self{
        Self{
            list:ArrayVec::new(MAX_ORIG_TABLE_SIZE),
        }
    }
    pub fn find_mac(&self, mac: Mac) -> Option<&Originator> {
        for originator in self.list.iter() {
            if originator.mac == mac {
                return Some(originator);
            }
        }
        None
    }
     pub fn find_mac_mut(&mut self, mac: Mac) -> Option<&mut Originator> {
        for originator in self.list.iter_mut() {
            if originator.mac == mac {
                return Some(originator);
            }
        }
        None
    }
    pub fn add(&mut self, orig: Originator, now: Instant){
        if self.list.id_full(){
            return;
        }
        let new_mac = orig.mac();
        for slot in &self.list[..self.list.len()]{
            if let Some(item) = slot {
                if item.mac == new_mac{
                    return;
                }
            }
        }
        orig.touch(now);
        self.list.push(orig);
    }
    pub fn remove(&mut self, orig:Originator){
        let index = self.list.position();
    }

    pub fn select_router(&self, orig_mac: Mac,candidate_mac: Mac)->bool{
        let originator = match self.find_mac(orig) {
            Some(entry) => entry,
            None => return false,
        };
        
        let candidate_path = match originator.neighbor(candidate) {
            Some(path) => path,
            None => return false,
        };
        
        let router_mac = match originator.router {
            Some(mac) => mac,
            None => return true,
        };
        if router_mac == candidate {
            return false;
        }
        let router_path = match originator.neighbor(router_mac) {
            Some(path) => path,
            None => return true,
        };
       
        
 
        // Compare path quality, higher wins
        if candidate_path.tq_avg > router_path.tq_avg {
            return true;
        }
        if candidate_path.tq_avg < router_path.tq_avg {
            return false;
        }
 
        // Equal quality: prefer the link that echoes more of our OGMs.
        // The router keeps ties so equal neighbors don't take turns.
        self.echo_sum(candidate) > self.echo_sum(router_mac)

    }
    pub fn next_hop(&self, dest_mac:Mac)-> Option<Mac>{
        let dest_orig = self.find_mac(dest);
        dest_orig.router_id()

    }
    pub fn purge_all(&mut self){

    }

}