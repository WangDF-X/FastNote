use super::*;

fn view() -> TodayView {
    TodayView {
        date: "2026-11-04".parse().unwrap(),
        evaluated_at: "2026-11-04T12:00:00".parse().unwrap(),
        next_overdue_at: None,
        sections: Vec::new(),
        open_count: 0,
        overdue_count: 0,
        completed_today_count: 0,
        tomorrow_count: 0,
    }
}

#[test]
fn test_successful_current_reply_releases_in_flight_state_and_applies_once() {
    let mut tasks = TaskSession::new();
    tasks.latest_today = 7;
    let expected = view();
    assert_eq!(
        complete_today(&mut tasks, 7, Some(expected.clone()), true),
        Some(TodayCompletion {
            applied: true,
            requery: false
        })
    );
    assert_eq!(tasks.latest_today, 0);
    assert_eq!(tasks.today, Some(expected));
    assert_eq!(complete_today(&mut tasks, 7, Some(view()), true), None);
}

#[test]
fn test_hidden_current_reply_releases_request_without_caching_or_projecting_snapshot() {
    let mut tasks = TaskSession::new();
    tasks.latest_today = 7;
    tasks.collapsed[5] = Some(true);
    tasks.limits[5] = 800;
    assert_eq!(
        complete_today(&mut tasks, 7, Some(view()), false),
        Some(TodayCompletion {
            applied: false,
            requery: false
        })
    );
    assert_eq!(tasks.latest_today, 0);
    assert!(tasks.today.is_none());
    assert_eq!(tasks.collapsed[5], Some(true));
    assert_eq!(tasks.limits[5], 800);
}

#[test]
fn test_in_flight_invalidations_are_coalesced_and_old_snapshot_is_not_adopted() {
    let mut tasks = TaskSession::new();
    let previous = view();
    tasks.today = Some(previous.clone());
    tasks.latest_today = 7;
    for _ in 0..20 {
        tasks.invalidate_today_read();
    }
    assert_eq!(
        complete_today(&mut tasks, 7, Some(view()), true),
        Some(TodayCompletion {
            applied: false,
            requery: true
        })
    );
    assert_eq!(tasks.latest_today, 0);
    assert!(!tasks.today_requery);
    assert_eq!(tasks.today, Some(previous));
}

#[test]
fn test_failed_and_unexpected_current_reply_do_not_lock_future_retries() {
    let mut tasks = TaskSession::new();
    for requery in [false, true] {
        tasks.latest_today = 7;
        tasks.today_requery = requery;
        assert_eq!(
            complete_today(&mut tasks, 7, None, true),
            Some(TodayCompletion {
                applied: false,
                requery
            })
        );
        assert_eq!(tasks.latest_today, 0);
        assert!(!tasks.today_requery);
    }
}

#[test]
fn test_stale_reply_cannot_release_new_request_or_clear_its_invalidation() {
    let mut tasks = TaskSession::new();
    tasks.latest_today = 8;
    tasks.today_requery = true;
    for id in [0, 7] {
        assert_eq!(complete_today(&mut tasks, id, Some(view()), true), None);
    }
    assert_eq!(tasks.latest_today, 8);
    assert!(tasks.today_requery);
    assert!(tasks.today.is_none());
}

#[test]
fn test_page_reply_visibility_gate_precedes_error_projection_requery_and_models() {
    let source = include_str!("../today.rs");
    let gate = source.find("if !page_visible {").unwrap();
    for work in [
        "self.request_today();",
        "self.with_ui(|ui|",
        "self.emit_ui(UiEffect::RefreshToday)",
    ] {
        assert!(
            gate < source.find(work).unwrap(),
            "tasks/replies/today.rs: {work}"
        );
    }
    assert!(!source.contains("ensure_ui("), "tasks/replies/today.rs");
    assert!(
        !source.contains("TodayPresenter::new("),
        "tasks/replies/today.rs"
    );
}
