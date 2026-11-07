//! 路由唯一持有者：完整参数导航、会话保留、设置与计划详情的显式返回。

use fastnote_core::Clock;
use fastnote_ui::{ShellUpdate, ShellView};

use crate::routing::{parse_action, NavigationAction, Page, Route, RouteSessions};

pub(crate) struct AppController<C: Clock> {
    clock: C,
    route: Route,
    settings_return_to: Option<Route>,
    sessions: RouteSessions,
}

impl<C: Clock> AppController<C> {
    pub(crate) fn new(clock: C) -> Self {
        Self {
            clock,
            route: Route::Today,
            settings_return_to: None,
            sessions: RouteSessions::default(),
        }
    }

    pub(crate) fn shell_view(&self) -> ShellView {
        self.route.shell_view(self.clock.today())
    }

    pub(crate) fn handle_action(&mut self, action: &str) -> ShellUpdate {
        let previous = self.route;
        let issue = match parse_action(action) {
            Ok(action) => {
                match action {
                    Some(NavigationAction::Back) => self.back(),
                    Some(NavigationAction::Page(page)) => self.navigate_page(page),
                    Some(NavigationAction::Route(route)) => self.navigate(route),
                    None => {}
                }
                None
            }
            Err(error) => Some(error.issue()),
        };
        // SHELL-01 §4.1.1：参数变化也算路由变化；解析失败不修改任何会话状态。
        ShellUpdate {
            view: self.shell_view(),
            route_changed: previous != self.route,
            issue,
        }
    }

    fn navigate_page(&mut self, page: Page) {
        // SHELL-01 §4.3.5：设置页再次点击来源导航项恢复完整来源，
        // 计划详情也属于「计划板」，不能降级成丢失 ID 的计划列表。
        if matches!(self.route, Route::Settings { .. }) {
            if let Some(source) = self.settings_return_to {
                if source.page().sidebar_page() == page {
                    self.navigate(source);
                    return;
                }
            }
        }
        if let Some(route) = self.sessions.for_page(page, &self.clock) {
            self.navigate(route);
        }
    }

    fn navigate(&mut self, destination: Route) {
        if matches!(destination, Route::Settings { .. }) {
            if !matches!(self.route, Route::Settings { .. }) {
                self.settings_return_to = Some(self.route);
            }
        } else {
            self.settings_return_to = None;
        }
        self.route = destination;
        self.sessions.remember(destination);
    }

