use crate::consts::{JITTER, REBORADCAST_INTER};

// tick rate
pub const TICK_HZ:u32 =1_000_000;

// Time types
pub type Instant = fugit::Instant<u64,1,TICK_HZ>;
pub type Duration = fugit::Duration<u64,1,TICK_HZ>;

// Takes u64 and returns duration of 'us' in microseconds 
pub const fn micros(us:u64)->Duration{
    Duration::from_ticks(us)
}

pub const fn millis(ms:u64)->Duration{
    Duration::from_ticks(ms.saturating_mul(1_000))
}

pub const fn secs(s: u64) -> Duration {
    Duration::from_ticks(s.saturating_mul(1_000_000))
}

pub const fn from_monotonic<const NOM: u32, const DENOM: u32>(
    t: fugit::Instant<u64, NOM, DENOM>,
) -> Instant {
    let ticks = t.ticks();
    let us = if NOM == 1 && TICK_HZ % DENOM == 0 {
        ticks.saturating_mul((TICK_HZ / DENOM) as u64)
    } else if NOM == 1 && DENOM % TICK_HZ == 0 {
        ticks / (DENOM / TICK_HZ) as u64
    } else {
        // At most 2^64 * 2^32 * 2^20, well inside u128.
        let wide = ticks as u128 * NOM as u128 * TICK_HZ as u128 / DENOM as u128;
        if wide > u64::MAX as u128 {
            u64::MAX
        } else {
            wide as u64
        }
    };
    Instant::from_ticks(us)
}

pub const fn add(t: Instant, d: Duration) -> Instant {
    Instant::from_ticks(t.ticks().saturating_add(d.ticks()))
}

pub const fn elapsed(since: Instant, now: Instant) -> Duration {
    Duration::from_ticks(now.ticks().saturating_sub(since.ticks()))
}


pub const fn is_before(a: Instant, b: Instant) -> bool {
    a.ticks() < b.ticks()
}

pub const fn is_after(a: Instant, b: Instant) -> bool {
    a.ticks() > b.ticks()
}

pub const fn has_timed_out(timestamp: Instant, timeout: Duration, now: Instant) -> bool {
    now.ticks() > timestamp.ticks().saturating_add(timeout.ticks())
}


pub const fn has_timed_out_or_never(
    timestamp: Option<Instant>,
    timeout: Duration,
    now: Instant,
) -> bool {
    match timestamp {
        None => true,
        Some(t) => has_timed_out(t, timeout, now),
    }
}
 

pub const fn earliest(a: Option<Instant>, b: Option<Instant>)-> Option<Instant>{
    match (a, b){
        (Some(x),Some(y))=>{
            if x.ticks()<=y.ticks(){
                Some(x)
            }else{
                Some(y)
            }
        }
        (Some(x),None)=> Some(x),
        (None, Some(y))=>Some(y),
        (None, None) => None,
    }

}

pub const fn random_below(rand :u32, span: Duration)->Duration{
    let span = span.ticks();
    if span == 0{
        return Duration::from_ticks(0);
    }
    Duration::from_ticks(rand as u64 % span)
}

pub const fn ogm_send_time(base: Instant, interval:Duration, rand:u32)->Instant{
    let fixed = Duration::from_ticks(
        interval.ticks().saturating_sub(JITTER.ticks())
    );
    let spread = random_below(
        rand,
         Duration::from_ticks(JITTER.ticks().saturating_mul(2))
    );
    add(add(base,fixed),spread)
}

pub const fn fwd_omg_send_time(now:Instant, rand:u32)->Instant{
    add(now, random_below(rand, Duration::from_ticks(JITTER.ticks() / 2)))
}

pub const fn bcast_repeat_time(now: Instant)->Instant{
    add(now, REBORADCAST_INTER)
}