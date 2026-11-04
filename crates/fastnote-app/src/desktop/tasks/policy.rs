use crate::tasks::{DetailPurpose, TaskSession};

/// 页面操作与已确认业务共用内核可用性，主窗和模态限制由页面入口另行检查。
pub(super) fn kernel_available(ready: bool, finished: bool, closing: bool, busy: bool) -> bool {
    ready && !finished && !closing && !busy
}

/// SHELL-03 R3：无会话只允许保存基线/删除确认的必要读取，不允许页面预取。
pub(super) fn may_query(purpose: Option<DetailPurpose>, has_ui: bool) -> bool {
    has_ui
        || matches!(
            purpose,
            Some(DetailPurpose::SaveAck(_) | DetailPurpose::Delete(_))
        )
}

/// 复用既有在途查询；不新增会话代次，也不把不同目的或不同任务合并。
pub(super) fn detail_query_pending(tasks: &TaskSession, purpose: DetailPurpose) -> bool {
    tasks.queries.values().any(|pending| {
        matches!((purpose, pending),
            (DetailPurpose::SaveAck(a), DetailPurpose::SaveAck(b))
                | (DetailPurpose::Refresh(a), DetailPurpose::Refresh(b)) if a == *b)
    })
}

/// 保存确认读取失败只标记匹配编辑器，不能清除已提交/在途保护或影响其它输入。
pub(super) fn save_read_failed(tasks: &mut TaskSession, purpose: DetailPurpose, message: &str) {
    if let DetailPurpose::SaveAck(id) = purpose {
        if let Some(editor) = tasks
            .editor
            .as_mut()
            .filter(|editor| editor.detail.task.id == id)
        {
            editor.error = Some(message.to_owned());
        }
    }
}

/// 焦点仅属于已显示、未最小化的今日页面；缺失原生窗口时不能调用焦点回调。
pub(super) fn may_focus_tasks(
    main_visible: bool,
    has_native_window: bool,
    minimized: Option<bool>,
    page: &str,
    dialog: bool,
) -> bool {
    main_visible && has_native_window && minimized != Some(true) && page == "今日任务" && !dialog
}

/// 先登记内核确认再呈现；回调期间不持有状态借用，拒绝时恢复已有决策。
pub(super) fn stage_confirmation<T>(
    stage: impl FnOnce() -> T,
    present: impl FnOnce() -> bool,
    rollback: impl FnOnce(T),
) -> bool {
    let previous = stage();
    if present() {
        true
    } else {
        rollback(previous);
        false
    }
}

/// 仅消费匹配且已接受的确认；执行回调不借用状态，队列拒绝时保留原决策供重试。
pub(super) fn resolve_confirmation<T>(
    take: impl FnOnce() -> Option<T>,
    execute: impl FnOnce(&T) -> bool,
    restore: impl FnOnce(T),
) -> bool {
    let Some(confirmation) = take() else {
        return false;
    };
    if execute(&confirmation) {
        true
    } else {
        restore(confirmation);
        false
    }
}

#[cfg(test)]
mod tests;
