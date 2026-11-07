//! 日历页面的轻量导航状态、查询生命周期和展示投影。

use std::rc::Rc;

use chrono::{Datelike, Duration, Months, NaiveDate};
use fastnote_core::derive::calendar::{
    calendar_month_days, calendar_week_days, DayAgenda, WeekStart,
};
use fastnote_core::{Clock, Query, QueryResult};
use fastnote_store::TaskStoreError;
use fastnote_ui::{cell_preview, day_sections, CalendarCellView};
use slint::{ModelRc, SharedString, VecModel};

use crate::clock::SystemClock;
use crate::desktop::runtime::Runtime;

use self::presentation::{panel_message, weekday_label};

mod presentation;

#[derive(Default)]
pub(super) struct CalendarSession {
    selected: Option<NaiveDate>,
    week: bool,
    panel: bool,
    latest: u64,
    data: Vec<(NaiveDate, DayAgenda)>,
    read_error: bool,
}

impl CalendarSession {
    fn release_data(&mut self) {
        self.latest = 0;
        self.data.clear();
        self.read_error = false;
    }

    fn take_reply(&mut self, id: u64) -> bool {
        if id == 0 || self.latest != id {
            return false;
        }
        self.latest = 0;
        true
    }
}

impl Runtime {
    pub(super) fn calendar_page_visible(&self) -> bool {
        self.main_content_visible()
            && self.controller.borrow().shell_view().page_label == "日历"
            && self
                .with_ui(|ui| ui.shell.component().get_current_page() == "日历")
                .unwrap_or(false)
    }

