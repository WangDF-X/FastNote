use super::*;
use fastnote_core::derive::calendar::WeekStart;

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[test]
fn test_range_for_month_and_week_follow_view_and_week_start() {
    let anchor = date(2026, 11, 11);
    let month = range_for(JournalView::Month, anchor, WeekStart::Monday).unwrap();
    assert_eq!(
        (month.start, month.end),
        (date(2026, 11, 1), date(2026, 11, 30))
    );
    let week = range_for(JournalView::Week, anchor, WeekStart::Monday).unwrap();
    assert_eq!(
        (week.start, week.end),
        (date(2026, 11, 9), date(2026, 11, 15))
    );
    let sunday = range_for(JournalView::Week, anchor, WeekStart::Sunday).unwrap();
    assert_eq!(
        (sunday.start, sunday.end),
        (date(2026, 11, 8), date(2026, 11, 14))
    );
}

#[test]
fn test_month_bounds_cover_leap_february_and_december() {
    assert_eq!(
        month_bounds(date(2024, 2, 1)),
        (date(2024, 2, 1), date(2024, 2, 29))
    );
    assert_eq!(
        month_bounds(date(2026, 12, 1)),
        (date(2026, 12, 1), date(2026, 12, 31))
    );
    assert_eq!(
        month_bounds(date(2026, 4, 18)),
        (date(2026, 4, 1), date(2026, 4, 30))
    );
}

#[test]
fn test_shift_month_keeps_day_and_clamps_at_month_end() {
    assert_eq!(shift_month(date(2026, 10, 1), 23, 1), date(2026, 11, 23));
    assert_eq!(shift_month(date(2026, 10, 1), 31, 1), date(2026, 11, 30));
    assert_eq!(shift_month(date(2026, 1, 1), 31, -1), date(2025, 12, 31));
    assert_eq!(shift_month(date(2026, 3, 1), 30, -1), date(2026, 2, 28));
}

#[test]
fn test_journal_access_allows_today_and_existing_past_only() {
    let today = date(2026, 11, 11);
    assert_eq!(journal_access(today, today, false), JournalAccess::Editable);
    assert_eq!(
        journal_access(date(2026, 11, 10), today, true),
        JournalAccess::Editable
    );
    assert_eq!(
        journal_access(date(2026, 11, 10), today, false),
        JournalAccess::MissingPast
    );
    assert_eq!(
        journal_access(date(2026, 11, 12), today, true),
        JournalAccess::Future
    );
    assert_eq!(
        journal_access(date(2026, 11, 12), today, false),
        JournalAccess::Future
    );
}

#[test]
fn test_view_defaults_to_month_and_needs_load_only_for_new_ranges() {
    let session = JournalSession::default();
    assert_eq!(session.view, JournalView::Month);
    let month = range_for(JournalView::Month, date(2026, 11, 11), WeekStart::Monday).unwrap();
    assert!(session.needs_load(month));

    let mut loaded = JournalSession {
        loaded_range: Some(month),
        ..JournalSession::default()
    };
    assert!(!loaded.needs_load(month));
    loaded.pending_range = Some(month);
    loaded.loaded_range = None;
    assert!(!loaded.needs_load(month));

    let week = range_for(JournalView::Week, date(2026, 11, 11), WeekStart::Monday).unwrap();
    assert!(loaded.needs_load(week));
}

#[test]
fn test_blank_session_is_not_protected() {
    let session = JournalSession::default();
    assert!(!session.protected());
    assert_eq!(session.editor_date(), None);
}

/// 保存回包刷新列表前必须先释放 RefCell 借用：`if let ... borrow()` 的守卫会活到块结束，
/// 块内再调用 `request_journal_range`（内部 borrow_mut）会直接 panic。
#[test]
fn test_save_reply_releases_session_borrow_before_refreshing_list() {
    let source = include_str!("replies.rs");
    assert!(!source.contains("if let Some(month) = self.journal.borrow().month"));
    assert!(!source.contains("if let Some(date) = self.journal.borrow().editor_date()"));
    assert!(source.contains("self.refresh_journal_summaries();"));
    let attachments = include_str!("attachments.rs");
    assert!(!attachments.contains("if let Some(date) = self.journal.borrow().editor_date()"));
    assert!(!attachments.contains("if let Some(month) = self.journal.borrow().month"));
    let journal = include_str!("../journal.rs");
    assert!(!journal.contains("if let Some(date) = self.journal.borrow().editor_date()"));
    assert!(!journal.contains("if let Some(month) = self.journal.borrow().month"));
}
