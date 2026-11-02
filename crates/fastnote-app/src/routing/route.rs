use fastnote_core::{NaiveDate, NoteId, PlanId, TaskId};
use fastnote_ui::ShellView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    Today,
    Calendar,
    Notes,
    Journal,
    PlanList,
    PlanDetail,
    Settings,
}

impl Page {
    pub(crate) fn from_label(label: &str) -> Option<Self> {
        match label {
            "今日任务" => Some(Self::Today),
            "日历" => Some(Self::Calendar),
            "小记" => Some(Self::Notes),
            "日记" => Some(Self::Journal),
            "计划板" => Some(Self::PlanList),
            "计划详情" => Some(Self::PlanDetail),
            "设置" => Some(Self::Settings),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Today => "今日任务",
            Self::Calendar => "日历",
            Self::Notes => "小记",
            Self::Journal => "日记",
            Self::PlanList => "计划板",
            Self::PlanDetail => "计划详情",
            Self::Settings => "设置",
        }
    }

    pub(crate) fn sidebar_page(self) -> Self {
        match self {
            Self::PlanDetail => Self::PlanList,
            page => page,
        }
    }
}

/// Q-SHELL-01（2026-11-02）：只保留进行中 / 已归档两个 Tab。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PlanTab {
    #[default]
    Active,
    Archived,
}

impl PlanTab {
    pub(crate) fn from_value(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    fn value(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Active => "进行中",
            Self::Archived => "已归档",
        }
    }
}

/// SET-01 §3.1–3.2：四个主分组与三个数据管理子页。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SettingsSection {
    #[default]
    General,
    Hotkeys,
    TaskPreferences,
    About,
    Storage,
    Backup,
    ImportExport,
}

impl SettingsSection {
    pub(crate) fn from_value(value: &str) -> Option<Self> {
        match value {
            "general" => Some(Self::General),
            "hotkeys" => Some(Self::Hotkeys),
            "task-preferences" => Some(Self::TaskPreferences),
            "about" => Some(Self::About),
            "storage" => Some(Self::Storage),
            "backup" => Some(Self::Backup),
            "import-export" => Some(Self::ImportExport),
            _ => None,
        }
    }

    fn value(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Hotkeys => "hotkeys",
            Self::TaskPreferences => "task-preferences",
            Self::About => "about",
            Self::Storage => "storage",
            Self::Backup => "backup",
            Self::ImportExport => "import-export",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::General => "通用与外观",
            Self::Hotkeys => "全局快捷键",
            Self::TaskPreferences => "任务偏好",
            Self::About => "关于",
            Self::Storage => "存储位置",
            Self::Backup => "备份与恢复",
            Self::ImportExport => "导入导出",
        }
    }
}

/// 与 SHELL-01 §4.1 完全对应；实体是否存在由未来业务查询校验。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Route {
    Today,
    Calendar {
        selected: NaiveDate,
    },
    Notes {
        selected: Option<NoteId>,
    },
    Journal {
        date: NaiveDate,
    },
    PlanList {
        tab: PlanTab,
    },
    PlanDetail {
        plan_id: PlanId,
        focus: Option<TaskId>,
    },
    Settings {
        section: SettingsSection,
    },
}

impl Route {
    pub(crate) fn page(self) -> Page {
        match self {
            Self::Today => Page::Today,
            Self::Calendar { .. } => Page::Calendar,
            Self::Notes { .. } => Page::Notes,
            Self::Journal { .. } => Page::Journal,
            Self::PlanList { .. } => Page::PlanList,
            Self::PlanDetail { .. } => Page::PlanDetail,
            Self::Settings { .. } => Page::Settings,
        }
    }

    pub(crate) fn to_action(self) -> String {
        let parameters = match self {
            Self::Today => String::new(),
            Self::Calendar { selected } => format!("?selected={selected}"),
            Self::Notes { selected } => format!(
                "?selected={}",
                selected
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".to_owned())
            ),
            Self::Journal { date } => format!("?date={date}"),
            Self::PlanList { tab } => format!("?tab={}", tab.value()),
            Self::PlanDetail { plan_id, focus } => {
                let focus = focus.map(|id| format!("&focus={id}")).unwrap_or_default();
                format!("?plan_id={plan_id}{focus}")
            }
            Self::Settings { section } => format!("?section={}", section.value()),
        };
        format!("navigate:{}{parameters}", self.page().label())
    }

    pub(crate) fn shell_view(self) -> ShellView {
        let context = match self {
            Self::Today => String::new(),
            Self::Calendar { selected } => format!("选中日期：{selected}"),
            Self::Notes { selected: Some(id) } => format!("选中小记：{id}"),
            Self::Notes { selected: None } => "未选中小记".to_owned(),
            Self::Journal { date } => format!("日记日期：{date}"),
            Self::PlanList { tab } => tab.label().to_owned(),
            Self::PlanDetail {
                plan_id,
                focus: Some(id),
            } => {
                format!("计划 ID：{plan_id} · 聚焦任务：{id}")
            }
            Self::PlanDetail {
                plan_id,
                focus: None,
            } => format!("计划 ID：{plan_id}"),
            Self::Settings { section } => section.label().to_owned(),
        };
        ShellView {
            page_label: self.page().sidebar_page().label(),
            title: self.page().label(),
            context,
            route_debug: self.to_action(),
            can_go_back: matches!(self, Self::Settings { .. } | Self::PlanDetail { .. }),
            is_plan_detail: matches!(self, Self::PlanDetail { .. }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_detail_keeps_plan_board_selected_and_exposes_both_ids() {
        let plan_id = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
        let task_id = "019a0fff-aaaa-7000-8000-000000000002".parse().unwrap();
        let view = Route::PlanDetail {
            plan_id,
            focus: Some(task_id),
        }
        .shell_view();
        assert_eq!(view.page_label, "计划板");
        assert_eq!(view.title, "计划详情");
        assert!(view.can_go_back && view.is_plan_detail);
        assert!(view
            .route_debug
            .contains(&format!("plan_id={plan_id}&focus={task_id}")));
    }

    #[test]
    fn test_only_detail_and_settings_have_a_back_button() {
        assert!(!Route::Today.shell_view().can_go_back);
        assert!(
            !Route::PlanList {
                tab: PlanTab::Archived
            }
            .shell_view()
            .can_go_back
        );
        assert!(
            Route::Settings {
                section: SettingsSection::Backup
            }
            .shell_view()
            .can_go_back
        );
    }

    #[test]
    fn test_every_settings_section_round_trips_its_value() {
        for section in [
            SettingsSection::General,
            SettingsSection::Hotkeys,
            SettingsSection::TaskPreferences,
            SettingsSection::About,
            SettingsSection::Storage,
            SettingsSection::Backup,
            SettingsSection::ImportExport,
        ] {
            assert_eq!(SettingsSection::from_value(section.value()), Some(section));
            assert!(!section.label().is_empty());
        }
    }

    #[test]
    fn test_plan_tab_has_only_active_and_archived() {
        for tab in [PlanTab::Active, PlanTab::Archived] {
            assert_eq!(PlanTab::from_value(tab.value()), Some(tab));
        }
        assert_eq!(PlanTab::from_value("completed"), None);
    }
}