    pub(super) fn connect_calendar(self: &Rc<Self>) {
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.on_calendar_action({
                let weak = self.ui_target();
                move |action| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.calendar_action(action.as_str());
                    }
                }
            });
            window.on_calendar_select_date({
                let weak = self.ui_target();
                move |value| {
                    if let Some(runtime) = weak.upgrade() {
                        if let Ok(date) = NaiveDate::parse_from_str(value.as_str(), "%Y-%m-%d") {
                            runtime.calendar_select(date);
                        }
                    }
                }
            });
        });
    }

    pub(super) fn release_calendar(&self) {
        let mut state = self.calendar.borrow_mut();
        state.release_data();
        drop(state);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_calendar_days(ModelRc::default());
            ui.shell
                .component()
                .set_calendar_panel_sections(ModelRc::default());
            ui.shell.component().set_calendar_panel_message("".into());
            ui.shell.component().set_calendar_error("".into());
        });
    }

    pub(super) fn load_calendar(&self) {
        if !self.calendar_page_visible() || !self.ready.get() {
            return;
        }
        let today = SystemClock.today();
        let (selected, week, pending) = {
            let mut state = self.calendar.borrow_mut();
            (
                *state.selected.get_or_insert(today),
                state.week,
                state.latest != 0,
            )
        };
        if pending {
            return;
        }
        self.calendar.borrow_mut().read_error = false;
        let _ = self.with_ui(|ui| ui.shell.component().set_calendar_error("".into()));
        let start = self.settings.borrow().week_start;
        let days = if week {
            calendar_week_days(selected, start)
        } else {
            calendar_month_days(selected, start)
        };
        let Some(days) = days else {
            return;
        };
        self.paint_calendar(&days);
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self
            .worker
            .query(
                id,
                Query::Calendar {
                    start: days[0],
                    end: days[days.len() - 1],
                },
            )
            .is_ok()
        {
            self.calendar.borrow_mut().latest = id;
        } else {
            self.calendar.borrow_mut().read_error = true;
            self.paint_calendar(&days);
            let _ = self.with_ui(|ui| {
                ui.shell
                    .component()
                    .set_calendar_error("日历读取队列不可用，请重试。".into())
            });
        }
    }

    fn calendar_action(&self, action: &str) {
        if !self.calendar_page_visible() {
            return;
        }
        let mut state = self.calendar.borrow_mut();
        let date = state.selected.unwrap_or_else(|| SystemClock.today());
        match action {
            "previous" | "next" => {
                let next = action == "next";
                let candidate = if state.week {
                    date.checked_add_signed(Duration::days(if next { 7 } else { -7 }))
                } else if next {
                    date.checked_add_months(Months::new(1))
                } else {
                    date.checked_sub_months(Months::new(1))
                };
                if let Some(date) = candidate.filter(|date| (1900..=9999).contains(&date.year())) {
                    state.selected = Some(date);
                }
            }
            "today" => state.selected = Some(SystemClock.today()),
            "mode" => state.week = !state.week,
            "close" => state.panel = false,
            _ => return,
        }
        state.latest = 0;
        state.data.clear();
        drop(state);
        self.load_calendar();
    }

    fn calendar_select(&self, date: NaiveDate) {
        if !self.calendar_page_visible() {
            return;
        }
        let mut state = self.calendar.borrow_mut();
        let selected = state.selected.unwrap_or(date);
        if !state.week && (selected.year(), selected.month()) != (date.year(), date.month()) {
            return;
        }
        state.selected = Some(date);
        state.panel = true;
        drop(state);
        let state = self.calendar.borrow();
        let days = if state.week {
            calendar_week_days(date, self.settings.borrow().week_start)
        } else {
            calendar_month_days(date, self.settings.borrow().week_start)
        };
        drop(state);
        if let Some(days) = days {
            self.paint_calendar(&days);
        }
    }

    pub(super) fn calendar_reply(
        &self,
        id: u64,
        result: &Result<QueryResult, TaskStoreError>,
    ) -> bool {
        if !self.calendar.borrow_mut().take_reply(id) {
            return false;
        }
        if !self.calendar_page_visible() {
            return true;
        }
        match result {
            Ok(QueryResult::Calendar(data)) => {
                let days = data.iter().map(|(date, _)| *date).collect::<Vec<_>>();
                let mut state = self.calendar.borrow_mut();
                state.data = data.clone();
                state.read_error = false;
                drop(state);
                let _ = self.with_ui(|ui| ui.shell.component().set_calendar_error("".into()));
                self.paint_calendar(&days);
            }
            _ => {
                let mut state = self.calendar.borrow_mut();
                state.data.clear();
                state.read_error = true;
                let days = state.selected.and_then(|date| {
                    let start = self.settings.borrow().week_start;
                    if state.week {
                        calendar_week_days(date, start)
                    } else {
                        calendar_month_days(date, start)
                    }
                });
                drop(state);
                if let Some(days) = days {
                    self.paint_calendar(&days);
                }
                let _ = self.with_ui(|ui| {
                    ui.shell
                        .component()
                        .set_calendar_error("日历读取失败，请切换日期重试。".into())
                });
            }
        }
        true
    }

    fn paint_calendar(&self, days: &[NaiveDate]) {
        let state = self.calendar.borrow();
        let Some(selected) = state.selected else {
            return;
        };
        let today = SystemClock.today();
        let views: Vec<CalendarCellView> = days
            .iter()
            .map(|date| {
                let preview = state
                    .data
                    .iter()
                    .find(|(day, _)| day == date)
                    .map(|(_, agenda)| cell_preview(agenda, today.year()))
                    .unwrap_or_default();
                CalendarCellView {
                    date: date.to_string().into(),
                    label: date.day().to_string().into(),
                    items: ModelRc::new(VecModel::from(preview.items)),
                    remaining: if preview.remaining == 0 {
                        "".into()
                    } else {
                        format!("+{}", preview.remaining).into()
                    },
                    completed: preview.completed_label.into(),
                    in_month: state.week || date.month() == selected.month(),
                    today: *date == today,
                    selected: *date == selected,
                }
            })
            .collect();
        let agenda = state
            .data
            .iter()
            .find(|(date, _)| *date == selected)
            .map(|(_, agenda)| agenda);
        let sections = if state.panel {
            agenda
                .map(|agenda| day_sections(agenda, today.year()))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let panel = panel_message(agenda, state.read_error);
        let weekdays = match self.settings.borrow().week_start {
            WeekStart::Monday => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
            WeekStart::Sunday => ["周日", "周一", "周二", "周三", "周四", "周五", "周六"],
        };
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_calendar_heading(
                format!("{} 年 {} 月", selected.year(), selected.month()).into(),
            );
            window.set_calendar_selected_label(
                format!("{} 月 {} 日", selected.month(), selected.day()).into(),
            );
            window.set_calendar_selected_context(
                format!("{} 年 · {}", selected.year(), weekday_label(selected)).into(),
            );
            window.set_calendar_panel_message(panel.into());
            window.set_calendar_panel_sections(ModelRc::new(VecModel::from(sections)));
            window.set_calendar_week_mode(state.week);
            window.set_calendar_panel_open(state.panel);
            window.set_calendar_days(ModelRc::new(VecModel::from(views)));
            window.set_calendar_weekdays(ModelRc::new(VecModel::from(
                weekdays
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_latest_nonzero_reply_is_accepted_once() {
        let mut state = CalendarSession {
            latest: 12,
            ..CalendarSession::default()
        };
        assert!(!state.take_reply(0));
        assert!(!state.take_reply(11));
        assert_eq!(state.latest, 12);
        assert!(state.take_reply(12));
        assert!(!state.take_reply(12));
        assert!(!state.take_reply(0));
    }

    #[test]
    fn test_release_discards_data_and_replies_but_preserves_light_navigation() {
        let date: NaiveDate = "2026-11-07".parse().unwrap();
        let mut state = CalendarSession {
            selected: Some(date),
            week: true,
            panel: true,
            latest: 12,
            data: vec![(date, DayAgenda::default())],
            read_error: true,
        };
        state.release_data();
        assert_eq!(state.selected, Some(date));
        assert!(state.week && state.panel);
        assert!(state.data.is_empty());
        assert!(!state.read_error);
        assert!(!state.take_reply(12));
    }
}
