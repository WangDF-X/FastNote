use std::rc::Rc;

use fastnote_core::{
    Datelike, TaskDetail, TaskError, TaskStatus, TaskSummary, TodaySectionKind, TodayView,
};
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::{Date, MainWindow, TaskRowData, TodaySectionData};

use super::calendar;
use super::summary::{date_label, display_date, schedule_label};

/// 保留每组模型实例，刷新数据时不重建整页或文本编辑器。
pub struct TodayPresenter {
    sections: Rc<VecModel<TodaySectionData>>,
    rows: [Rc<VecModel<TaskRowData>>; 4],
}

impl TodayPresenter {
    /// 在 UI 线程为主窗口安装空模型。
    pub fn new(window: &MainWindow) -> Self {
        let sections = Rc::new(VecModel::default());
        window.set_today_sections(ModelRc::from(sections.clone()));
        window.set_task_hours(ModelRc::new(VecModel::from(time_labels(24))));
        window.set_task_minutes(ModelRc::new(VecModel::from(time_labels(60))));
        window.set_task_years(ModelRc::new(VecModel::from(year_labels())));
        window.on_task_calendar_month(|year, month, today| {
            ModelRc::new(VecModel::from(calendar::month_days(year, month, today)))
        });
        window.on_task_calendar_day(calendar::move_day);
        window.on_task_calendar_month_move(calendar::move_month);
        window.on_task_calendar_select_month(calendar::select_month);
        window.on_task_date_label(|date, current_year| {
            display_date(date.as_str(), current_year).into()
        });
        Self {
            sections,
            rows: std::array::from_fn(|_| Rc::new(VecModel::default())),
        }
    }

    /// 页面离开/主窗隐藏时解除绑定，并替换 Vec 分配以释放行/标题/关联名的缓存。
    pub fn release(&self, window: &MainWindow) {
        window.set_today_mounted(false);
        window.set_today_sections(ModelRc::default());
        self.release_models();
    }

    fn release_models(&self) {
        self.sections.set_vec(Vec::new());
        for rows in &self.rows {
            rows.set_vec(Vec::new());
        }
    }

    /// 应用已查询的只读快照与会话折叠状态；不修改业务数据。
    pub fn apply(&self, window: &MainWindow, view: &TodayView, collapsed: [Option<bool>; 4]) {
        let current_year = view.date.year();
        window.set_task_picker_today(Date {
            year: current_year,
            month: view.date.month() as i32,
            day: view.date.day() as i32,
        });
        let mut sections = Vec::new();
        let mut present = [false; 4];
        for section in &view.sections {
            let index = section.kind as usize;
            present[index] = true;
            sync_model(
                &self.rows[index],
                section
                    .items
                    .iter()
                    .map(|task| row_data(task, current_year))
                    .collect(),
            );
            let remaining = section.total.saturating_sub(section.items.len() as u32);
            let can_load_more = section.has_more && section.items.len() < 1000;
            sections.push(TodaySectionData {
                kind: index as i32,
                label: section_label(section.kind).into(),
                total: number(section.total),
                collapsed: collapsed[index].unwrap_or(section.collapsed_default),
                items: ModelRc::from(self.rows[index].clone()),
                has_more: section.has_more,
                can_load_more,
                more_label: if can_load_more {
                    format!("显示更多 · 还有 {remaining} 项")
                } else {
                    format!("已显示 1,000 项 · 还有 {remaining} 项未展开")
                }
                .into(),
            });
        }
        for (index, rows) in self.rows.iter().enumerate() {
            if !present[index] {
                rows.set_vec(Vec::new());
            }
        }
        sync_model(&self.sections, sections);
        let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            [view.date.weekday().num_days_from_monday() as usize];
        window.set_today_date(
            format!("{}月{}日  {}", view.date.month(), view.date.day(), weekday).into(),
        );
        window.set_today_open_count(number(view.open_count));
        window.set_today_overdue_count(number(view.overdue_count));
        window.set_today_completed_count(number(view.completed_today_count));
    }
}

fn number(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn time_labels(count: u32) -> Vec<SharedString> {
    (0..count)
        .map(|value| format!("{value:02}").into())
        .collect()
}

fn year_labels() -> Vec<SharedString> {
    (calendar::PICKER_FIRST_YEAR..=calendar::PICKER_LAST_YEAR)
        .map(|year| format!("{year}年").into())
        .collect()
}

fn sync_model<T: Clone + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
    for (index, row) in rows.into_iter().enumerate() {
        if index == model.row_count() {
            model.push(row);
        } else {
            model.set_row_data(index, row);
        }
    }
}

/// 下拉选择器的状态索引；仅为显示映射。
pub fn task_status_index(status: Option<TaskStatus>) -> i32 {
    match status {
        Some(TaskStatus::Todo) | None => 0,
        Some(TaskStatus::InProgress) => 1,
        Some(TaskStatus::Done) => 2,
    }
}