    fn back(&mut self) {
        let destination = match self.route {
            Route::Settings { .. } => self.settings_return_to.unwrap_or(Route::Today),
            Route::PlanDetail { .. } => self.sessions.plan_list(),
            _ => return,
        };
        self.navigate(destination);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use fastnote_core::NaiveDate;
    use fastnote_ui::NavigationIssue;

    use crate::routing::{PlanTab, SettingsSection};

    use super::*;

    const PLAN: &str = "019a0fff-aaaa-7000-8000-000000000001";
    const TASK: &str = "019a0fff-aaaa-7000-8000-000000000002";
    const NOTE: &str = "019a0fff-aaaa-7000-8000-000000000003";

    struct FixedClock(Cell<NaiveDate>);

    impl Clock for FixedClock {
        fn today(&self) -> NaiveDate {
            self.0.get()
        }
    }

    fn date(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    fn controller() -> AppController<FixedClock> {
        AppController::new(FixedClock(Cell::new(date("2026-10-23"))))
    }

    #[test]
    fn test_initial_route_is_today() {
        let controller = controller();
        assert_eq!(controller.route, Route::Today);
        assert_eq!(controller.shell_view().page_label, "今日任务");
    }

    #[test]
    fn test_settings_back_restores_previous_route() {
        let mut controller = controller();
        controller.handle_action("navigate:计划板");
        controller.handle_action("navigate:设置");
        assert_eq!(controller.handle_action("back").view.page_label, "计划板");
    }

    #[test]
    fn test_navigation_from_settings_replaces_return_route() {
        let mut controller = controller();
        controller.handle_action("navigate:设置");
        controller.handle_action("navigate:日历");
        controller.handle_action("navigate:设置");
        assert_eq!(controller.handle_action("back").view.page_label, "日历");
    }

    #[test]
    fn test_invalid_navigation_does_not_change_route() {
        let mut controller = controller();
        let result = controller.handle_action("navigate:未知页面");
        assert_eq!(result.issue, Some(NavigationIssue::Target));
        assert!(!result.route_changed);
        assert_eq!(controller.route, Route::Today);
    }

    #[test]
    fn test_legacy_navigation_uses_injected_clock_and_defaults() {
        let mut controller = controller();
        for (action, expected) in [
            (
                "navigate:日历",
                Route::Calendar {
                    selected: date("2026-10-23"),
                },
            ),
            ("navigate:小记", Route::Notes { selected: None }),
            (
                "navigate:日记",
                Route::Journal {
                    date: date("2026-10-23"),
                },
            ),
            (
                "navigate:计划板",
                Route::PlanList {
                    tab: PlanTab::Active,
                },
            ),
            (
                "navigate:设置",
                Route::Settings {
                    section: SettingsSection::General,
                },
            ),
        ] {
            assert_eq!(controller.handle_action(action).issue, None);
            assert_eq!(controller.route, expected);
            assert_eq!(controller.shell_view().local_today, date("2026-10-23"));
        }
    }

    #[test]
    fn test_settings_back_restores_calendar_date() {
        let mut controller = controller();
        controller.handle_action("navigate:日历?selected=2025-03-17");
        let source = controller.route;
        controller.handle_action("navigate:设置?section=backup");
        controller.handle_action("back");
        assert_eq!(controller.route, source);
    }

    #[test]
    fn test_settings_sections_keep_original_plan_and_focus() {
        let mut controller = controller();
        controller.handle_action(&format!("navigate:计划详情?plan_id={PLAN}&focus={TASK}"));
        let source = controller.route;
        controller.handle_action("navigate:设置?section=backup");
        controller.handle_action("navigate:设置?section=hotkeys");
        controller.handle_action("navigate:设置");
        controller.handle_action("back");
        assert_eq!(controller.route, source);
    }

    #[test]
    fn test_settings_source_sidebar_restores_plan_detail_not_list() {
        let mut controller = controller();
        controller.handle_action(&format!("navigate:计划详情?plan_id={PLAN}&focus={TASK}"));
        let source = controller.route;
        controller.handle_action("navigate:设置");
        controller.handle_action("navigate:计划板");
        assert_eq!(controller.route, source);
    }

    #[test]
    fn test_explicit_navigation_from_settings_overrides_source() {
        let mut controller = controller();
        controller.handle_action(&format!("navigate:计划详情?plan_id={PLAN}"));
        controller.handle_action("navigate:设置");
        controller.handle_action("navigate:计划板?tab=archived");
        assert_eq!(
            controller.route,
            Route::PlanList {
                tab: PlanTab::Archived
            }
        );
        assert_eq!(controller.settings_return_to, None);
    }

    #[test]
    fn test_plan_detail_back_restores_previous_list_tab() {
        let mut controller = controller();
        controller.handle_action("navigate:计划板?tab=archived");
        controller.handle_action(&format!("navigate:计划详情?plan_id={PLAN}&focus={TASK}"));
        controller.handle_action("back");
        assert_eq!(
            controller.route,
            Route::PlanList {
                tab: PlanTab::Archived
            }
        );
        assert!(!controller.shell_view().can_go_back);
    }

    #[test]
    fn test_route_changed_includes_parameter_changes_on_same_page() {
        let mut controller = controller();
        controller.handle_action("navigate:日历?selected=2026-10-23");
        assert!(
            controller
                .handle_action("navigate:日历?selected=2026-10-24")
                .route_changed
        );
        assert!(
            !controller
                .handle_action("navigate:日历?selected=2026-10-24")
                .route_changed
        );
    }

    #[test]
    fn test_each_page_keeps_its_session_parameters() {
        let mut controller = controller();
        controller.handle_action("navigate:日历?selected=2024-02-29");
        controller.handle_action(&format!("navigate:小记?selected={NOTE}"));
        controller.handle_action("navigate:日记?date=2025-03-17");
        controller.handle_action("navigate:计划板?tab=archived");
        controller.handle_action("navigate:设置?section=import-export");
        controller.handle_action("navigate:今日任务");
        for (action, expected) in [
            (
                "navigate:日历",
                Route::Calendar {
                    selected: date("2024-02-29"),
                },
            ),
            (
                "navigate:小记",
                Route::Notes {
                    selected: Some(NOTE.parse().unwrap()),
                },
            ),
            (
                "navigate:日记",
                Route::Journal {
                    date: date("2025-03-17"),
                },
            ),
            (
                "navigate:计划板",
                Route::PlanList {
                    tab: PlanTab::Archived,
                },
            ),
            (
                "navigate:设置",
                Route::Settings {
                    section: SettingsSection::ImportExport,
                },
            ),
        ] {
            controller.handle_action(action);
            assert_eq!(controller.route, expected);
        }
    }

    #[test]
    fn test_invalid_parameters_preserve_route_source_and_sessions() {
        let mut controller = controller();
        controller.handle_action("navigate:日历?selected=2025-03-17");
        controller.handle_action("navigate:设置?section=backup");
        let before = (
            controller.route,
            controller.settings_return_to,
            controller.sessions,
        );
        for action in [
            "navigate:日历?selected=2026-02-29",
            "navigate:小记?selected=invalid",
            "navigate:计划详情?focus=none",
            "navigate:计划板?tab=completed",
            "navigate:设置?section=invalid",
        ] {
            let result = controller.handle_action(action);
            assert!(result.issue.is_some());
            assert!(!result.route_changed);
            assert_eq!(
                (
                    controller.route,
                    controller.settings_return_to,
                    controller.sessions
                ),
                before
            );
        }
        controller.handle_action("back");
        assert_eq!(
            controller.route,
            Route::Calendar {
                selected: date("2025-03-17")
            }
        );
    }

    #[test]
    fn test_first_visit_reads_current_clock_but_cached_date_is_kept() {
        let mut controller = controller();
        controller.clock.0.set(date("2026-10-24"));
        controller.handle_action("navigate:日历");
        assert_eq!(
            controller.route,
            Route::Calendar {
                selected: date("2026-10-24")
            }
        );
        controller.handle_action("navigate:今日任务");
        controller.clock.0.set(date("2026-10-25"));
        controller.handle_action("navigate:日历");
        assert_eq!(
            controller.route,
            Route::Calendar {
                selected: date("2026-10-24")
            }
        );
        controller.handle_action("navigate:日记");
        assert_eq!(
            controller.route,
            Route::Journal {
                date: date("2026-10-25")
            }
        );
        assert_eq!(controller.shell_view().local_today, date("2026-10-25"));
    }

    #[test]
    fn test_cleared_note_selection_is_remembered() {
        let mut controller = controller();
        controller.handle_action(&format!("navigate:小记?selected={NOTE}"));
        controller.handle_action("navigate:小记?selected=none");
        controller.handle_action("navigate:今日任务");
        controller.handle_action("navigate:小记");
        assert_eq!(controller.route, Route::Notes { selected: None });
    }

    #[test]
    fn test_back_is_not_a_general_history_stack() {
        let mut controller = controller();
        controller.handle_action("navigate:日历");
        controller.handle_action("navigate:小记");
        assert!(!controller.handle_action("back").route_changed);
        assert_eq!(controller.route, Route::Notes { selected: None });
        assert!(!controller.handle_action("create-task").route_changed);
    }
}
