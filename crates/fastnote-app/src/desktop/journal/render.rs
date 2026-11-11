use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc, Weekday};
use fastnote_core::derive::calendar::{calendar_week_days, WeekStart};
use fastnote_core::JournalSummary;
use fastnote_ui::JournalRowData;
use slint::{ModelRc, VecModel};

use super::super::runtime::Runtime;
use super::{journal_access, range_for, JournalAccess, JournalView};
use crate::journal::JournalEditor;

impl Runtime {
    /// 投影当前视图：月视图按月分组，周视图固定 7 行。
    pub(super) fn render_journal_rows(&self) {
        let today = self.controller.borrow().shell_view().local_today;
        let week_start = self.settings.borrow().week_start;
        let (summaries, view, selected, loaded) = {
            let session = self.journal.borrow();
            (
                session.rows.clone(),
                session.view,
                session.selected_date,
                session.loaded_range,
            )
        };
        let rows = match view {
            JournalView::Month => journal_rows(&summaries, today),
            JournalView::Week => selected
                .map(|anchor| journal_week_rows(&summaries, anchor, week_start, today))
                .unwrap_or_default(),
        };
        let period = selected
            .map(|anchor| period_label(view, anchor))
            .unwrap_or_default();
        let range = selected.and_then(|anchor| range_for(view, anchor, week_start));
        let empty = view == JournalView::Month
            && range.is_some()
            && loaded == range
            && summaries.is_empty();
        let has_today = range.is_some_and(|range| range.start <= today && today <= range.end)
            && !summaries.iter().any(|entry| entry.entry_date == today);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_journal_rows(ModelRc::new(VecModel::from(rows)));
            window.set_journal_period_label(period.into());
            window.set_journal_week_mode(view == JournalView::Week);
            window.set_journal_empty(empty);
            window.set_journal_has_today(has_today);
        });
    }

    /// 同步单日草稿与保存状态；不覆盖列表选择。
    pub(super) fn sync_journal(&self) {
        let today = self.controller.borrow().shell_view().local_today;
        let session = self.journal.borrow();
        let selected = session.editor_date().or(session.selected_date);
        let (editing, content, error, label, preview, blocks) = match &session.editor {
            Some(editor) => (
                true,
                editor.content.clone(),
                editor.error.clone().unwrap_or_default(),
                editor_save_label(editor, today),
                session.preview,
                if session.preview {
                    crate::notes::preview_blocks(&editor.content)
                } else {
                    Vec::new()
                },
            ),
            None => (
                false,
                String::new(),
                String::new(),
                String::new(),
                false,
                Vec::new(),
            ),
        };
        drop(session);
        self.journal_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_journal_editing(editing);
            window.set_journal_date_label(selected.map(date_label).unwrap_or_default().into());
            window.set_journal_selected_date(
                selected
                    .map(|date| date.to_string())
                    .unwrap_or_default()
                    .into(),
            );
            window.set_journal_content(content.into());
            window.set_journal_error(error.into());
            window.set_journal_save_label(label.into());
            window.set_journal_preview_mode(preview);
            window.set_journal_preview_blocks(ModelRc::new(VecModel::from(blocks)));
        });
        self.journal_syncing.set(false);
    }
}

fn editor_save_label(editor: &JournalEditor, today: NaiveDate) -> String {
    if editor.error.is_some() {
        "保存失败".to_owned()
    } else if editor.pending.is_some() {
        "正在保存…".to_owned()
    } else if editor.dirty() {
        "未保存".to_owned()
    } else {
        editor
            .baseline
            .as_ref()
            .map(|entry| save_label(entry.updated_at, today))
            .unwrap_or_default()
    }
}

/// 当天只显示时分；跨天补月日，跨年补完整日期，避免误以为是今天保存。
fn save_label(updated_at: DateTime<Utc>, today: NaiveDate) -> String {
    format!("已保存 {}", list_time_label(updated_at, today))
}

