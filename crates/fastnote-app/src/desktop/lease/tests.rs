use std::cell::Cell;

use super::*;

struct Ticks(Cell<u64>);
impl TickSource for Ticks {
    fn milliseconds(&self) -> u64 {
        self.0.get()
    }
}

fn mounted(clock: &Ticks) -> UiLease {
    let mut lease = UiLease::default();
    assert_eq!(lease.apply(LeaseEvent::Mounted, clock), LeaseAction::None);
    lease
}

fn hidden(lease: &mut UiLease, clock: &Ticks) -> (u64, u64) {
    match lease.apply(LeaseEvent::Hidden { protected: false }, clock) {
        LeaseAction::Arm {
            deadline,
            generation,
        } => (deadline, generation),
        other => panic!("expected new lease, got {other:?}"),
    }
}

#[test]
fn test_cold_has_no_lease_and_hidden_events_do_not_construct_resources() {
    let clock = Ticks(Cell::new(0));
    let mut lease = UiLease::default();
    assert_eq!(lease.state(), LeaseState::Cold);
    assert_eq!(lease.label(), "cold");
    assert_eq!(
        lease.apply(LeaseEvent::Hidden { protected: false }, &clock),
        LeaseAction::None
    );
    assert_eq!(lease.expired(&clock), None);
    assert_eq!(lease.apply(LeaseEvent::Exit, &clock), LeaseAction::None);
}

#[test]
fn test_saved_hidden_window_releases_at_exact_120_second_boundary() {
    let clock = Ticks(Cell::new(4_000));
    let mut lease = mounted(&clock);
    let (deadline, generation) = hidden(&mut lease, &clock);
    assert_eq!(deadline, 124_000);
    assert_eq!(lease.label(), "hot");
    clock.0.set(deadline - 1);
    assert_eq!(lease.expired(&clock), None);
    assert_eq!(
        lease.apply(LeaseEvent::Tick { generation }, &clock),
        LeaseAction::Arm {
            deadline,
            generation
        }
    );
    clock.0.set(deadline);
    assert_eq!(lease.expired(&clock), Some(generation));
    assert_eq!(
        lease.apply(LeaseEvent::Tick { generation }, &clock),
        LeaseAction::Release(ReleaseReason::Expired)
    );
    assert_eq!(lease.state(), LeaseState::Cold);
}

#[test]
fn test_reopen_cancels_old_timer_and_second_hide_starts_a_new_generation() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    let (_, old) = hidden(&mut lease, &clock);
    clock.0.set(60_000);
    lease.apply(LeaseEvent::Shown, &clock);
    assert_eq!(lease.label(), "active");
    clock.0.set(70_000);
    let (deadline, current) = hidden(&mut lease, &clock);
    assert_eq!(deadline, 190_000);
    assert_ne!(old, current);
    clock.0.set(200_000);
    assert_eq!(
        lease.apply(LeaseEvent::Tick { generation: old }, &clock),
        LeaseAction::None
    );
    assert_eq!(
        lease.apply(
            LeaseEvent::Tick {
                generation: current
            },
            &clock
        ),
        LeaseAction::Release(ReleaseReason::Expired)
    );
}

#[test]
fn test_ball_menu_theme_and_background_rechecks_do_not_renew_hot_deadline() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    let (deadline, generation) = hidden(&mut lease, &clock);
    for now in [1, 30_000, 80_000, 119_999] {
        clock.0.set(now);
        assert_eq!(
            lease.apply(LeaseEvent::Hidden { protected: false }, &clock),
            LeaseAction::None
        );
        assert_eq!(
            lease.state(),
            LeaseState::Hot {
                deadline,
                generation
            }
        );
    }
}

#[test]
fn test_dirty_in_flight_failed_or_modal_guard_does_not_enter_hot() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    assert_eq!(
        lease.apply(LeaseEvent::Hidden { protected: true }, &clock),
        LeaseAction::None
    );
    assert_eq!(lease.label(), "protected");
    clock.0.set(900_000);
    assert_eq!(lease.expired(&clock), None);
    assert_eq!(
        lease.apply(
            LeaseEvent::Evict {
                reason: ReleaseReason::Pressure,
                protected: true
            },
            &clock
        ),
        LeaseAction::None
    );
    let (deadline, _) = hidden(&mut lease, &clock);
    assert_eq!(deadline, 1_020_000);
}

