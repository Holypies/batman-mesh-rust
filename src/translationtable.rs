use crate::ArrayVec::ArrayVec;

pub enum TTFlags{
    New,
    NoPurge,
    Delete,
}


pub struct VlanCrc{
    vlan_id: u16,
    crc:u32,
}

impl VlanCrc{
    pub const fn new(vlan_id:u16, crc:u32)->Self{
        Self{
            vlan_id,
            crc,
        }
    }

    // --- Getters ---
    pub fn vlan_id(&self)->u16{
        self.vlan_id
    }
    pub fn crc(&self)->u32{
        self.crc
    }

    // --- Setters ---
    pub fn set_vlan_id(&mut self, vlan_id:u16){
        self.vlan_id=vlan_id
    }
    pub fn set_crc(&mut self, crc:u32){
        self.crc=crc
    }
}

pub struct Change{
    addr:Mac,
    vlan_id:u16,
    flags:u8,
}

impl Change{
    pub const fn new(addr:Mac, vlan_id:u16, flags:u8)->Self{
        Self{
            addr,
            vlan_id,
            flags,
        }
    }
}


pub struct TranslationTable{
    local_table: LocalTable,
    global_table: GlobalTable,
}

impl TranslationTable{
    pub const fn new()->Self{
        Self{
            local_table: LocalTable::new(),
            global_table: GlobalTable::new(),
        }
    }
}

pub struct LocalTable{
    entries: ArrayVec<LocalEnty, TT_MAX_LOCAL_ENT>,
    unannounced: ArrayVec<Change,TT_MAX_CHANGES>,
    commited: ArrayVec<Change,TT_MAX_CHANGES>,
    repeats_left:u8,
    ttvn:u8,
    crcs:ArrayVec<VlanCrc,>
}

impl LocalTable{
    pub const fn new()->Self{
        Self{
            entries:ArrayVec<LocalEnty, TT_MAX_LOCAL_ENT>::new(),
            unannounced: ArrayVec<Change,TT_MAX_CHANGES>::new(),
            commited: ArrayVec<Change,TT_MAX_CHANGES>::new(),
            repeats_left:0,
            ttvn:0,
            crcs:ArrayVec<VlanCrc,V>::new(),
        }
    }

    // --- functions ---
    pub fn add(&mut self, addr:Mac, vlan_id:u16, flags:u8)->Result<(), TableError>{
        
        for local_entry in self.entries.iter() {

            if local_entry.mac() == addr && local_entry.vlan_id() == vlan_id {
                if local_entry.flags() == TT_FLAG_PENDING{
                    // if flagged for deletetion, remove deletetion flag
                    // TODO: fix removal from unannounced
                    local_entry.set_flag(local_entry.flags()^TT_FLAG_PENDING);
                    for change in self.unannounced.iter(){
                        if change.addr() == addr && change.vlan_id()== vlan_id{
                            
                        }
                    }

                }else{
                    return Ok();
                }
            }
        }
        
        if self.entries.is_full(){
            return Err(TableError::TableFull);
        }
        if self.unannounced.is_full(){
            return Err(TableError::UnannouncedFull);
        }

        let new_local = LocalEnty::new(addr, vlan_id, (flags|TT_FLAG_NEW));

        self.entries.push(new_local)

    }

    pub fn remove(&mut self, addr:Mac, vlan_id:u16){
        
        let Some(i) = self.entries.iter().position(
            |e| e.mac == addr && c.vlan_id == vlan_id
        ) else {
            return;
        };
        
        if self.entries.get(i).flags() == TT_FLAG_NEW{
            self.entries.swap_remove(i);
            cancel_change(addr, vlan_id);
        }

        let new_change = Change::new(addr, vlan_id, TT_FLAG_DELETE);
        self.unannounced.push(new_change);
    }

    
    pub fn is_my_client(addr:Mac, vlan_id:u16)->bool{
        for entry in self.entries.iter() {
            if entry.mac == addr && entry.flags != TT_FLAG_PENDING {
                return true;
            }
        }
        return false;
    }

    fn cancel_change(&mut self, addr: Mac, vlan_id: u16) -> bool {
        let Some(i) = self.pending.iter().position(
            |c| c.addr == addr && c.vlan_id == vlan_id) else {
            return false;
        };
        self.pending.swap_remove(i).is_some()
    }



}

pub struct LocalEnty{
    mac:Mac,
    vlan_id:u8,
    flags:u8,
    last_seen:Instant,
}


pub struct GlobalTable{
    entries: ArrayVec<GlobalEntry,TT_MAX_GLOBAL_ENT>,
}

pub struct GlobalEntry{
    mac: Mac,
    vlan_id:u8,
    flag:u8,
    origs: ArrayVec<Mac, O>,
}