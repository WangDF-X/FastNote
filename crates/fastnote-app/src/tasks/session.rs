use std::collections::HashMap;

use fastnote_core::{NaiveDateTime, TaskId, TaskStatus, TodaySectionKind, TodayView};

use super::EditorSession;

#[derive(Clone, Debug)]
pub(crate) enum Transition {
    Navigate(String),
    Open(TaskId),
    OpenDates(TaskId),
    CloseDetail,
    Close {
        exit: bool,
    },
    Minimize,
    Status {
        id: TaskId,
        status: TaskStatus,
        confirmed: bool,
    },
    Delete(TaskId),
    DeleteConfirmed(TaskId),
    Priority(TaskId),
}

impl Transition {
    fn editor_target(&self) -> Option<TaskId> {
        match self {
            Self::Open(id) | Self::OpenDates(id) => Some(*id),
            _ => None,
        }
    }

    pub(crate) fn editor_focus(&self) -> Option<&'static str> {
        match self {
            Self::Open(_) => Some("editor:title"),
            Self::OpenDates(_) => Some("editor:start"),
            _ => None,
        }
    }
}

pub(crate) enum PendingTask {
    Created { title: String },
    Save { id: TaskId },
    Status { id: TaskId, status: TaskStatus },
    Priority,
    Deleted,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DetailPurpose {
    Open(TaskId),
    Refresh(TaskId),
    SaveAck(TaskId),
    Delete(TaskId),
}

#[derive(Default)]
pub(crate) struct TaskSession {
    pub today: Option<TodayView>,
    pub collapsed: [Option<bool>; 8],
    pub limits: [u32; 8],
    pub editor: Option<EditorSession>,
    pub pending: HashMap<u64, PendingTask>,
    pub queries: HashMap<u64, DetailPurpose>,
    /// 当前今日读取的在途 ID；每次完成后清零，不作为会话生命周期代次。
    pub latest_today: u64,
    /// 读取期间又有失效；先结束当前读取，再按最新条件补读一次。
    pub today_requery: bool,
    pub latest_open: u64,
    pub latest_delete: u64,
    pub after_save: Option<Transition>,
    pub delete_confirmation: Option<TaskId>,
    pub status_confirmation: Option<(TaskId, TaskStatus)>,
    pub reload_confirmation: Option<TaskId>,
    pub discard_exit_allowed: bool,
    /// 仅用户点击隐藏组提示的快捷按钮后请求，等待已提交偏好的查询结果。
    pub reveal_unscheduled: bool,
}

impl TaskSession {
    pub(crate) fn new() -> Self {
        Self {
            limits: [200; 8],
            ..Default::default()
        }
    }

    pub(crate) fn cancel_open_requests(&mut self) {
        self.latest_open = 0;
        self.queries
            .retain(|_, purpose| !matches!(purpose, DetailPurpose::Open(_)));
    }

    /// 页面变为不可见后撤销普通读意图，保存基线/删除确认不随页面取消。
    pub(crate) fn cancel_page_reads(&mut self) {
        self.cancel_open_requests();
        self.queries
            .retain(|_, purpose| !matches!(purpose, DetailPurpose::Refresh(_)));
    }

    /// 合并读取期间的失效，不并发发起第二个今日查询。
    pub(crate) fn invalidate_today_read(&mut self) {
        self.today_requery |= self.latest_today != 0;
    }

    /// 接受当前回包后释放在途状态；旧 ID 不能结束新查询。
    pub(crate) fn finish_today_read(&mut self, id: u64) -> Option<bool> {
        if id == 0 || self.latest_today != id {
            return None;
        }
        self.latest_today = 0;
        Some(std::mem::take(&mut self.today_requery))
    }

    pub(crate) fn current_editor_focus(&self, transition: &Transition) -> Option<&'static str> {
        let target = transition.editor_target()?;
        self.editor
            .as_ref()
            .filter(|editor| editor.detail.task.id == target)?;
        transition.editor_focus()
    }

    pub(crate) fn non_text_pending(&self) -> bool {
        self.pending
            .values()
            .any(|pending| !matches!(pending, PendingTask::Save { .. }))
            || self.queries.values().any(|purpose| {
                matches!(purpose, DetailPurpose::Refresh(id)
                if self.editor.as_ref().is_some_and(|editor| editor.detail.task.id == *id))
            })
            || self
                .queries
                .values()
                .any(|purpose| matches!(purpose, DetailPurpose::Delete(_)))
    }

    pub(crate) fn unfinished(&self) -> bool {
        !self.pending.is_empty() || self.editor.as_ref().is_some_and(EditorSession::unfinished)
    }

    pub(crate) fn needs_clock_refresh(&self, now: NaiveDateTime) -> bool {
        self.today.as_ref().is_none_or(|view| {
            view.date != now.date()
                || now < view.evaluated_at
                || view.next_overdue_at.is_some_and(|deadline| now > deadline)
        })
    }

    pub(crate) fn take_ready_transition(&mut self) -> Option<Transition> {
        if self.unfinished() {
            None
        } else {
            self.after_save.take()
        }
    }

    pub(crate) fn toggle_section(&mut self, index: usize) -> bool {
        let Some(kind) = TodaySectionKind::from_index(index) else {
            return false;
        };
        let Some(section) = self
            .today
            .as_ref()
            .and_then(|view| view.sections.iter().find(|section| section.kind == kind))
        else {
            return false;
        };
        self.collapsed[index] = Some(!self.collapsed[index].unwrap_or(section.collapsed_default));
        true
    }

    pub(crate) fn load_more(&mut self, index: usize) -> bool {
        let Some(limit) = self.limits.get_mut(index) else {
            return false;
        };
        if *limit >= 1000 {
            return false;
        }
        *limit = (*limit + 200).min(1000);
        true
    }

    pub(crate) fn request_unscheduled_reveal(&mut self) {
        self.collapsed[TodaySectionKind::Unscheduled as usize] = Some(false);
        self.reveal_unscheduled = true;
    }

    pub(crate) fn take_unscheduled_reveal(&mut self) -> bool {
        let requested = std::mem::take(&mut self.reveal_unscheduled);
        requested
            && self.today.as_ref().is_some_and(|view| {
                view.sections
                    .iter()
                    .any(|section| section.kind == TodaySectionKind::Unscheduled)
            })
    }

    pub(crate) fn visible_ids(&self) -> Vec<TaskId> {
        self.today
            .iter()
            .flat_map(|view| &view.sections)
            .filter(|section| {
                !self.collapsed[section.kind as usize].unwrap_or(section.collapsed_default)
            })
            .flat_map(|section| section.items.iter().map(|task| task.id))
            .collect()
    }

    pub(crate) fn focus_neighbor(&self, id: TaskId, direction: i32) -> Option<TaskId> {
        let ids = self.visible_ids();
        let index = ids.iter().position(|candidate| *candidate == id)?;
        let next = if direction < 0 {
            index.saturating_sub(1)
        } else {
            (index + 1).min(ids.len().saturating_sub(1))
        };
        ids.get(next).copied()
    }
}

#[cfg(test)]
mod tests;
