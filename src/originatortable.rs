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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::originator::Neighbor;
    use crate::consts::{ORIG_TIME_OUT, WINDOW_SIZE};
    use crate::time::micros;

    const NOW: Instant = Instant::from_ticks(0);

    // --- test helpers ---

    // mac creater helper
    fn mac(n: u8) -> Mac {
        Mac::from_octets([0x02, 0x00, 0x00, 0x00, 0x00, n])
    }

    // originator `dest` with neighbors (addr, tq) and optional router, public API only
    fn orig_with(dest: Mac, neighs: &[(Mac, u8)], router: Option<Mac>) -> Originator {
        let mut orig = Originator::new(dest, NOW);
        for &(addr, tq) in neighs {
            let mut n = Neighbor::new(addr, NOW);
            n.record_transmit_quality(tq);
            orig.add_neighbor(n, NOW).unwrap();
        }
        if let Some(r) = router {
            orig.set_router(r);
        }
        orig
    }

    // originator with `marks` distinct echo slots set
    fn echo_orig(addr: Mac, marks: usize) -> Originator {
        let mut orig = Originator::new(addr, NOW);
        for i in 0..marks {
            orig.record_echo(i as i32);
        }
        orig
    }

    // table filled by direct push, bypasses add()
    fn table_with(origs: &[Originator]) -> OriginatorTable {
        let mut table = OriginatorTable::new();
        for o in origs {
            table.list.push(*o).unwrap();
        }
        table
    }

    #[test]
    fn orig_table_new_test(){
        let table = OriginatorTable::new();
        assert_eq!(table.list.len(), 0);
        assert!(table.find_mac(mac(1)).is_none());

    }

    #[test]
    fn orig_table_find_mac_test(){
        let empty = OriginatorTable::new();
        assert!(empty.find_mac(mac(1)).is_none());

        let t1 = Instant::from_ticks(1_000);
        let table = table_with(
            &[
                Originator::new(mac(1), NOW),
                Originator::new(mac(2), NOW),
                Originator::new(mac(2), t1), // duplicate, only reachable via direct push
            ]
        );

        assert_eq!(table.find_mac(mac(1)).unwrap().mac(), mac(1));
        assert_eq!(table.find_mac(mac(2)).unwrap().mac(), mac(2));
        assert!(table.find_mac(mac(9)).is_none());

        // duplicate: first wins (should never be able to happen)
        assert_eq!(table.find_mac(mac(2)).unwrap().last_seen(), NOW);
    }

    #[test]
    fn orig_table_find_mac_mut_test(){
        let mut empty = OriginatorTable::new();
        assert!(empty.find_mac_mut(mac(1)).is_none());

        let mut table = table_with(
            &[
                Originator::new(mac(1), NOW), 
                Originator::new(mac(2), NOW)
            ]
        );
        assert!(table.find_mac_mut(mac(9)).is_none());

        // mutation through returned ref persists
        let t = Instant::from_ticks(5_000);
        let orig = table.find_mac_mut(mac(2)).unwrap();
        assert_eq!(orig.mac(), mac(2));

        orig.touch(t);
        assert_eq!(table.list.get(1).unwrap().last_seen(), t);

        // other entry untouched
        assert_eq!(table.list.get(0).unwrap().last_seen(), NOW);
    }

    #[test]
    fn orig_table_add_test(){
        let t1 = Instant::from_ticks(1_000);
        let t2 = Instant::from_ticks(2_000);
        let mut table = OriginatorTable::new();

        // new: Ok, stored, stamped with `now`
        assert_eq!(table.add(Originator::new(mac(1), NOW), t1), Ok(()));
        assert_eq!(table.list.len(), 1);
        assert_eq!(table.find_mac(mac(1)).unwrap().last_seen(), t1);

        // duplicate: AlreadyInTable, existing entry not touched
        assert_eq!(
            table.add(Originator::new(mac(1), NOW), t2),
            Err(OriginatorTableError::AlreadyInTable)
        );
        assert_eq!(table.list.len(), 1);
        assert_eq!(table.find_mac(mac(1)).unwrap().last_seen(), t1);

        // second distinct: appended
        assert_eq!(table.add(Originator::new(mac(2), NOW), t2), Ok(()));
        assert_eq!(table.list.len(), 2);
        assert_eq!(table.find_mac(mac(2)).unwrap().last_seen(), t2);

        // fill to capacity
        let mut full = OriginatorTable::new();
        for i in 0..MAX_NEIGHBORS {
            assert_eq!(full.add(Originator::new(mac(i as u8 + 1), NOW), NOW), Ok(()));
        }
        assert_eq!(full.list.len(), MAX_NEIGHBORS);

        // one more distinct: TableFull, table unchanged
        let extra = mac(MAX_NEIGHBORS as u8 + 1);
        assert_eq!(
            full.add(Originator::new(extra, NOW), NOW),
            Err(OriginatorTableError::TableFull)
        );
        assert_eq!(full.list.len(), MAX_NEIGHBORS);
        assert!(full.find_mac(extra).is_none());
    }

    #[test]
    fn orig_table_add_existing_when_full_test(){
        let mut table = OriginatorTable::new();
        for i in 0..MAX_NEIGHBORS {
            table.list.push(Originator::new(mac(i as u8 + 1), NOW)).unwrap();
        }
        assert_eq!(
            table.add(Originator::new(mac(1), NOW), NOW),
            Err(OriginatorTableError::AlreadyInTable)
        );
    }

    #[test]
    fn orig_table_remove_mac_test(){
        let mut empty = OriginatorTable::new();
        assert_eq!(empty.remove_mac(mac(1)), None);

        let orig1 = Originator::new(mac(1), NOW);
        let orig2 = Originator::new(mac(2), NOW);
        let orig3 = Originator::new(mac(3), NOW);
        let mut table = table_with(&[orig1, orig2, orig3]);

        // middle entry: returns exact originator, rest kept
        assert_eq!(table.remove_mac(mac(2)), Some(orig2));
        assert_eq!(table.list.len(), 2);
        assert!(table.find_mac(mac(2)).is_none());

        assert!(table.find_mac(mac(1)).is_some());
        assert!(table.find_mac(mac(3)).is_some());

        // already removed / unknown: None, table unchanged
        assert_eq!(table.remove_mac(mac(2)), None);
        assert_eq!(table.remove_mac(mac(9)), None);
        assert_eq!(table.list.len(), 2);

        // drain the rest
        assert_eq!(table.remove_mac(mac(1)), Some(orig1));
        assert_eq!(table.remove_mac(mac(3)), Some(orig3));
        assert_eq!(table.list.len(), 0);
    }

    #[test]
    fn orig_table_select_router_test(){
        let dst = mac(10);
        let a = mac(1);
        let b = mac(2);

        // unknown originator: return false
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 100)], None)
            ]
        );
        assert!(!table.select_router(mac(9), a));

        // candidate not a neighbor: return false
        assert!(!table.select_router(dst, mac(9)));

        // TODO: double check if this should be true
        // no router yet and 0 tq on neighbor: return false
        let table = table_with(
            &[
                orig_with(dst, &[(a, 0)], None)
            ]
        );
        assert!(!table.select_router(dst, a));
        
        // candidate already router: return false
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 200)], Some(b))
            ]
        );
        assert!(!table.select_router(dst, b));

        // TODO: Double check that this is the intended functionality
        // router not in neighbor list: candidate accepted
        let table = table_with(
            &[
                orig_with(dst, &[(a, 10)], Some(mac(8)))
            ]
        );
        assert!(table.select_router(dst, a));

        // both tq 0
        let table = table_with(
            &[
                orig_with(dst, &[(a, 0), (b, 0)], Some(a))
            ]
        );
        assert!(!table.select_router(dst, b));

        // candidate better tq
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 200)], Some(a))
            ]
        );
        assert!(table.select_router(dst, b));

        // TODO: add router id to mac
        // read-only: router not changed by select
        assert_eq!(table.find_mac(dst).unwrap().router_id(), Some(a));

        // candidate worse tq
        let table = table_with(
            &[
                orig_with(dst, &[(a, 200), (b, 100)], Some(a))
            ]
        );
        assert!(!table.select_router(dst, b));

        // tq tie, neither in table as originator: echo 0 vs 0
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 100)], Some(a))
            ]
        );
        assert!(!table.select_router(dst, b));

        // tq tie, candidate higher echo
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 100)], Some(a)),
                echo_orig(b, 2),
            ]
        );
        assert!(table.select_router(dst, b));

        // tq tie, router higher echo
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 100)], Some(a)),
                echo_orig(a, 3),
                echo_orig(b, 2),
            ]
        );
        assert!(!table.select_router(dst, b));

        // tq tie, equal echo
        let table = table_with(
            &[
                orig_with(dst, &[(a, 100), (b, 100)], Some(a)),
                echo_orig(a, 2),
                echo_orig(b, 2),
            ]
        );
        assert!(!table.select_router(dst, b));

    }

    #[test]
    fn orig_table_echo_sum_test(){
        let table = table_with(
            &[
                echo_orig(mac(1), 0), 
                echo_orig(mac(2), 3)
            ]
        );
        assert_eq!(table.echo_sum(mac(1)), 0);
        assert_eq!(table.echo_sum(mac(2)), 3);

        // unknown originator: 0
        assert_eq!(table.echo_sum(mac(9)), 0);
    }

    #[test]
    fn orig_table_next_hop_test(){
        let table = table_with(
            &[
                orig_with(mac(10), &[(mac(1), 100)], None),
                orig_with(mac(11), &[(mac(1), 100), (mac(2), 50)], Some(mac(1))),
            ]
        );

        // unknown destination
        assert_eq!(table.next_hop(mac(9)), None);
        // known, no router
        assert_eq!(table.next_hop(mac(10)), None);
        // known, router set
        assert_eq!(table.next_hop(mac(11)), Some(mac(1)));
    }

    #[test]
    fn orig_table_purge_all_test(){
        let mut table = table_with(
            &[
                Originator::new(mac(1), NOW), Originator::new(mac(2), NOW)
            ]
        );
        table.purge_all();
        assert_eq!(table.list.len(), 0);
        assert!(table.find_mac(mac(1)).is_none());
        assert!(table.find_mac(mac(2)).is_none());

        // usable after clear
        assert_eq!(table.add(Originator::new(mac(1), NOW), NOW), Ok(()));
        assert_eq!(table.list.len(), 1);

        // empty table: no panic
        let mut empty = OriginatorTable::new();
        empty.purge_all();
        assert_eq!(empty.list.len(), 0);
    }

    #[test]
    fn orig_table_purge_test(){
        let boundary = NOW + ORIG_TIME_OUT;
        let past = boundary + micros(1);

        // exactly at boundary: nothing removed
        let mut table = table_with(
            &[
                Originator::new(mac(1), NOW)
            ]);
        table.purge(boundary);
        assert_eq!(table.list.len(), 1);

        // stale removed, fresh kept
        let mut table = table_with(
            &[
                Originator::new(mac(1), NOW), Originator::new(mac(2), past)
            ]
        );
        table.purge(past);
        assert_eq!(table.list.len(), 1);
        assert!(table.find_mac(mac(1)).is_none());
        assert!(table.find_mac(mac(2)).is_some());

        // consecutive + trailing stale (swap_remove path): only fresh survive
        let mut table = table_with(&[
            Originator::new(mac(1), NOW),
            Originator::new(mac(2), NOW),
            Originator::new(mac(3), past),
            Originator::new(mac(4), NOW),
        ]);
        table.purge(past);
        assert_eq!(table.list.len(), 1);
        assert!(table.find_mac(mac(3)).is_some());

        assert!(table.find_mac(mac(1)).is_none());
        assert!(table.find_mac(mac(2)).is_none());
        assert!(table.find_mac(mac(4)).is_none());


        // surviving originator: stale neighbors purged, router reassigned
        let mut orig = Originator::new(mac(10), past);
        orig.add_neighbor(Neighbor::new(mac(1), NOW), NOW).unwrap();
        let mut good = Neighbor::new(mac(2), past);
        good.record_transmit_quality(150);
        orig.add_neighbor(good, past).unwrap();
        orig.set_router(mac(1));

        let mut table = table_with(&[orig]);
        table.purge(past);
        let orig = table.find_mac(mac(10)).unwrap();
        assert_eq!(orig.neighbors().len(), 1);
        assert!(orig.find_neighbor(mac(1)).is_none());
        assert_eq!(orig.router_id(), Some(mac(2)));

        // empty table: no panic
        let mut empty = OriginatorTable::new();
        empty.purge(past);
        assert_eq!(empty.list.len(), 0);
    }

    #[test]
    fn orig_table_slide_echo_windows_test(){
        let mut table = table_with(
            &[
                echo_orig(mac(1), 1), echo_orig(mac(2), 1)
            ]
        );

        // every originator slides: bits shifted, not dropped
        table.slide_echo_windows();
        assert_eq!(table.find_mac(mac(1)).unwrap().echo(), 1);
        assert_eq!(table.find_mac(mac(2)).unwrap().echo(), 1);

        // slot 0 freed in every originator
        for &m in [mac(1), mac(2)].iter() {
            let orig = table.find_mac_mut(m).unwrap();
            orig.record_echo(0);
            assert_eq!(orig.echo(), 2);
        }

        // full window of slides: all cleared
        for _ in 0..WINDOW_SIZE {
            table.slide_echo_windows();
        }
        assert_eq!(table.find_mac(mac(1)).unwrap().echo(), 0);
        assert_eq!(table.find_mac(mac(2)).unwrap().echo(), 0);

        // empty table: no panic
        let mut empty = OriginatorTable::new();
        empty.slide_echo_windows();

    }
    
}