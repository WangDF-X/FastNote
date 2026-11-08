use fastnote_core::{parse_task_date, Datelike, NaiveDate, NaiveTime};

/// D-TODAY-04/05：只显示真实日期/时分；当年省略年份，当前年份由本地今日快照注入。
pub(super) fn schedule_label(
    start_date: Option<NaiveDate>,
    start_time: Option<NaiveTime>,
    due_date: Option<NaiveDate>,
    due_time: Option<NaiveTime>,
    current_year: i32,
) -> String {
    let start = start_date.map(|date| date_label(date, start_time, current_year));
    let due = due_date.map(|date| date_label(date, due_time, current_year));
    match (start, due) {
        (Some(start), Some(due)) => format!("{start} → {due}"),
        (Some(date), None) | (None, Some(date)) => date,
        (None, None) => String::new(),
    }
}

/// 详情和弹层仅转换可解析的显示日期；不修改原始草稿，不掩盖非法输入。
pub(super) fn display_date(value: &str, current_year: i32) -> String {
    match parse_task_date(value) {
        Ok(Some(date)) => date_part(date, current_year),
        _ => value.to_owned(),
    }
}

pub(super) fn date_label(date: NaiveDate, time: Option<NaiveTime>, current_year: i32) -> String {
    let date = date_part(date, current_year);
    match time {
        Some(time) => format!("{date} {}", time.format("%H:%M")),
        None => date,
    }
}

fn date_part(date: NaiveDate, current_year: i32) -> String {
    if date.year() == current_year {
        date.format("%m-%d").to_string()
    } else {
        date.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unset_dates_do_not_create_a_summary_or_fabricate_a_date_for_orphan_times() {
        assert!(schedule_label(None, None, None, None, 2026).is_empty());
        let time = Some("09:05:00".parse().unwrap());
        assert!(schedule_label(None, time, None, time, 2026).is_empty());
    }

    #[test]
    fn test_single_start_or_due_date_has_no_action_label_or_extra_separator() {
        let date = Some("2026-11-03".parse().unwrap());
        let time = Some("09:05:00".parse().unwrap());
        assert_eq!(schedule_label(date, None, None, None, 2026), "11-03");
        assert_eq!(schedule_label(None, None, date, None, 2026), "11-03");
        assert_eq!(schedule_label(date, time, None, None, 2026), "11-03 09:05");
        assert_eq!(schedule_label(None, None, date, time, 2026), "11-03 09:05");
        assert_eq!(
            schedule_label(date, time, None, None, 2025),
            "2026-11-03 09:05"
        );
    }

    #[test]
    fn test_two_endpoints_are_an_ordered_range_with_precise_minutes_and_no_labels() {
        let start = Some("2026-11-03".parse().unwrap());
        let due = Some("2027-01-01".parse().unwrap());
        assert_eq!(
            schedule_label(
                start,
                Some("09:05:00".parse().unwrap()),
                due,
                Some("15:31:00".parse().unwrap()),
                2026,
            ),
            "11-03 09:05 → 2027-01-01 15:31"
        );
        assert_eq!(
            schedule_label(start, None, due, None, 2026),
            "11-03 → 2027-01-01"
        );
    }

    #[test]
    fn test_same_day_and_mixed_all_day_endpoints_keep_their_original_precision() {
        let date = Some("2026-11-03".parse().unwrap());
        let time = Some("15:31:00".parse().unwrap());
        assert_eq!(
            schedule_label(date, None, date, time, 2026),
            "11-03 → 11-03 15:31"
        );
        assert_eq!(
            schedule_label(date, time, date, None, 2026),
            "11-03 15:31 → 11-03"
        );
    }

    #[test]
    fn test_display_dates_use_local_current_year_and_preserve_raw_draft_values() {
        for (input, expected) in [
            ("2026-01-02", "01-02"),
            ("2025-01-02", "2025-01-02"),
            ("2027-01-02", "2027-01-02"),
            ("", ""),
            ("invalid-date", "invalid-date"),
            ("2026-1-2", "2026-1-2"),
        ] {
            let draft = input.to_owned();
            assert_eq!(display_date(&draft, 2026), expected);
            assert_eq!(draft, input);
        }
        assert_eq!(display_date("2024-02-29", 2024), "02-29");
    }

    #[test]
    fn test_year_rollover_reformats_each_endpoint_without_reinterpreting_its_date() {
        let start = Some("2026-12-31".parse().unwrap());
        let due = Some("2027-01-01".parse().unwrap());
        assert_eq!(
            schedule_label(start, None, due, None, 2026),
            "12-31 → 2027-01-01"
        );
        assert_eq!(
            schedule_label(start, None, due, None, 2027),
            "2026-12-31 → 01-01"
        );
        assert_eq!(
            schedule_label(start, None, due, None, 2028),
            "2026-12-31 → 2027-01-01"
        );
        assert_eq!(display_date("2026-12-31", 2027), "2026-12-31");
        assert_eq!(display_date("2027-01-01", 2027), "01-01");
    }
}
