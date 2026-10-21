use crate::consts::{JITTER, REBORADCAST_INTER};

// tick rate
pub const TICK_HZ:u32 =1_000_000;

// Time types
pub type Instant = fugit::Instant<u64,1,TICK_HZ>;
pub type Duration = fugit::Duration<u64,1,TICK_HZ>;

pub const INSTANT_ZERO: Instant = Instant::from_ticks(0);

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

pub const fn has_reached(a: Instant, b: Instant) -> bool {
    a.ticks() >= b.ticks()
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

#[cfg(test)]
mod test{
    use super::*;

    const NOW: Instant = Instant::from_ticks(0);

    fn at(us:u64)->Instant{
        Instant::from_ticks(us)
    }

    #[test]
    fn micros_test(){
        assert_eq!(micros(0).ticks(),0);
        assert_eq!(micros(1).ticks(),1);
        assert_eq!(micros(2_136).ticks(),2_136);
        assert_eq!(micros(u64::MAX).ticks(),u64::MAX);
        
    }
    #[test]
    fn millis_test(){
        assert_eq!(millis(0).ticks(),0);
        assert_eq!(millis(1).ticks(),1_000);
        assert_eq!(millis(2_136).ticks(),2_136_000);
        let max = u64::MAX;
        let mil_max = max/1_000;
        assert_eq!(millis(mil_max).ticks(),max);
        assert_eq!(millis(mil_max + 1).ticks(), max);
        assert_eq!(millis(max).ticks(), max);
    }
    #[test]
    fn secs_test(){
        assert_eq!(secs(0).ticks(),0);
        assert_eq!(secs(1).ticks(),1_000_000);

        let max = u64::MAX;
        let sec_max = max/1_000_000;
        assert_eq!(secs(sec_max).ticks(),max);
        assert_eq!(secs(sec_max + 1).ticks(), max);
        assert_eq!(secs(max).ticks(), max);
    }
    #[test]
    fn from_monotonic_test(){
        // zero
        let zero = fugit::Instant::<u64, 1, 1_000_000>::from_ticks(0);
        assert_eq!(from_monotonic(zero), INSTANT_ZERO);

        // micro secs
        let us = fugit::Instant::<u64,1,1_000_000>::from_ticks(20);
        assert_eq!(from_monotonic(us).ticks(), 20);

        // milli secs
        let ms = fugit::Instant::<u64, 1, 1_000>::from_ticks(5);
        assert_eq!(from_monotonic(ms).ticks(), 5_000);

        // secs
        let s = fugit::Instant::<u64, 1, 1>::from_ticks(2);
        assert_eq!(from_monotonic(s).ticks(), 2_000_000);

        // nano secs (should truncate)
        let ns = fugit::Instant::<u64, 1, 1_000_000_000>::from_ticks(1_999);
        assert_eq!(from_monotonic(ns).ticks(), 1);
        let ns = fugit::Instant::<u64, 1, 1_000_000_000>::from_ticks(999);
        assert_eq!(from_monotonic(ns).ticks(), 0);


        // 32.768 kHz RTC (general branch)
        let rtc = fugit::Instant::<u64, 1, 32_768>::from_ticks(32_768);
        assert_eq!(from_monotonic(rtc).ticks(), 1_000_000);
        // 1 tick = 30.52 us -> floor
        let rtc = fugit::Instant::<u64, 1, 32_768>::from_ticks(1);
        assert_eq!(from_monotonic(rtc).ticks(), 30);

        // NOM != 1: 10 ms per tick (general branch)
        let coarse = fugit::Instant::<u64, 10, 1_000>::from_ticks(3);
        assert_eq!(from_monotonic(coarse).ticks(), 30_000);

        // 3 Hz, no saturation (general branch)
        let odd = fugit::Instant::<u64, 1, 3>::from_ticks(3);
        assert_eq!(from_monotonic(odd).ticks(), 1_000_000);

        // saturation, multiply branch
        let ms = fugit::Instant::<u64, 1, 1_000>::from_ticks(u64::MAX);
        assert_eq!(from_monotonic(ms).ticks(), u64::MAX);

        // saturation, general branch
        let odd = fugit::Instant::<u64, 1, 3>::from_ticks(u64::MAX);
        assert_eq!(from_monotonic(odd).ticks(), u64::MAX);
    
    }
    #[test]
    fn add_test_test(){
        assert_eq!(add(at(100), micros(50)), at(150));
        assert_eq!(add(at(100), micros(0)), at(100));
        assert_eq!(add(INSTANT_ZERO, secs(1)), at(1_000_000));

        // saturates instead of wrapping / panicking
        assert_eq!(add(at(u64::MAX - 1), micros(5)), at(u64::MAX));
        assert_eq!(add(at(u64::MAX), micros(u64::MAX)), at(u64::MAX));
    }
    #[test]
    fn elapsed_test(){
        assert_eq!(elapsed(at(100), at(150)), micros(50));
        assert_eq!(elapsed(at(100), at(100)), micros(0));
        assert_eq!(elapsed(INSTANT_ZERO, at(u64::MAX)), micros(u64::MAX));

    }
    #[test]
    fn is_before_test(){
        let a = at(10);
        let b = at(20);
        assert!(is_before(a, b));
        assert!(!is_before(b, a));
        assert!(!is_before(a, a));

    }
    #[test]
    fn is_after_test(){
        let a = at(10);
        let b = at(20);
        assert!(is_after(b, a));
        assert!(!is_after(a, b));
        assert!(!is_after(a, a));

    }
    #[test]
    fn has_reached_test(){
        let a = at(10);
        let b = at(20);
        let c = at(10);
        assert!(has_reached(b, a));
        assert!(!has_reached(a, b));
        assert!(has_reached(a, c));
        
    }
    #[test]
    fn has_timed_out_test(){
        let ts = at(1_000);
        let to = millis(1);

        assert!(!has_timed_out(ts, to, at(1_000)));

        // exactly at deadline: not timed out (strict >)
        assert!(!has_timed_out(ts, to, at(2_000)));

        // just past deadline
        assert!(has_timed_out(ts, to, at(2_001)));

        // now before timestamp
        assert!(!has_timed_out(ts, to, INSTANT_ZERO));

        // zero timeout: times out one tick later
        assert!(!has_timed_out(ts, micros(0), ts));
        assert!(has_timed_out(ts, micros(0), at(1_001)));

        // deadline saturates at u64::MAX: never times out, no overflow
        assert!(!has_timed_out(at(u64::MAX - 10), micros(100), at(u64::MAX)));

    }

    #[test]
    fn has_timed_out_or_never_test(){
        // never seen: always timed out, regardless of timeout / now
        assert!(has_timed_out_or_never(None, secs(1), INSTANT_ZERO));
        assert!(has_timed_out_or_never(None, micros(u64::MAX), INSTANT_ZERO));

        // Some: same rules as has_timed_out
        let ts = at(1_000);
        assert!(!has_timed_out_or_never(Some(ts), millis(1), at(2_000)));
        assert!(has_timed_out_or_never(Some(ts), millis(1), at(2_001)));

    }
    #[test]
    fn earliest_test(){
        let a = at(10);
        let b = at(20);

        // order of args irrelevant
        assert_eq!(earliest(Some(a), Some(b)), Some(a));
        assert_eq!(earliest(Some(b), Some(a)), Some(a));

        // equal
        assert_eq!(earliest(Some(a), Some(a)), Some(a));

        // one side None: other wins
        assert_eq!(earliest(Some(a), None), Some(a));
        assert_eq!(earliest(None, Some(b)), Some(b));
        assert_eq!(earliest(None, None), None);
    
    }
    #[test]
    fn random_below_test(){
        let span = micros(100);

        assert_eq!(random_below(0, span), micros(0));
        assert_eq!(random_below(42, span), micros(42));
        assert_eq!(random_below(99, span), micros(99));

        // wraps at span
        assert_eq!(random_below(100, span), micros(0));
        assert_eq!(random_below(250, span), micros(50));

        // always strictly below span
        for &rand in [0u32, 1, 99, 100, 101, 12_345, u32::MAX].iter() {
            assert!(random_below(rand, span).ticks() < span.ticks());
        }

        // span 0: 0, no div by zero
        assert_eq!(random_below(u32::MAX, micros(0)), micros(0));

        // span 1: always 0
        assert_eq!(random_below(u32::MAX, micros(1)), micros(0));

        // span wider than u32: rand returned as-is
        assert_eq!(random_below(u32::MAX, micros(u64::MAX)), micros(u32::MAX as u64));
    
    }
    
    #[test]
    fn ogm_send_time_test(){
        let j = JITTER.ticks();
        assert!(j > 0, "test assumes JITTER > 0");

        let base = at(1_000_000);
        let interval = secs(1);
        assert!(interval.ticks() >= j, "test assumes JITTER <= 1 s");

        let first = base.ticks() + interval.ticks() - j;
        let last = base.ticks() + interval.ticks() + j - 1;

        // rand 0: earliest, interval - JITTER
        assert_eq!(ogm_send_time(base, interval, 0).ticks(), first);

        // rand J: exactly base + interval
        assert_eq!(ogm_send_time(base, interval, j as u32).ticks(), base.ticks() + interval.ticks());

        // rand 2J - 1: latest
        assert_eq!(ogm_send_time(base, interval, (2 * j - 1) as u32).ticks(), last);

        // rand 2J: wraps back to earliest
        assert_eq!(ogm_send_time(base, interval, (2 * j) as u32).ticks(), first);

        // any rand: within [interval - J, interval + J)
        for &rand in [1u32, 7, 12_345, u32::MAX].iter() {
            let t = ogm_send_time(base, interval, rand).ticks();
            assert!(t >= first && t <= last);
        }

        // interval shorter than JITTER: fixed part clamps to 0, never before base
        assert_eq!(ogm_send_time(base, micros(0), 0), base);
        assert_eq!(
            ogm_send_time(base, micros(0), (2 * j - 1) as u32).ticks(),
            base.ticks() + 2 * j - 1
        );

        // saturates near end of time
        assert_eq!(ogm_send_time(at(u64::MAX - 1), interval, 0), at(u64::MAX));

    }

    #[test]
    fn fwd_omg_send_time_test(){
        let j = JITTER.ticks();
        assert!(j >= 2, "test assumes JITTER >= 2 us");
        let half = j / 2;
        let now = at(5_000);

        // rand 0: immediate
        assert_eq!(fwd_omg_send_time(now, 0), now);

        // max delay: JITTER/2 - 1
        assert_eq!(fwd_omg_send_time(now, (half - 1) as u32).ticks(), now.ticks() + half - 1);

        // wraps at JITTER/2
        assert_eq!(fwd_omg_send_time(now, half as u32), now);

        // any rand: within [now, now + JITTER/2)
        for &rand in [1u32, 7, 12_345, u32::MAX].iter() {
            let t = fwd_omg_send_time(now, rand).ticks();
            assert!(t >= now.ticks() && t < now.ticks() + half);
        }

        // saturates near end of time
        assert_eq!(fwd_omg_send_time(at(u64::MAX), u32::MAX), at(u64::MAX));

    }

    #[test]
    fn bcast_repeat_time_test(){
        let now = at(5_000);
        assert_eq!(bcast_repeat_time(now).ticks(), 5_000 + REBORADCAST_INTER.ticks());
        assert_eq!(bcast_repeat_time(INSTANT_ZERO).ticks(), REBORADCAST_INTER.ticks());

        // saturates near end of time
        assert_eq!(bcast_repeat_time(at(u64::MAX)), at(u64::MAX));

    }
}