/// 列表与状态行共用的时间格式：今天 `HH:mm`，同年 `M月D日 HH:mm`，跨年补年份。
fn list_time_label(updated_at: DateTime<Utc>, today: NaiveDate) -> String {
    let local = updated_at.with_timezone(&Local);
    if local.date_naive() == today {
        local.format("%H:%M").to_string()
    } else if local.year() == today.year() {
        format!(
            "{}月{}日 {}",
            local.month(),
            local.day(),
            local.format("%H:%M")
        )
    } else {
        format!(
            "{}年{}月{}日 {}",
            local.year(),
            local.month(),
            local.day(),
            local.format("%H:%M")
        )
    }
}

/// 月视图显示年月；周视图显示 ISO 年和周序号。
pub(super) fn period_label(view: JournalView, anchor: NaiveDate) -> String {
    match view {
        JournalView::Month => format!("{}年{}月", anchor.year(), anchor.month()),
        JournalView::Week => {
            let iso = anchor.iso_week();
            format!("{}年第{}周", iso.year(), iso.week())
        }
    }
}

pub(super) fn date_label(date: NaiveDate) -> String {
    format!(
        "{}年{}月{}日 {}",
        date.year(),
        date.month(),
        date.day(),
        weekday_label(date)
    )
}

fn weekday_label(date: NaiveDate) -> &'static str {
    match date.weekday() {
        Weekday::Mon => "周一",
        Weekday::Tue => "周二",
        Weekday::Wed => "周三",
        Weekday::Thu => "周四",
        Weekday::Fri => "周五",
        Weekday::Sat => "周六",
        Weekday::Sun => "周日",
    }
}

/// 0 今天、1 昨日、2 一周内、3 更早、4 未来；未来日期允许记录（Q-DIARY-01）。
fn group_key(date: NaiveDate, today: NaiveDate) -> i32 {
    if date == today {
        0
    } else if date == today - chrono::Duration::days(1) {
        1
    } else if date > today {
        4
    } else if (today - date).num_days() <= 7 {
        2
    } else {
        3
    }
}

fn group_label(key: i32, (latest, earliest): (NaiveDate, NaiveDate)) -> String {
    match key {
        0 => "今天".to_owned(),
        1 => "昨日".to_owned(),
        2 if earliest == latest => format!("{}月{}日", earliest.month(), earliest.day()),
        2 => format!(
            "{}月{}日至{}月{}日",
            earliest.month(),
            earliest.day(),
            latest.month(),
            latest.day()
        ),
        4 => "以后".to_owned(),
        _ => "更早".to_owned(),
    }
}

pub(super) fn journal_rows(summaries: &[JournalSummary], today: NaiveDate) -> Vec<JournalRowData> {
    let mut bounds: BTreeMap<i32, (NaiveDate, NaiveDate)> = BTreeMap::new();
    for summary in summaries {
        let bounds = bounds
            .entry(group_key(summary.entry_date, today))
            .or_insert((summary.entry_date, summary.entry_date));
        if summary.entry_date > bounds.0 {
            bounds.0 = summary.entry_date;
        }
        if summary.entry_date < bounds.1 {
            bounds.1 = summary.entry_date;
        }
    }
    let mut rows = Vec::with_capacity(summaries.len());
    let mut previous = None;
    for summary in summaries {
        let key = group_key(summary.entry_date, today);
        let group = if previous == Some(key) {
            String::new()
        } else {
            group_label(key, bounds[&key])
        };
        previous = Some(key);
        let excerpt = summary_excerpt(summary);
        rows.push(JournalRowData {
            group: group.into(),
            date: summary.entry_date.to_string().into(),
            axis: summary.entry_date.day().to_string().into(),
            caption: weekday_label(summary.entry_date).into(),
            excerpt: excerpt.into(),
            image_count: image_count_label(summary).into(),
            time: summary_time(summary, today).into(),
            is_today: summary.entry_date == today,
            openable: matches!(
                journal_access(summary.entry_date, today, true),
                JournalAccess::Editable
            ),
            placeholder: false,
        });
    }
    rows
}

