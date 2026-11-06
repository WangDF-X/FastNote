use std::cell::Cell;

use fastnote_core::TaskId;

use crate::tasks::{DetailPurpose, TaskSession};

/// 属性投影期间禁止失焦保存；嵌套投影和提前返回都恢复调用前状态。
pub(in crate::desktop) fn while_syncing<R>(syncing: &Cell<bool>, project: impl FnOnce() -> R) -> R {
    struct Restore<'a> {
        syncing: &'a Cell<bool>,
        previous: bool,
    }
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            self.syncing.set(self.previous);
        }
    }
    let _restore = Restore {
        syncing,
        previous: syncing.replace(true),
    };
    project()
}

/// 调用者已保存或明确放弃后关闭编辑器；保留保存确认读取，不受 UI 是否存在影响。
pub(super) fn close_editor(tasks: &mut TaskSession) -> Option<TaskId> {
    tasks.cancel_open_requests();
    tasks.resume.selected = None;
    tasks.editor.take().map(|editor| editor.detail.task.id)
}

/// 显式重新加载只有新读取入队成功后才丢弃旧输入，不能撤销刚接受的打开请求。
pub(super) fn discard_for_reload(tasks: &mut TaskSession, id: TaskId) -> bool {
    let queued = matches!(tasks.queries.get(&tasks.latest_open), Some(DetailPurpose::Open(target)) if *target == id);
    let selected = tasks
        .editor
        .as_ref()
        .is_some_and(|editor| editor.detail.task.id == id && editor.in_flight.is_none());
    if !queued || !selected {
        return false;
    }
    tasks.editor.take();
    true
}

/// 迟到的焦点只能作用于同一个已加载编辑器，不得跳到随后打开的另一任务。
pub(super) fn editor_focus_matches(
    expected: TaskId,
    selected: Option<TaskId>,
    visible: bool,
    loading: bool,
) -> bool {
    selected == Some(expected) && visible && !loading
}

#[cfg(test)]
mod tests;
