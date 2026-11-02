use std::str::FromStr;

use chrono::Datelike;
use fastnote_core::{IdError, NaiveDate};

use super::error::RouteError;
use super::route::{Page, PlanTab, Route, SettingsSection};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NavigationAction {
    Back,
    /// 无参数导航恢复会话状态，不直接覆盖为默认日期或选中项。
    Page(Page),
    /// 参数导航携带完整目标；必须全部校验通过后才能修改控制器。
    Route(Route),
}

pub(crate) fn parse_action(action: &str) -> Result<Option<NavigationAction>, RouteError> {
    if action == "back" {
        return Ok(Some(NavigationAction::Back));
    }
    let Some(destination) = action.strip_prefix("navigate:") else {
        // 其它业务动作留给命令层，不将它们误判为导航。
        return Ok(None);
    };
    let (label, query) = match destination.split_once('?') {
        Some((label, query)) => (label, Some(query)),
        None => (destination, None),
    };
    let page = Page::from_label(label).ok_or(RouteError::Target)?;
    let Some(query) = query else {
        if page == Page::PlanDetail {
            return Err(RouteError::MissingParameter("plan_id"));
        }
        return Ok(Some(NavigationAction::Page(page)));
    };

    let route = match page {
        Page::Today => return Err(RouteError::UnexpectedParameter),
        Page::Calendar => {
            let parameters = Parameters::parse(query, &["selected"])?;
            Route::Calendar {
                selected: parse_date(parameters.required("selected")?)?,
            }
        }
        Page::Notes => {
            let parameters = Parameters::parse(query, &["selected"])?;
            Route::Notes {
                selected: optional_id(parameters.required("selected")?)?,
            }
        }
        Page::Journal => {
            let parameters = Parameters::parse(query, &["date"])?;
            Route::Journal {
                date: parse_date(parameters.required("date")?)?,
            }
        }
        Page::PlanList => {
            let parameters = Parameters::parse(query, &["tab"])?;
            let tab = PlanTab::from_value(parameters.required("tab")?).ok_or(RouteError::Tab)?;
            Route::PlanList { tab }
        }
        Page::PlanDetail => {
            let parameters = Parameters::parse(query, &["plan_id", "focus"])?;
            let plan_id = parameters
                .required("plan_id")?
                .parse()
                .map_err(RouteError::Id)?;
            let focus = parameters
                .get("focus")
                .map(optional_id)
                .transpose()?
                .flatten();
            Route::PlanDetail { plan_id, focus }
        }
        Page::Settings => {
            let parameters = Parameters::parse(query, &["section"])?;
            let section = SettingsSection::from_value(parameters.required("section")?)
                .ok_or(RouteError::Section)?;
            Route::Settings { section }
        }
    };
    Ok(Some(NavigationAction::Route(route)))
}

struct Parameters<'a>(Vec<(&'a str, &'a str)>);

impl<'a> Parameters<'a> {
    fn parse(query: &'a str, allowed: &[&str]) -> Result<Self, RouteError> {
        let mut values = Vec::new();
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or(RouteError::Parameters)?;
            if key.is_empty() || value.is_empty() || value.contains('=') {
                return Err(RouteError::Parameters);
            }
            if !allowed.contains(&key) {
                return Err(RouteError::UnexpectedParameter);
            }
            if values.iter().any(|(seen, _)| *seen == key) {
                return Err(RouteError::DuplicateParameter);
            }
            values.push((key, value));
        }
        Ok(Self(values))
    }

    fn get(&self, key: &str) -> Option<&'a str> {
        self.0
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }

    fn required(&self, key: &'static str) -> Result<&'a str, RouteError> {
        self.get(key).ok_or(RouteError::MissingParameter(key))
    }
}

fn parse_date(value: &str) -> Result<NaiveDate, RouteError> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(RouteError::DateFormat)?;
    // ARCH-03 §6.8/§6.10：格式必须为 YYYY-MM-DD，日期不能越界。
    if !(1900..=9999).contains(&date.year()) || date.to_string() != value {
        return Err(RouteError::DateRange);
    }
    Ok(date)
}

fn optional_id<T: FromStr<Err = IdError>>(value: &str) -> Result<Option<T>, RouteError> {
    if value == "none" {
        Ok(None)
    } else {
        value.parse().map(Some).map_err(RouteError::Id)
    }
}

#[cfg(test)]
mod tests {
    use fastnote_core::{NoteId, PlanId, TaskId};
    use fastnote_ui::NavigationIssue;

    use super::*;

    const PLAN: &str = "019a0fff-aaaa-7000-8000-000000000001";
    const TASK: &str = "019a0fff-aaaa-7000-8000-000000000002";
    const NOTE: &str = "019a0fff-aaaa-7000-8000-000000000003";

    #[test]
    fn test_legacy_navigation_and_back_are_supported() {
        for page in [
            Page::Today,
            Page::Calendar,
            Page::Notes,
            Page::Journal,
            Page::PlanList,
            Page::Settings,
        ] {
            assert_eq!(
                parse_action(&format!("navigate:{}", page.label())).unwrap(),
                Some(NavigationAction::Page(page))
            );
        }
        assert_eq!(parse_action("back").unwrap(), Some(NavigationAction::Back));
        assert_eq!(parse_action("create-task").unwrap(), None);
    }

