use fastnote_core::TaskId;

use super::{DetailPurpose, EditorSession, TaskSession};

/// 白名单只保留单个当前详情 ID、一个有界焦点锚点和滚动位置，不保存正文/模型。
#[derive(Default)]
pub(crate) struct TaskResume {
    pub selected: Option<TaskId>,
    pub focus: Option<String>,
    pub scroll_y: f32,
    pub scroll_pending: bool,
    pub focus_pending: bool,
}

impl TaskResume {
    pub(crate) fn capture(&mut self, selected: Option<TaskId>, anchor: &str, scroll_y: f32) {
        self.selected = selected;
        self.focus = focus_anchor(anchor);
        self.scroll_y = if scroll_y.is_finite() {
            scroll_y.min(0.0)
        } else {
            0.0
        };
        self.scroll_pending = true;
        self.focus_pending = true;
    }
}

/// 未知控件/任意输入绝不成为冷驻留缓存，ID 锚点必须是合法的 TaskId。
fn focus_anchor(anchor: &str) -> Option<String> {
    if anchor.len() > 64 {
        return None;
    }
    let fixed = matches!(
        anchor,
        "today:quick"
            | "today:add"
            | "editor:title"
            | "editor:description"
            | "editor:start"
            | "editor:due"
            | "editor:close"
            | "editor:delete"
    );
    let row = anchor
        .strip_prefix("task:")
        .is_some_and(|id| id.parse::<TaskId>().is_ok());
    let section = ["today:section:", "today:more:"].into_iter().any(|prefix| {
        anchor
            .strip_prefix(prefix)
            .is_some_and(|index| index.parse::<u8>().is_ok_and(|index| index < 8))
    });
    (fixed || row || section).then(|| anchor.to_owned())
}

impl TaskSession {
    /// 普通转场也不能绕过失败或尚未读回的编辑状态。
    pub(crate) fn transition_protected(&self) -> bool {
        self.unfinished()
            || self
                .editor
                .as_ref()
                .is_some_and(EditorSession::resource_protected)
    }

    /// 模态取消只撤销匹配许可；未知/旧任务的取消不得清除另一项决策。
    pub(crate) fn cancel_confirmation(&mut self, command: &str) {
        if command == "task:discard-and-quit" {
            self.discard_exit_allowed = false;
        }
        if let Some(id) = command
            .strip_prefix("task:delete-confirm:")
            .and_then(|id| id.parse::<TaskId>().ok())
        {
            self.delete_confirmation.take_if(|pending| *pending == id);
        }
        if let Some(id) = command
            .strip_prefix("task:status-confirm:")
            .and_then(|id| id.parse::<TaskId>().ok())
        {
            self.status_confirmation
                .take_if(|(pending, _)| *pending == id);
        }
        if let Some(id) = command
            .strip_prefix("task:reload-confirm:")
            .and_then(|id| id.parse::<TaskId>().ok())
        {
            self.reload_confirmation.take_if(|pending| *pending == id);
        }
    }

    /// P6：草稿、写确认及尚未解决的决策优先于期限/资源预算。
    pub(crate) fn resource_protected(&self) -> bool {
        self.transition_protected()
            || self.after_save.is_some()
            || self.delete_confirmation.is_some()
            || self.status_confirmation.is_some()
            || self.reload_confirmation.is_some()
            || self.discard_exit_allowed
            || self.queries.values().any(|purpose| {
                matches!(
                    purpose,
                    DetailPurpose::SaveAck(_) | DetailPurpose::Delete(_)
                )
            })
    }

    /// P4/P5：只清理可重查的重数据。轻量恢复、折叠/分页、必要写结果保持不变。
    pub(crate) fn release_clean_cache(&mut self) -> bool {
        if self.resource_protected() {
            return false;
        }
        self.today.take();
        self.editor.take();
        self.cancel_page_reads();
        self.queries.shrink_to_fit();
        self.read_stamps.shrink_to_fit();
        self.pending.shrink_to_fit();
        true
    }
}

#[cfg(test)]
mod tests;
