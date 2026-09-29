use crate::mac::Mac;
use crate::arrayvec::ArrayVec;
use crate::originator::Originator;
pub struct MeshCtx{
    orig_addr: Mac,
    software_mac:Mac,
  //  org_table: ArrayVec<Originator, N>,
    trans_table: TranslationTable,
    ttvn:u8,
    mesh_active: bool,
    bcast_seqno: u32,
}

impl MeshCtx {
    pub const fn new()->Self{
        let zero_octets:[u8;6] = [0u8;6];
        Self{
            orig_addr: Mac::from_octets(zero_octets),
            software_mac:Mac::from_octets(zero_octets),
          //  org_table: ArrayVec<Originator,8>::new(),
            trans_table: TranslationTable::new(),
            ttvn:0,
            mesh_active: false,
            bcast_seqno: 0,
        }
    }
    pub fn init(&mut self, orig_hardware_addr: [u8;6]){
        self.orig_addr = Mac::from_octets(orig_hardware_addr);
        //self.software_mac = create_software_addr(orig_hardware_addr);
        //self.org_table

    } 

    pub fn activate(&mut self, time:u32){
        self.mesh_active = true;

    }

    pub fn deactivate(&mut self){
        self.mesh_active = false;

    }
}

pub struct TranslationTable{
    placeholder: u8,
}

impl TranslationTable {
    pub const fn new()->Self{
        Self{
            placeholder:0,
        }
    }
}