/// 今日列表与日历共用的状态、日期及关联信息投影，不重新判定派生标记。
pub(crate) fn row_data(task: &TaskSummary, current_year: i32) -> TaskRowData {
    let mut metadata = Vec::new();
    let dates = schedule_label(
        task.start_date,
        task.start_time,
        task.due_date,
        task.due_time,
        current_year,
    );
    metadata.push(if dates.is_empty() {
        "未设日期".to_owned()
    } else {
        dates
    });
    if task.is_overdue {
        metadata.push("已逾期".to_owned());
    }
    if let Some(plan) = &task.context.plan_name {
        metadata.push(plan.clone());
    }
    if let Some(stage) = &task.context.stage_name {
        metadata.push(stage.clone());
    }
    if task.is_blocked {
        metadata.push("前置待完成".to_owned());
    }
    if task.context.child_total > 0 {
        metadata.push(format!(
            "子任务 {}/{}",
            task.context.child_done, task.context.child_total
        ));
    }
    TaskRowData {
        id: task.id.to_string().into(),
        title: task.title.clone().into(),
        status_index: task_status_index(task.status),
        status_label: match task.status {
            Some(TaskStatus::InProgress) => "进行中",
            Some(TaskStatus::Done) => "已完成",
            _ => "未开始",
        }
        .into(),
        done: task.status == Some(TaskStatus::Done),
        priority: task.priority as i32,
        priority_label: match task.priority as i32 {
            3 => "高",
            2 => "中",
            1 => "低",
            _ => "—",
        }
        .into(),
        metadata: metadata.join(" · ").into(),
        today_context: task
            .context
            .plan_name
            .as_ref()
            .map(|name| format!("所属计划：{name}"))
            .unwrap_or_default()
            .into(),
        blocking_label: if task.context.blocking_count > 0 {
            format!("{} 个前置任务未完成", task.context.blocking_count)
        } else {
            String::new()
        }
        .into(),
        blocking_tooltip: {
            let mut names = task.context.blocking_titles.clone();
            let remaining = (task.context.blocking_count as usize).saturating_sub(names.len());
            if remaining > 0 {
                names.push(format!("还有 {remaining} 项"));
            }
            names.join("\n").into()
        },
        start_label: task
            .start_date
            .map(|date| date_label(date, task.start_time, current_year).replace('-', "/"))
            .unwrap_or_default()
            .into(),
        due_label: task
            .due_date
            .map(|date| date_label(date, task.due_time, current_year).replace('-', "/"))
            .unwrap_or_default()
            .into(),
        overdue: task.is_overdue,
        read_only: task.context.read_only,
    }
}

fn section_label(kind: TodaySectionKind) -> &'static str {
    match kind {
        TodaySectionKind::Focus => "当前关注",
        TodaySectionKind::Unscheduled => "待安排",
        TodaySectionKind::Later => "稍后",
        TodaySectionKind::CompletedToday => "今日完成",
    }
}

/// 共享详情的已有计划/子任务/依赖说明，不呈现未实现的选择器。
pub fn task_context_label(detail: &TaskDetail) -> String {
    let mut parts = vec![format!(
        "所属计划：{}",
        detail.context.plan_name.as_deref().unwrap_or("独立任务")
    )];
    if detail.context.child_total > 0 {
        parts.push(format!(
            "子任务：{}/{} 已完成",
            detail.context.child_done, detail.context.child_total
        ));
    }
    if detail.context.blocking_count > 0 {
        parts.push(format!(
            "还有 {} 个前置任务未完成",
            detail.context.blocking_count
        ));
    }
    parts.join("\n")
}

/// 只展示稳定中文文案，不能把领域/存储源错误原文直接给 UI。
pub fn task_error_message(error: TaskError) -> &'static str {
    match error {
        TaskError::TitleRequired => "请输入任务标题。",
        TaskError::TitleTooLong => "标题最多 200 个字符，输入已保留。",
        TaskError::DescriptionTooLong => "描述最多 10,000 个字符，输入已保留。",
        TaskError::DateRange => "开始日期和时间不能晚于到期，请重新选择有效的日期和时分。",
        TaskError::NotFound => "任务已不存在，请刷新。",
        TaskError::Conflict => "内容已变化或保存发生冲突，请重新加载后重试。",
        TaskError::PlanArchived => "计划已归档，仅供查看。",
        TaskError::StageParentInvalid => "阶段不能作为普通任务编辑。",
        TaskError::HasChildren => "该任务包含子任务，请重新确认删除范围。",
    }
}

