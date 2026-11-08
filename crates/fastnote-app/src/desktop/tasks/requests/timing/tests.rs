use fastnote_core::{Clock, DateTime, NaiveDate, NaiveDateTime, TodayView, Utc};

use crate::desktop::tasks::policy::{may_query, needs_page_load};

use super::*;

struct FakeClock {
    utc: Cell<DateTime<Utc>>,
    offset: Cell<i32>,
}

impl FakeClock {
    fn new(utc: &str) -> Self {
        Self {
            utc: Cell::new(utc.parse().unwrap()),
            offset: Cell::new(0),
        }
    }
    fn advance(&self, seconds: i64) {
        self.utc
            .set(self.utc.get() + chrono::Duration::seconds(seconds));
    }
}

impl Clock for FakeClock {
    fn today(&self) -> NaiveDate {
        self.local_datetime(self.utc.get()).date()
    }
}

impl TaskClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.utc.get()
    }
    fn local_datetime(&self, instant: DateTime<Utc>) -> NaiveDateTime {
        instant.naive_utc() + chrono::Duration::seconds(i64::from(self.offset.get()))
    }
}

fn view(at: &str, deadline: Option<&str>) -> TodayView {
    let at: NaiveDateTime = at.parse().unwrap();
    TodayView {
        date: at.date(),
        evaluated_at: at,
        next_overdue_at: deadline.map(|at| at.parse().unwrap()),
        sections: Vec::new(),
        open_count: 0,
        overdue_count: 0,
        completed_today_count: 0,
    }
}

#[test]
fn test_twenty_hidden_ticks_across_deadline_and_midnight_only_invalidate_with_zero_queries() {
    for has_snapshot in [false, true] {
        let clock = FakeClock::new("2026-11-04T23:55:00Z");
        let mut tracker = ClockTracker::default();
        let mut tasks = TaskSession::new();
        if has_snapshot {
            tasks.today = Some(view("2026-11-04T23:55:00", Some("2026-11-04T23:56:00")));
        }
        tasks.collapsed[1] = Some(true);
        tasks.limits[1] = 800;
        let today_dirty = Cell::new(!has_snapshot);
        let detail_dirty = Cell::new(false);
        observe_invalidation(
            &mut tracker,
            &clock,
            &mut tasks,
            &today_dirty,
            &detail_dirty,
        );
        let mut queries = 0;
        let mut projections = 0;
        for _ in 0..20 {
            clock.advance(30);
            let check = observe_invalidation(
                &mut tracker,
                &clock,
                &mut tasks,
                &today_dirty,
                &detail_dirty,
            );
            let visible = false;
            queries += usize::from(check.changed && may_query(None, visible));
            projections += usize::from(check.changed && visible);
        }
        assert_eq!(queries, 0);
        assert_eq!(projections, 0);
        assert!(today_dirty.get());
        assert!(detail_dirty.get());
        assert_eq!(tasks.today.is_some(), has_snapshot);
        assert_eq!(tasks.collapsed[1], Some(true));
        assert_eq!(tasks.limits[1], 800);
        let check = observe_invalidation(
            &mut tracker,
            &clock,
            &mut tasks,
            &today_dirty,
            &detail_dirty,
        );
        assert_eq!(
            check.sample.local.date(),
            "2026-11-05".parse::<NaiveDate>().unwrap()
        );
        assert!(needs_page_load(
            today_dirty.get(),
            tasks.today.is_some(),
            tasks.latest_today
        ));
    }
}

#[test]
fn test_visible_deadline_refresh_triggers_once_after_the_exact_boundary() {
    let clock = FakeClock::new("2026-11-04T12:00:00Z");
    let mut tracker = ClockTracker::default();
    let mut tasks = TaskSession::new();
    tasks.today = Some(view("2026-11-04T12:00:00", Some("2026-11-04T12:01:00")));
    let today_dirty = Cell::new(false);
    let detail_dirty = Cell::new(false);
    observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    clock.advance(60);
    assert!(
        !observe_invalidation(
            &mut tracker,
            &clock,
            &mut tasks,
            &today_dirty,
            &detail_dirty
        )
        .changed
    );
    clock.advance(1);
    let mut queries = 0;
    for _ in 0..20 {
        let check = observe_invalidation(
            &mut tracker,
            &clock,
            &mut tasks,
            &today_dirty,
            &detail_dirty,
        );
        queries += usize::from(check.changed && may_query(None, true));
        clock.advance(30);
    }
    assert_eq!(queries, 1);
    assert!(today_dirty.get() && detail_dirty.get());
}

#[test]
fn test_reopen_before_next_timer_tick_checks_deadline_and_uses_current_local_date() {
    let clock = FakeClock::new("2026-11-04T23:59:00Z");
    let mut tracker = ClockTracker::default();
    let mut tasks = TaskSession::new();
    tasks.today = Some(view("2026-11-04T23:59:00", None));
    let today_dirty = Cell::new(false);
    let detail_dirty = Cell::new(false);
    observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    clock.advance(61);
    let check = observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    assert!(check.changed);
    assert_eq!(
        check.sample.local.date(),
        "2026-11-05".parse::<NaiveDate>().unwrap()
    );
    assert!(needs_page_load(
        today_dirty.get(),
        tasks.today.is_some(),
        tasks.latest_today
    ));
}

