//! 日历格子的纯显示投影，不承担任务分类和时间计算。

use fastnote_core::derive::calendar::DayAgenda;
use slint::{ModelRc, VecModel};

use crate::today::row_data;
use crate::{CalendarSectionData, TaskRowData};

/// 日期格最多显示的未完成任务数量。
const MAX_PREVIEWS: usize = 3;

/// 日期格中的任务预览和弱完成计数。
#[derive(Debug, Default, PartialEq)]
pub struct CellPreview {
    /// 最多三条未完成任务摘要，保留真实状态和领域派生标记。
    pub items: Vec<TaskRowData>,
    /// 未显示的未完成任务数量。
    pub remaining: usize,
    /// 已完成数量文本，不含图标；超过九项显示为 9+。
    pub completed_label: String,
}

/// 按领域层已确定的分组顺序投影，完成项不显示标题。
/// 本地当前年份由调用者注入，与今日列表共享日期缩写规则。
pub fn cell_preview(agenda: &DayAgenda, current_year: i32) -> CellPreview {
    let open = agenda
        .due
        .iter()
        .chain(&agenda.start)
        .chain(&agenda.ongoing);
    let count = agenda.due.len() + agenda.start.len() + agenda.ongoing.len();
    let completed_label = match agenda.completed.len() {
        0 => String::new(),
        1..=9 => agenda.completed.len().to_string(),
        _ => "9+".to_owned(),
    };
    CellPreview {
        items: open
            .take(MAX_PREVIEWS)
            .map(|item| row_data(item, current_year))
            .collect(),
        remaining: count.saturating_sub(MAX_PREVIEWS),
        completed_label,
    }
}

/// 将日面板投影为有序分组和独立任务摘要；空分组不生成控件。
/// 只消费已查询的日程，不改变归组、排序、任务状态或存储。
pub fn day_sections(agenda: &DayAgenda, current_year: i32) -> Vec<CalendarSectionData> {
    [
        ("到期", &agenda.due),
        ("开始", &agenda.start),
        ("进行中", &agenda.ongoing),
        ("已完成", &agenda.completed),
    ]
    .into_iter()
    .enumerate()
    .filter(|(_, (_, tasks))| !tasks.is_empty())
    .map(|(kind, (label, tasks))| CalendarSectionData {
        kind: kind as i32,
        label: label.into(),
        total: i32::try_from(tasks.len()).unwrap_or(i32::MAX),
        items: ModelRc::new(VecModel::from(
            tasks
                .iter()
                .map(|task| row_data(task, current_year))
                .collect::<Vec<_>>(),
        )),
    })
    .collect()
}

#[cfg(test)]
mod style_contract;
#[cfg(test)]
mod tests;
