//! 日面板的空态文案和日期辅助信息，不依赖窗口或数据库。

use chrono::{Datelike, NaiveDate};
use fastnote_core::derive::calendar::DayAgenda;

pub(super) fn panel_message(agenda: Option<&DayAgenda>, read_error: bool) -> &'static str {
    if read_error {
        return "日程暂时读取失败，请切换日期重试。";
    }
    match agenda {
        None => "正在读取日程…",
        Some(agenda)
            if agenda.due.is_empty()
                && agenda.start.is_empty()
                && agenda.ongoing.is_empty()
                && agenda.completed.is_empty() =>
        {
            "这一天没有安排的任务"
        }
        Some(_) => "",
    }
}

pub(super) fn weekday_label(date: NaiveDate) -> &'static str {
    ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [date.weekday().num_days_from_monday() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_agenda_has_clear_empty_state() {
        assert_eq!(
            panel_message(Some(&DayAgenda::default()), false),
            "这一天没有安排的任务"
        );
    }

    #[test]
    fn test_loading_and_failed_agenda_do_not_look_like_an_empty_day() {
        assert_eq!(panel_message(None, false), "正在读取日程…");
        assert_eq!(
            panel_message(None, true),
            "日程暂时读取失败，请切换日期重试。"
        );
        assert_eq!(
            panel_message(Some(&DayAgenda::default()), true),
            panel_message(None, true)
        );
    }

    #[test]
    fn test_selected_context_uses_the_dates_actual_weekday() {
        assert_eq!(weekday_label("2026-11-02".parse().unwrap()), "周一");
        assert_eq!(weekday_label("2026-11-07".parse().unwrap()), "周六");
        assert_eq!(weekday_label("2026-11-08".parse().unwrap()), "周日");
    }
}