#[cfg(test)]
mod residency_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use fastnote_core::{Task, TaskContext};

    #[test]
    fn test_row_uses_core_flags_and_does_not_infer_overdue_from_its_date() {
        let task = Task::new(
            "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap(),
            "假任务".to_owned(),
            "2026-11-03T12:00:00Z".parse().unwrap(),
        )
        .unwrap();
        let mut summary = TaskSummary::from_task(&task, TaskContext::default());
        assert_eq!(row_data(&summary, 2026).title.as_str(), "假任务");
        assert_eq!(row_data(&summary, 2026).metadata.as_str(), "未设日期");
        summary.due_date = Some("2020-01-01".parse().unwrap());
        assert!(!row_data(&summary, 2026).overdue);
        summary.is_overdue = true;
        assert!(row_data(&summary, 2026).overdue);
        assert!(row_data(&summary, 2026).metadata.contains("已逾期"));
    }

    #[test]
    fn test_all_domain_errors_have_readable_chinese_without_input_content() {
        for error in [
            TaskError::TitleRequired,
            TaskError::TitleTooLong,
            TaskError::DescriptionTooLong,
            TaskError::DateRange,
            TaskError::NotFound,
            TaskError::Conflict,
            TaskError::PlanArchived,
            TaskError::StageParentInvalid,
            TaskError::HasChildren,
        ] {
            assert!(!task_error_message(error).is_empty());
            assert!(!task_error_message(error).contains("E_"));
        }
    }

    #[test]
    fn test_model_updates_preserve_titles_and_date_metadata() {
        let task = Task::new(
            "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap(),
            "准备中文测试任务".to_owned(),
            "2026-11-03T12:00:00Z".parse().unwrap(),
        )
        .unwrap();
        let mut summary = TaskSummary::from_task(&task, TaskContext::default());
        let model = VecModel::default();
        sync_model(&model, vec![row_data(&summary, 2026)]);
        assert_eq!(
            model.row_data(0).unwrap().title.as_str(),
            "准备中文测试任务"
        );
        summary.title = "修改后的中文任务".to_owned();
        summary.due_date = Some("2026-11-04".parse().unwrap());
        summary.due_time = fastnote_core::parse_task_time("15:31").unwrap();
        sync_model(&model, vec![row_data(&summary, 2026)]);
        let row = model.row_data(0).unwrap();
        assert_eq!(row.title.as_str(), "修改后的中文任务");
        assert_eq!(row.metadata.as_str(), "11-04 15:31");
    }

    #[test]
    fn test_concise_date_range_keeps_plan_context() {
        let task = Task::new(
            "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap(),
            "假任务".to_owned(),
            "2026-11-03T12:00:00Z".parse().unwrap(),
        )
        .unwrap();
        let mut summary = TaskSummary::from_task(&task, TaskContext::default());
        summary.start_date = Some("2026-11-03".parse().unwrap());
        summary.start_time = fastnote_core::parse_task_time("09:05").unwrap();
        summary.due_date = Some("2026-11-04".parse().unwrap());
        summary.due_time = fastnote_core::parse_task_time("15:31").unwrap();
        summary.context.plan_name = Some("假计划".to_owned());
        assert_eq!(
            row_data(&summary, 2026).metadata.as_str(),
            "11-03 09:05 → 11-04 15:31 · 假计划"
        );
        let row = row_data(&summary, 2026);
        assert_eq!(row.start_label, "11/03 09:05");
        assert_eq!(row.due_label, "11/04 15:31");
        assert_eq!(row.today_context, "所属计划：假计划");
        summary.start_date = None;
        assert_eq!(row_data(&summary, 2026).start_label, "");
        summary.context.blocking_count = 12;
        summary.context.blocking_titles = (1..=10).map(|n| format!("任务 {n}")).collect();
        let row = row_data(&summary, 2026);
        assert_eq!(row.blocking_label, "12 个前置任务未完成");
        assert!(row.blocking_tooltip.ends_with("还有 2 项"));
    }

    #[test]
    fn test_time_dropdown_models_include_every_hour_and_minute_without_manual_input() {
        let hours = time_labels(24);
        let minutes = time_labels(60);
        assert_eq!(hours.len(), 24);
        assert_eq!(hours[0].as_str(), "00");
        assert_eq!(hours[23].as_str(), "23");
        assert_eq!(minutes.len(), 60);
        assert_eq!(minutes[0].as_str(), "00");
        assert_eq!(minutes[59].as_str(), "59");
        for (index, label) in minutes.iter().enumerate() {
            assert_eq!(label.as_str(), format!("{index:02}"));
        }
    }

    #[test]
    fn test_year_menu_covers_2000_through_2100_inclusively() {
        let years = year_labels();
        assert_eq!(years.len(), 101);
        for (index, label) in years.iter().enumerate() {
            assert_eq!(label.as_str(), format!("{}年", 2000 + index));
        }
    }
}
