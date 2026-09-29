use crate::arrayvec::ArrayVec;
use crate::consts::TQ_RING_SIZE;
use crate::mac::Mac;

pub struct Originator{
    orig: Mac,
    in_use:u8,
    last_seen:u32,
    router_id:u8,
    last_seqno:u32,
    last_ttl:u8,
    last_seqno_forwarded:u32,
    time_seqno_reset:u32,
    broadcast_own:u64,
    broadcast_own_sum: u8,
    broadcast_bits:u64,
    broadcast_seqno_reset:u32,
    last_bcast_seqno:u32,
    neigh:ArrayVec<Neighbor,8>,
    last_ttvn:u8,
}

pub struct Neighbor{
    addr:Mac,
    last_seen:u32,
    real_bits:u64,
    real_packet_count:u8,
    tq: TqRing,
    last_ttl: u8,
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


