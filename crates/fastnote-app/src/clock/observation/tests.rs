use std::cell::Cell;

use fastnote_core::{Clock, NaiveDate};

use super::*;

struct FakeClock {
    utc: Cell<DateTime<Utc>>,
    offset: Cell<i32>,
    now_calls: Cell<u32>,
    today_calls: Cell<u32>,
}

impl FakeClock {
    fn new(utc: &str, offset: i32) -> Self {
        Self {
            utc: Cell::new(utc.parse().unwrap()),
            offset: Cell::new(offset),
            now_calls: Cell::new(0),
            today_calls: Cell::new(0),
        }
    }
    fn advance(&self, seconds: i64) {
        self.utc
            .set(self.utc.get() + chrono::Duration::seconds(seconds));
    }
}

impl Clock for FakeClock {
    fn today(&self) -> NaiveDate {
        self.today_calls.set(self.today_calls.get() + 1);
        self.local_datetime(self.utc.get()).date()
    }
}

impl TaskClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now_calls.set(self.now_calls.get() + 1);
        self.utc.get()
    }
    fn local_datetime(&self, instant: DateTime<Utc>) -> NaiveDateTime {
        instant.naive_utc() + chrono::Duration::seconds(i64::from(self.offset.get()))
    }
}

#[test]
fn test_first_clock_observation_establishes_local_baseline_without_a_change() {
    let clock = FakeClock::new("2026-11-04T12:00:00Z", 8 * 3600);
    let mut tracker = ClockTracker::default();
    let check = tracker.observe(&clock);
    assert!(!check.changed);
    assert_eq!(check.sample.local, "2026-11-04T20:00:00".parse().unwrap());
    assert_eq!(tracker.previous, Some(check.sample));
}

#[test]
fn test_clock_sample_uses_one_utc_read_instead_of_a_second_independent_today_read() {
    let clock = FakeClock::new("2026-11-04T23:59:59Z", 0);
    let check = ClockTracker::default().observe(&clock);
    assert_eq!(clock.now_calls.get(), 1);
    assert_eq!(clock.today_calls.get(), 0);
    assert_eq!(check.sample.local, check.sample.utc.naive_utc());
}

#[test]
fn test_twenty_forward_ticks_only_replace_the_single_baseline() {
    let clock = FakeClock::new("2026-11-04T12:00:00Z", 0);
    let mut tracker = ClockTracker::default();
    tracker.observe(&clock);
    for _ in 0..20 {
        clock.advance(30);
        let check = tracker.observe(&clock);
        assert!(!check.changed);
        assert_eq!(tracker.previous, Some(check.sample));
    }
    assert_eq!(
        tracker.previous.unwrap().local,
        "2026-11-04T12:10:00".parse().unwrap()
    );
}

#[test]
fn test_midnight_without_any_page_snapshot_invalidates_once_and_keeps_new_date() {
    let clock = FakeClock::new("2026-11-04T15:59:59Z", 8 * 3600);
    let mut tracker = ClockTracker::default();
    tracker.observe(&clock);
    clock.advance(1);
    let check = tracker.observe(&clock);
    assert!(check.changed);
    assert_eq!(
        check.sample.local.date(),
        "2026-11-05".parse::<NaiveDate>().unwrap()
    );
    clock.advance(30);
    assert!(!tracker.observe(&clock).changed);
}

#[test]
fn test_clock_rollback_is_detected_even_when_it_stays_after_the_last_query_time() {
    let clock = FakeClock::new("2026-11-04T13:00:00Z", 0);
    let mut tracker = ClockTracker::default();
    tracker.observe(&clock);
    clock.utc.set("2026-11-04T12:30:00Z".parse().unwrap());
    assert!(tracker.observe(&clock).changed);
    clock.advance(30);
    assert!(!tracker.observe(&clock).changed);
}

#[test]
fn test_timezone_change_is_detected_even_if_local_time_moves_forward_on_the_same_day() {
    let clock = FakeClock::new("2026-11-04T12:00:00Z", 0);
    let mut tracker = ClockTracker::default();
    tracker.observe(&clock);
    clock.offset.set(3600);
    let check = tracker.observe(&clock);
    assert!(check.changed);
    assert_eq!(check.sample.local, "2026-11-04T13:00:00".parse().unwrap());
    clock.advance(30);
    assert!(!tracker.observe(&clock).changed);
}