/// 周视图固定 7 行：过去的空日期显示「未记录」，未来显示「未到日期」。
pub(super) fn journal_week_rows(
    summaries: &[JournalSummary],
    anchor: NaiveDate,
    week_start: WeekStart,
    today: NaiveDate,
) -> Vec<JournalRowData> {
    let Some(days) = calendar_week_days(anchor, week_start) else {
        return Vec::new();
    };
    let by_date: BTreeMap<NaiveDate, &JournalSummary> = summaries
        .iter()
        .map(|summary| (summary.entry_date, summary))
        .collect();
    days.into_iter()
        .map(|date| {
            let summary = by_date.get(&date).copied();
            let access = journal_access(date, today, summary.is_some());
            let future = date > today;
            let excerpt = if future {
                "未到日期".to_owned()
            } else if let Some(summary) = summary {
                summary_excerpt(summary)
            } else if date == today {
                "今天还没写".to_owned()
            } else {
                "未记录".to_owned()
            };
            JournalRowData {
                group: "".into(),
                date: date.to_string().into(),
                axis: weekday_label(date).into(),
                caption: format!("{}/{}", date.month(), date.day()).into(),
                excerpt: excerpt.into(),
                image_count: if future {
                    String::new()
                } else {
                    summary.map(image_count_label).unwrap_or_default()
                }
                .into(),
                time: if future {
                    String::new()
                } else {
                    summary
                        .map(|summary| summary_time(summary, today))
                        .unwrap_or_default()
                }
                .into(),
                is_today: date == today,
                openable: matches!(access, JournalAccess::Editable),
                placeholder: future || summary.is_none(),
            }
        })
        .collect()
}

fn summary_excerpt(summary: &JournalSummary) -> String {
    if summary.excerpt.is_empty() {
        "无正文".to_owned()
    } else {
        summary.excerpt.clone()
    }
}

fn image_count_label(summary: &JournalSummary) -> String {
    if summary.attachment_count == 0 {
        String::new()
    } else {
        format!("{} 图", summary.attachment_count)
    }
}

