use fastnote_core::{AppEvent, NaiveDateTime, TaskDetail, TaskId, TaskScope, TodayView};

use crate::tasks::{today_clock_stale, DetailPurpose, TaskSession};

pub(super) use super::super::policy::stage_confirmation;

/// 仅描述已提交事件使哪些已实现读模型失效，不持有 UI 或正文。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Invalidations {
    pub today: bool,
    pub detail: bool,
}

/// ARCH-02：All 同时影响今日与当前详情，其它模块事件不伪称已经刷新。
pub(super) fn invalidations(events: &[AppEvent], selected: Option<TaskId>) -> Invalidations {
    let mut result = Invalidations::default();
    for event in events {
        if let AppEvent::TasksChanged { scope, .. } = event {
            match scope {
                TaskScope::All => {
                    result.today = true;
                    result.detail |= selected.is_some();
                }
                TaskScope::Today => result.today = true,
                TaskScope::Detail(id) if Some(*id) == selected => result.detail = true,
                _ => {}
            }
        }
    }
    result
}

/// 无可见页只失效，不能因事件为页面发起后台预取。
pub(super) fn should_refresh(invalidated: bool, page_visible: bool) -> bool {
    invalidated && page_visible
}

/// 迟到列表读结果不能在无主窗时填回大型页面快照；既有请求 ID 检查不变。
pub(super) fn adopt_today(
    tasks: &mut TaskSession,
    id: u64,
    view: TodayView,
    page_visible: bool,
) -> bool {
    if id == 0 || id != tasks.latest_today || !page_visible {
        return false;
    }
    tasks.today = Some(view);
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TodayCompletion {
    pub applied: bool,
    pub requery: bool,
}

/// 当前请求成功/失败/不可见都释放在途状态；期间再失效的快照不能当作最新数据。
pub(super) fn complete_today(
    tasks: &mut TaskSession,
    id: u64,
    view: Option<TodayView>,
    page_visible: bool,
    now: NaiveDateTime,
) -> Option<TodayCompletion> {
    if id == 0 || tasks.latest_today != id {
        return None;
    }
    if view
        .as_ref()
        .is_some_and(|view| today_clock_stale(view, now))
    {
        tasks.invalidate_today_read();
    }
    let can_apply = page_visible && !tasks.today_requery;
    let applied = view.is_some_and(|view| adopt_today(tasks, id, view, can_apply));
    let requery = tasks.finish_today_read(id)?;
    Some(TodayCompletion { applied, requery })
}

/// 保存确认/删除确认仍属内核闭环；普通打开/刷新读结果仅为可见页面使用。
pub(super) fn may_apply_detail(purpose: DetailPurpose, page_visible: bool) -> bool {
    page_visible
        || matches!(
            purpose,
            DetailPurpose::SaveAck(_) | DetailPurpose::Delete(_)
        )
}

/// 只确认当前编辑器真实在途保存，旧任务回包不能改变另一份输入状态。
pub(super) fn mark_save_committed(tasks: &mut TaskSession, id: TaskId) -> bool {
    let Some(editor) = tasks
        .editor
        .as_mut()
        .filter(|editor| editor.detail.task.id == id && editor.in_flight.is_some())
    else {
        return false;
    };
    editor.committed = true;
    true
}

/// 保存失败仍在内核保留输入；不依赖主窗投影。
pub(super) fn mark_save_failed(tasks: &mut TaskSession, id: TaskId, message: String) -> bool {
    let Some(editor) = tasks
        .editor
        .as_mut()
        .filter(|editor| editor.detail.task.id == id)
    else {
        return false;
    };
    editor.failed(message);
    true
}

/// 保存确认读回包更新真实基线，EditorSession 自行保留更新版本的输入。
pub(super) fn acknowledge_save(tasks: &mut TaskSession, id: TaskId, detail: TaskDetail) -> bool {
    tasks
        .editor
        .as_mut()
        .filter(|editor| editor.detail.task.id == id)
        .is_some_and(|editor| editor.acknowledge(detail))
}

#[cfg(test)]
mod loading_tests;
#[cfg(test)]
mod tests;