#[test]
fn test_clock_rollback_after_query_baseline_invalidates_an_in_flight_read_without_touching_snapshot(
) {
    let clock = FakeClock::new("2026-11-04T13:00:00Z");
    let mut tracker = ClockTracker::default();
    let mut tasks = TaskSession::new();
    let previous = view("2026-11-04T12:00:00", None);
    tasks.today = Some(previous.clone());
    let today_dirty = Cell::new(false);
    let detail_dirty = Cell::new(false);
    observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    tasks.latest_today = 7;
    clock.advance(-30 * 60);
    let check = observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    assert!(!tasks.needs_clock_refresh(check.sample.local));
    assert!(check.changed);
    assert!(today_dirty.get() && detail_dirty.get());
    assert!(tasks.today_requery);
    assert_eq!(tasks.today, Some(previous));
    assert_eq!(tasks.latest_today, 7);
}

#[test]
fn test_timezone_change_invalidates_cache_without_relying_on_date_change() {
    let clock = FakeClock::new("2026-11-04T12:00:00Z");
    let mut tracker = ClockTracker::default();
    let mut tasks = TaskSession::new();
    tasks.today = Some(view("2026-11-04T12:00:00", None));
    let today_dirty = Cell::new(false);
    let detail_dirty = Cell::new(false);
    observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    clock.offset.set(3600);
    let check = observe_invalidation(
        &mut tracker,
        &clock,
        &mut tasks,
        &today_dirty,
        &detail_dirty,
    );
    assert!(check.changed);
    assert_eq!(
        check.sample.local.date(),
        tasks.today.as_ref().unwrap().date
    );
    assert!(today_dirty.get() && detail_dirty.get());
}

#[test]
fn test_clock_checks_precede_querying_and_never_project_or_construct_ui() {
    let source = include_str!("../timing.rs");
    let observe = source
        .split_once("fn observe_invalidation(")
        .unwrap()
        .1
        .split_once("fn refresh_task_clock(")
        .unwrap()
        .0;
    for forbidden in [
        "request_today(",
        "with_ui(",
        "ensure_ui(",
        "TodayPresenter::new",
        ".clone()",
    ] {
        assert!(
            !observe.contains(forbidden),
            "tasks/requests/timing.rs: {forbidden}"
        );
    }
    let refresh = source
        .split_once("fn refresh_task_clock(")
        .unwrap()
        .1
        .split_once("#[cfg(test)]")
        .unwrap()
        .0;
    assert!(
        refresh.find("!self.today_page_visible()").unwrap()
            < refresh.find("self.request_today();").unwrap(),
        "tasks/requests/timing.rs"
    );
    let loading = include_str!("../loading.rs")
        .split_once("fn start_page_tasks(")
        .unwrap()
        .1
        .split_once("fn defer_page_tasks(")
        .unwrap()
        .0;
    assert!(
        loading.find("self.observe_task_clock();").unwrap()
            < loading.find("self.request_today();").unwrap(),
        "tasks/requests/loading.rs"
    );
    let requests = include_str!("../../requests.rs");
    assert!(
        requests.contains("let date = self.observe_task_clock().sample.local.date();"),
        "tasks/requests.rs"
    );
    assert!(
        !requests.contains("SystemClock.today()"),
        "tasks/requests.rs"
    );
}

#[test]
fn test_shared_shell_date_refreshes_before_today_gating_and_hidden_ui_is_not_touched() {
    let source = include_str!("../timing.rs");
    let refresh = source.split_once("fn refresh_task_clock(").unwrap().1;
    assert!(
        refresh
            .find("self.sync_shell_date(check.sample.local.date());")
            .unwrap()
            < refresh.find("!self.today_page_visible()").unwrap()
    );
    let sync = source.split_once("fn sync_shell_date(").unwrap().1;
    assert!(
        sync.find("!self.main_content_visible()").unwrap() < sync.find("self.with_ui(").unwrap()
    );
    assert!(sync.contains("ui.shell.set_local_date(date)"));
    for forbidden in [
        "request_today(",
        "load_calendar(",
        "ensure_ui(",
        "TodayPresenter::new",
    ] {
        assert!(!sync.contains(forbidden), "timing.rs: {forbidden}");
    }
}

#[test]
fn test_initial_clock_and_explicit_restore_reuse_kernel_resources_without_navigation() {
    let loading = include_str!("../loading.rs");
    let start = loading.split_once("fn start_kernel_tasks(").unwrap().1;
    assert!(
        start.find("self.task_day_timer.running()").unwrap()
            < start.find(".start(TimerMode::Repeated").unwrap()
    );
    assert!(include_str!("../../../runtime.rs").contains("runtime.start_kernel_tasks();"));
    let restore = include_str!("../../../runtime/session.rs");
    assert!(restore.contains("let date = self.observe_task_clock().sample.local.date();"));
    assert!(restore.contains("ui.shell.set_local_date(date)"));
}