fn summary_time(summary: &JournalSummary, today: NaiveDate) -> String {
    list_time_label(summary.updated_at, today)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    fn summary(entry_date: NaiveDate, excerpt: &str) -> JournalSummary {
        JournalSummary {
            entry_date,
            excerpt: excerpt.to_owned(),
            updated_at: "2026-10-23T06:32:00Z".parse().unwrap(),
            attachment_count: 0,
        }
    }

    #[test]
    fn test_groups_are_split_into_today_yesterday_week_and_earlier() {
        let today = date(10, 23);
        let rows = journal_rows(
            &[
                summary(date(10, 23), "今天"),
                summary(date(10, 22), "昨天"),
                summary(date(10, 19), "一周"),
                summary(date(10, 18), "同一周"),
                summary(date(10, 1), "更早"),
            ],
            today,
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.group.as_str())
                .collect::<Vec<_>>(),
            vec!["今天", "昨日", "10月18日至10月19日", "", "更早"]
        );
        assert!(rows[0].is_today);
        assert!(!rows[1].is_today);
    }

    #[test]
    fn test_future_entries_get_their_own_group_and_are_not_openable() {
        let today = date(10, 23);
        let rows = journal_rows(&[summary(date(10, 25), "以后")], today);
        assert_eq!(rows[0].group, "以后");
        assert!(!rows[0].openable);
        assert!(!rows[0].placeholder);
    }

    #[test]
    fn test_blank_excerpt_falls_back_to_no_content() {
        let today = date(10, 23);
        let rows = journal_rows(&[summary(today, "")], today);
        assert_eq!(rows[0].excerpt, "无正文");
        assert_eq!(rows[0].axis, "23");
        assert_eq!(rows[0].caption, "周五");
    }

    #[test]
    fn test_image_count_is_separate_from_excerpt() {
        let today = date(10, 23);
        let mut only_images = summary(today, "");
        only_images.attachment_count = 2;
        let mut text_and_images = summary(date(10, 22), "写了报告");
        text_and_images.attachment_count = 3;
        let rows = journal_rows(&[only_images, text_and_images], today);
        assert_eq!(rows[0].excerpt, "无正文");
        assert_eq!(rows[0].image_count, "2 图");
        assert_eq!(rows[1].excerpt, "写了报告");
        assert_eq!(rows[1].image_count, "3 图");
        assert_eq!(
            journal_rows(&[summary(today, "文字")], today)[0].image_count,
            ""
        );
    }

    #[test]
    fn test_empty_month_has_no_rows() {
        assert!(journal_rows(&[], date(10, 23)).is_empty());
    }

    #[test]
    fn test_date_and_period_labels_are_localized() {
        assert_eq!(date_label(date(10, 23)), "2026年10月23日 周五");
        assert_eq!(period_label(JournalView::Month, date(10, 1)), "2026年10月");
    }

    #[test]
    fn test_week_rows_cover_all_seven_days_and_mark_editable_days() {
        let mut monday = summary(date(11, 9), "周一记录");
        monday.attachment_count = 2;
        let rows = journal_week_rows(
            &[monday, summary(date(11, 11), "周三记录")],
            date(11, 11),
            WeekStart::Monday,
            date(11, 11),
        );
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0].axis, "周一");
        assert_eq!(rows[0].caption, "11/9");
        assert_eq!(rows[0].excerpt, "周一记录");
        assert_eq!(rows[0].image_count, "2 图");
        assert!(rows[0].openable);
        assert_eq!(rows[1].axis, "周二");
        assert_eq!(rows[1].excerpt, "未记录");
        assert_eq!(rows[1].image_count, "");
        assert!(rows[1].placeholder);
        assert!(!rows[1].openable);
        assert_eq!(rows[2].axis, "周三");
        assert!(rows[2].is_today);
        assert!(rows[2].openable);
        assert_eq!(rows[3].excerpt, "未到日期");
        assert!(!rows[3].openable);
        assert_eq!(rows[6].axis, "周日");
        assert_eq!(rows[6].date, "2026-11-15");

        let sunday = journal_week_rows(&[], date(11, 11), WeekStart::Sunday, date(11, 11));
        assert_eq!(sunday[0].axis, "周日");
        assert_eq!(sunday[0].caption, "11/8");
        assert_eq!(sunday[0].excerpt, "未记录");
        assert_eq!(sunday[3].excerpt, "今天还没写");
        assert!(sunday[3].openable);
        assert_eq!(sunday[6].axis, "周六");
        assert_eq!(sunday[6].caption, "11/14");
        assert_eq!(sunday[6].excerpt, "未到日期");
        assert!(!sunday[6].openable);
    }

    #[test]
    fn test_week_period_label_shows_iso_year_and_week() {
        assert_eq!(
            period_label(JournalView::Week, date(11, 11)),
            "2026年第46周"
        );
        assert_eq!(period_label(JournalView::Week, date(12, 1)), "2026年第49周");
        assert_eq!(
            period_label(
                JournalView::Week,
                NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
            ),
            "2026年第53周"
        );
        assert_eq!(
            period_label(
                JournalView::Week,
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
            ),
            "2026年第1周"
        );
    }

    #[test]
    fn test_list_time_adds_date_outside_today() {
        use chrono::TimeZone;

        let local = Local
            .with_ymd_and_hms(2026, 11, 10, 14, 32, 0)
            .single()
            .unwrap();
        let updated_at = local.with_timezone(&Utc);
        assert_eq!(list_time_label(updated_at, date(11, 10)), "14:32");
        assert_eq!(list_time_label(updated_at, date(11, 11)), "11月10日 14:32");
        assert_eq!(
            list_time_label(updated_at, NaiveDate::from_ymd_opt(2027, 1, 2).unwrap()),
            "2026年11月10日 14:32"
        );
    }

    #[test]
    fn test_save_label_adds_date_only_when_not_saved_today() {
        use chrono::TimeZone;

        let local = Local
            .with_ymd_and_hms(2026, 11, 10, 14, 32, 0)
            .single()
            .unwrap();
        let updated_at = local.with_timezone(&Utc);
        assert_eq!(save_label(updated_at, date(11, 10)), "已保存 14:32");
        assert_eq!(
            save_label(updated_at, date(11, 11)),
            "已保存 11月10日 14:32"
        );
        assert_eq!(
            save_label(updated_at, NaiveDate::from_ymd_opt(2027, 1, 2).unwrap()),
            "已保存 2026年11月10日 14:32"
        );
    }
}
