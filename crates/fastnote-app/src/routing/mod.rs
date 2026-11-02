//! SHELL-01 §4：路由数据及本地动作解析，路由状态仅由 AppController 持有。

mod error;
mod parse;
mod route;
mod session;

pub(crate) use parse::{parse_action, NavigationAction};
pub(crate) use route::{Page, PlanTab, Route, SettingsSection};
pub(crate) use session::RouteSessions;