    #[test]
    fn test_parameter_routes_and_canonical_actions_round_trip() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let plan_id: PlanId = PLAN.parse().unwrap();
        let task_id: TaskId = TASK.parse().unwrap();
        let note_id: NoteId = NOTE.parse().unwrap();
        let routes = [
            Route::Calendar { selected: date },
            Route::Journal { date },
            Route::Notes {
                selected: Some(note_id),
            },
            Route::Notes { selected: None },
            Route::PlanList {
                tab: PlanTab::Active,
            },
            Route::PlanList {
                tab: PlanTab::Archived,
            },
            Route::PlanDetail {
                plan_id,
                focus: Some(task_id),
            },
            Route::PlanDetail {
                plan_id,
                focus: None,
            },
            Route::Settings {
                section: SettingsSection::General,
            },
            Route::Settings {
                section: SettingsSection::Hotkeys,
            },
            Route::Settings {
                section: SettingsSection::TaskPreferences,
            },
            Route::Settings {
                section: SettingsSection::About,
            },
            Route::Settings {
                section: SettingsSection::Storage,
            },
            Route::Settings {
                section: SettingsSection::Backup,
            },
            Route::Settings {
                section: SettingsSection::ImportExport,
            },
        ];
        for route in routes {
            assert_eq!(
                parse_action(&route.to_action()).unwrap(),
                Some(NavigationAction::Route(route))
            );
        }
    }

    #[test]
    fn test_plan_focus_can_be_cleared_and_parameter_order_is_independent() {
        let route = Route::PlanDetail {
            plan_id: PLAN.parse().unwrap(),
            focus: None,
        };
        let action = format!("navigate:计划详情?focus=none&plan_id={PLAN}");
        assert_eq!(
            parse_action(&action).unwrap(),
            Some(NavigationAction::Route(route))
        );
    }

    #[test]
    fn test_unknown_target_has_its_own_error_code() {
        assert_eq!(
            parse_action("navigate:未知页面").unwrap_err().issue(),
            NavigationIssue::Target
        );
    }

    #[test]
    fn test_unknown_duplicate_and_malformed_parameters_are_rejected() {
        for action in [
            "navigate:日历?",
            "navigate:日历?selected",
            "navigate:日历?selected=",
            "navigate:日历?=2026-10-23",
            "navigate:日历?selected=2026-10-23=extra",
            "navigate:日历?selected=2026-10-23&",
            "navigate:日历?unknown=value",
            "navigate:日历?selected=2026-10-23&selected=2026-10-24",
            "navigate:今日任务?selected=2026-10-23",
            "navigate:设置?tab=active",
        ] {
            assert_eq!(
                parse_action(action).unwrap_err().issue(),
                NavigationIssue::Parameters,
                "{action}"
            );
        }
    }

    #[test]
    fn test_plan_detail_requires_plan_id() {
        for action in [
            "navigate:计划详情".to_owned(),
            format!("navigate:计划详情?focus={TASK}"),
        ] {
            assert!(matches!(
                parse_action(&action),
                Err(RouteError::MissingParameter("plan_id"))
            ));
        }
    }

    #[test]
    fn test_invalid_enum_values_do_not_fall_back_to_defaults() {
        for action in [
            "navigate:计划板?tab=completed",
            "navigate:设置?section=unknown",
        ] {
            assert_eq!(
                parse_action(action).unwrap_err().issue(),
                NavigationIssue::Parameters
            );
        }
    }

    #[test]
    fn test_dates_accept_bounds_and_leap_days() {
        for date in ["1900-01-01", "2024-02-29", "9999-12-31"] {
            assert!(parse_action(&format!("navigate:日历?selected={date}")).is_ok());
        }
    }

    #[test]
    fn test_invalid_and_noncanonical_dates_are_rejected() {
        for date in [
            "1899-12-31",
            "10000-01-01",
            "2026-02-29",
            "2026-2-03",
            "2026/10/23",
            "invalid",
        ] {
            let error = parse_action(&format!("navigate:日记?date={date}")).unwrap_err();
            assert_eq!(error.issue(), NavigationIssue::Date, "{date}");
        }
    }

    #[test]
    fn test_invalid_ids_are_rejected_for_each_parameter() {
        for action in [
            "navigate:小记?selected=not-an-id".to_owned(),
            "navigate:小记?selected=00000000-0000-0000-0000-000000000000".to_owned(),
            "navigate:计划详情?plan_id=550e8400-e29b-41d4-a716-446655440000".to_owned(),
            format!("navigate:计划详情?plan_id={PLAN}&focus=invalid"),
        ] {
            assert_eq!(
                parse_action(&action).unwrap_err().issue(),
                NavigationIssue::Id
            );
        }
    }

    #[test]
    fn test_id_and_date_parse_errors_keep_their_source() {
        use std::error::Error;

        let id_error = parse_action("navigate:小记?selected=invalid").unwrap_err();
        let date_error = parse_action("navigate:日历?selected=invalid").unwrap_err();
        assert!(id_error.source().is_some());
        assert!(date_error.source().is_some());
    }
}
