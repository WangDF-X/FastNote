#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Today,
    Calendar,
    Notes,
    Journal,
    PlanBoard,
    Settings,
}

impl Route {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "今日任务" => Some(Self::Today),
            "日历" => Some(Self::Calendar),
            "小记" => Some(Self::Notes),
            "日记" => Some(Self::Journal),
            "计划板" => Some(Self::PlanBoard),
            "设置" => Some(Self::Settings),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Today => "今日任务",
            Self::Calendar => "日历",
            Self::Notes => "小记",
            Self::Journal => "日记",
            Self::PlanBoard => "计划板",
            Self::Settings => "设置",
        }
    }
}

pub(crate) struct AppController {
    route: Route,
    settings_return_to: Option<Route>,
}

impl Default for AppController {
    fn default() -> Self {
        Self {
            route: Route::Today,
            settings_return_to: None,
        }
    }
}

impl AppController {
    pub(crate) fn current_page(&self) -> &'static str {
        self.route.label()
    }

    pub(crate) fn handle_action(&mut self, action: &str) -> &'static str {
        if action == "back" {
            self.back_from_settings();
        } else if let Some(label) = action.strip_prefix("navigate:") {
            if let Some(route) = Route::from_label(label) {
                self.navigate(route);
            }
        }

        self.current_page()
    }

    fn navigate(&mut self, destination: Route) {
        match (self.route, destination) {
            (Route::Settings, Route::Settings) => {}
            (_, Route::Settings) => {
                self.settings_return_to = Some(self.route);
                self.route = Route::Settings;
            }
            (_, route) => {
                self.settings_return_to = None;
                self.route = route;
            }
        }
    }

    fn back_from_settings(&mut self) {
        if self.route == Route::Settings {
            self.route = self.settings_return_to.take().unwrap_or(Route::Today);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppController, Route};

    #[test]
    fn test_initial_route_is_today() {
        let controller = AppController::default();

        assert_eq!(controller.current_page(), "今日任务");
    }

    #[test]
    fn test_settings_back_restores_previous_route() {
        let mut controller = AppController::default();
        controller.handle_action("navigate:计划板");
        controller.handle_action("navigate:设置");

        assert_eq!(controller.handle_action("back"), "计划板");
    }

    #[test]
    fn test_navigation_from_settings_replaces_return_route() {
        let mut controller = AppController::default();
        controller.handle_action("navigate:设置");
        controller.handle_action("navigate:日历");
        controller.handle_action("navigate:设置");

        assert_eq!(controller.handle_action("back"), "日历");
    }

    #[test]
    fn test_invalid_navigation_does_not_change_route() {
        let mut controller = AppController::default();

        assert_eq!(controller.handle_action("navigate:未知页面"), "今日任务");
        assert_eq!(controller.route, Route::Today);
    }
}
