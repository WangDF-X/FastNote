//! 由 AppController 保存的各页会话参数；不是导航历史栈（SHELL-01 §4.3）。

use fastnote_core::{Clock, NaiveDate, NoteId};

use super::{Page, PlanTab, Route, SettingsSection};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RouteSessions {
    calendar: Option<NaiveDate>,
    notes: Option<NoteId>,
    journal: Option<NaiveDate>,
    plan_tab: PlanTab,
    settings_section: SettingsSection,
}

impl RouteSessions {
    pub(crate) fn remember(&mut self, route: Route) {
        match route {
            Route::Calendar { selected } => self.calendar = Some(selected),
            Route::Notes { selected } => self.notes = selected,
            Route::Journal { date } => self.journal = Some(date),
            Route::PlanList { tab } => self.plan_tab = tab,
            Route::Settings { section } => self.settings_section = section,
            Route::Today | Route::PlanDetail { .. } => {}
        }
    }

    pub(crate) fn for_page(&self, page: Page, clock: &impl Clock) -> Option<Route> {
        match page {
            Page::Today => Some(Route::Today),
            Page::Calendar => Some(Route::Calendar {
                selected: self.calendar.unwrap_or_else(|| clock.today()),
            }),
            Page::Notes => Some(Route::Notes {
                selected: self.notes,
            }),
            Page::Journal => Some(Route::Journal {
                date: self.journal.unwrap_or_else(|| clock.today()),
            }),
            Page::PlanList => Some(self.plan_list()),
            Page::Settings => Some(Route::Settings {
                section: self.settings_section,
            }),
            // 无参数请求不能凭空构造计划 ID。
            Page::PlanDetail => None,
        }
    }

    pub(crate) fn plan_list(&self) -> Route {
        Route::PlanList { tab: self.plan_tab }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedClock;

    impl Clock for FixedClock {
        fn today(&self) -> NaiveDate {
            NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
        }
    }

    #[test]
    fn test_detail_is_not_a_history_entry_and_does_not_reset_list_tab() {
        let mut sessions = RouteSessions::default();
        sessions.remember(Route::PlanList {
            tab: PlanTab::Archived,
        });
        sessions.remember(Route::PlanDetail {
            plan_id: "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap(),
            focus: None,
        });
        assert_eq!(
            sessions.plan_list(),
            Route::PlanList {
                tab: PlanTab::Archived
            }
        );
        assert_eq!(sessions.for_page(Page::PlanDetail, &FixedClock), None);
    }
}
