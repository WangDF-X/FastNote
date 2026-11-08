use super::*;

#[test]
fn test_today_sort_values_round_trip_without_persisting_ui_labels() {
    for (sort, value) in [
        (TodaySort::Priority, "priority"),
        (TodaySort::DueDate, "due_date"),
        (TodaySort::CreatedDescending, "created_desc"),
    ] {
        assert_eq!(sort.as_str(), value);
        assert_eq!(TodaySort::parse(value), Some(sort));
    }
    for invalid in ["", "优先级", "created", "priority ", "DUE_DATE"] {
        assert_eq!(TodaySort::parse(invalid), None);
    }
}

#[test]
fn test_task_preference_changes_use_only_registered_keys_and_preserve_other_settings() {
    for (change, key) in [
        (
            DesktopSetting::TodaySort(TodaySort::DueDate),
            "tasks.default_sort",
        ),
        (
            DesktopSetting::TodayShowUnscheduled(false),
            "today.show_unscheduled",
        ),
        (
            DesktopSetting::TodayShowCompleted(false),
            "today.show_completed",
        ),
    ] {
        assert_eq!(change.key(), key);
        let mut settings = DesktopSettings::default();
        let mut expected = TodayPreferences::default();
        match &change {
            DesktopSetting::TodaySort(value) => expected.sort = *value,
            DesktopSetting::TodayShowUnscheduled(value) => expected.show_unscheduled = *value,
            DesktopSetting::TodayShowCompleted(value) => expected.show_completed = *value,
            _ => panic!("only task preferences are exercised"),
        }
        settings.apply(&change);
        assert_eq!(settings.today_preferences, expected);
        assert_eq!(settings.entry_mode, EntryMode::Ball);
        assert_eq!(settings.theme, ThemeMode::System);
        assert_eq!(settings.close_behavior, CloseBehavior::Entry);
        assert_eq!(settings.geometry, None);
        assert!(!settings.first_run_done);
    }
}

#[test]
fn test_group_visibility_changes_leave_focus_later_and_collapse_default_unchanged() {
    use crate::TodaySectionKind;

    let mut settings = DesktopSettings::default();
    for change in [
        DesktopSetting::TodayShowUnscheduled(false),
        DesktopSetting::TodayShowCompleted(false),
    ] {
        settings.apply(&change);
    }
    for kind in [
        TodaySectionKind::Unscheduled,
        TodaySectionKind::CompletedToday,
    ] {
        assert!(!settings.today_preferences.enabled(kind));
    }
    for kind in [TodaySectionKind::Focus, TodaySectionKind::Later] {
        assert!(settings.today_preferences.enabled(kind));
    }
    assert!(settings.today_preferences.later_collapsed);
}