#[test]
fn test_guard_appearing_while_hidden_blocks_release_without_replacing_original_deadline() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    let (_, generation) = hidden(&mut lease, &clock);
    lease.apply(LeaseEvent::Hidden { protected: true }, &clock);
    clock.0.set(121_000);
    assert_eq!(
        lease.apply(LeaseEvent::Tick { generation }, &clock),
        LeaseAction::None
    );
    assert_eq!(lease.label(), "protected");
    assert_eq!(
        lease.apply(LeaseEvent::Hidden { protected: false }, &clock),
        LeaseAction::Release(ReleaseReason::Expired)
    );
}

#[test]
fn test_clearing_protection_before_expiry_resumes_only_the_original_remaining_lease() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    let (deadline, generation) = hidden(&mut lease, &clock);
    clock.0.set(10_000);
    lease.apply(LeaseEvent::Hidden { protected: true }, &clock);
    clock.0.set(100_000);
    assert_eq!(
        lease.apply(LeaseEvent::Hidden { protected: false }, &clock),
        LeaseAction::Arm {
            deadline,
            generation
        }
    );
    assert_eq!(deadline, 120_000);
}

#[test]
fn test_failed_hidden_show_can_discard_an_unprotected_session_but_not_input() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    assert_eq!(
        lease.apply(
            LeaseEvent::Discard {
                reason: ReleaseReason::ShowFailure,
                protected: true
            },
            &clock
        ),
        LeaseAction::None
    );
    assert_eq!(lease.state(), LeaseState::Active);
    assert_eq!(
        lease.apply(
            LeaseEvent::Discard {
                reason: ReleaseReason::ShowFailure,
                protected: false
            },
            &clock
        ),
        LeaseAction::Release(ReleaseReason::ShowFailure)
    );
    assert_eq!(lease.state(), LeaseState::Cold);
}

#[test]
fn test_sleep_is_counted_and_first_wakeup_check_expires_without_new_lease() {
    let clock = Ticks(Cell::new(10_000));
    let mut lease = mounted(&clock);
    let (_, generation) = hidden(&mut lease, &clock);
    clock.0.set(600_000);
    assert_eq!(lease.expired(&clock), Some(generation));
    assert_eq!(
        lease.apply(LeaseEvent::Tick { generation }, &clock),
        LeaseAction::Release(ReleaseReason::Expired)
    );
}

#[test]
fn test_pressure_and_budget_release_hot_but_never_active_window() {
    let clock = Ticks(Cell::new(0));
    for reason in [ReleaseReason::Pressure, ReleaseReason::Budget] {
        let mut lease = mounted(&clock);
        assert_eq!(
            lease.apply(
                LeaseEvent::Evict {
                    reason,
                    protected: false
                },
                &clock
            ),
            LeaseAction::None
        );
        hidden(&mut lease, &clock);
        assert_eq!(
            lease.apply(
                LeaseEvent::Evict {
                    reason,
                    protected: false
                },
                &clock
            ),
            LeaseAction::Release(reason)
        );
    }
}

#[test]
fn test_complete_exit_releases_immediately_without_waiting_for_deadline() {
    let clock = Ticks(Cell::new(0));
    for hot in [false, true] {
        let mut lease = mounted(&clock);
        if hot {
            hidden(&mut lease, &clock);
        }
        assert_eq!(
            lease.apply(LeaseEvent::Exit, &clock),
            LeaseAction::Release(ReleaseReason::Exit)
        );
        assert_eq!(lease.apply(LeaseEvent::Exit, &clock), LeaseAction::None);
    }
}

#[test]
fn test_counter_or_tick_overflow_releases_instead_of_extending_or_reusing_generation() {
    let clock = Ticks(Cell::new(0));
    let mut lease = mounted(&clock);
    lease.generation = u64::MAX;
    assert_eq!(
        lease.apply(LeaseEvent::Hidden { protected: false }, &clock),
        LeaseAction::Release(ReleaseReason::Expired)
    );
    let mut lease = mounted(&clock);
    clock.0.set(u64::MAX - 1);
    assert_eq!(
        lease.apply(LeaseEvent::Hidden { protected: false }, &clock),
        LeaseAction::Release(ReleaseReason::Expired)
    );
}
