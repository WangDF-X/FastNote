//! Dialog 队列的纯逻辑（SHELL-02 §6）。
//!
//! 队列只描述「谁该显示、默认聚焦哪个按钮、遮罩能不能关闭」，
//! 不碰 Slint 元素；界面侧读取队列算出的属性，因此这些规则可以单元测试。

use std::collections::VecDeque;

/// Dialog 的语义类型，同时决定主按钮样式、默认焦点和关闭策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogKind {
    /// 非要命性确认，主按钮为强调色。
    Normal,
    /// 破坏性操作，主按钮为危险色。
    Destructive,
}

/// 对话框打开后应当聚焦的按钮。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogFocus {
    /// 主操作按钮。
    Confirm,
    /// 取消按钮。
    Cancel,
}

/// 用户对对话框的答复。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogOutcome {
    /// 点击主操作。
    Confirmed,
    /// 点击取消、按 Esc 或点击遮罩。
    Cancelled,
}

/// 取消按钮的固定文案（SHELL-02 §6.1「取消在左」）。
pub const CANCEL_LABEL: &str = "取消";

/// 一次对话框请求。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogRequest {
    /// 标题，写成动作式提问，例如「删除任务？」。
    pub title: String,
    /// 正文，说明操作后果。
    pub body: String,
    /// 可选补充信息（数量、路径）；空字符串表示不显示。
    pub detail: String,
    /// 主按钮文案，必须是具体动词，例如「删除」「归档」。
    pub confirm_label: String,
    /// 取消按钮文案，默认「取消」。
    pub cancel_label: String,
    /// 语义类型。
    pub kind: DialogKind,
    /// 确认后交给应用统一动作入口的字符串；空字符串表示只关闭、不分发动作。
    pub confirm_command: String,
}

impl DialogRequest {
    /// 创建一个不带补充信息的请求。
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        confirm_label: impl Into<String>,
        kind: DialogKind,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            detail: String::new(),
            confirm_label: confirm_label.into(),
            cancel_label: CANCEL_LABEL.to_owned(),
            kind,
            confirm_command: String::new(),
        }
    }

    /// 追加补充信息（数量、路径等）。
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// 指定确认后要分发的动作字符串。
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.confirm_command = command.into();
        self
    }
}

/// [`DialogQueue::open`] 的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenOutcome {
    /// 没有其它对话框，已立即显示。
    Shown,
    /// 已有对话框显示中，本次请求排队等待。
    Queued,
}

/// 对话框关闭时产生的结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resolution {
    /// 用户的选择。
    pub outcome: DialogOutcome,
    /// 确认时要分发的动作；取消或未指定动作时为 `None`。
    pub command: Option<String>,
}

/// 对话框队列：同一时刻最多显示一个，其余按提交顺序排队等待。
///
/// SHELL-02 §6.1「打开第二个请求排队，不叠加」：可见卡片永远只有一个，
/// 排队的请求在前一个结束后按 FIFO 依次补位，不限制队列长度。
#[derive(Debug, Default)]
pub struct DialogQueue {
    current: Option<DialogRequest>,
    pending: VecDeque<DialogRequest>,
}

impl DialogQueue {
    /// 创建空队列。
    pub fn new() -> Self {
        Self::default()
    }

    /// 正在显示的对话框。
    pub fn current(&self) -> Option<&DialogRequest> {
        self.current.as_ref()
    }

    /// 是否有对话框正在显示。
    pub fn is_open(&self) -> bool {
        self.current.is_some()
    }

    /// 除当前之外还在排队的请求数量。
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// 提交一个对话框请求。
    pub fn open(&mut self, request: DialogRequest) -> OpenOutcome {
        if self.current.is_none() {
            self.current = Some(request);
            OpenOutcome::Shown
        } else {
            self.pending.push_back(request);
            OpenOutcome::Queued
        }
    }

    /// 打开时应当聚焦的按钮：破坏性操作聚焦「取消」（AC-SHELL-13）。
    pub fn default_focus(&self) -> DialogFocus {
        match self.current.as_ref().map(|request| request.kind) {
            Some(DialogKind::Normal) | None => DialogFocus::Confirm,
            Some(DialogKind::Destructive) => DialogFocus::Cancel,
        }
    }

    /// 点击遮罩能否关闭：非破坏性可以，破坏性只能通过按钮或 Esc（SHELL-02 §6.1）。
    pub fn allows_scrim_close(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|request| request.kind == DialogKind::Normal)
    }

    /// 结束当前对话框；排队中的下一个请求自动补位。
    ///
    /// 没有正在显示的对话框时返回 `None`，调用方无需额外判断。
    pub fn resolve(&mut self, outcome: DialogOutcome) -> Option<Resolution> {
        let request = self.current.take()?;
        self.current = self.pending.pop_front();
        let command = match (outcome, request.confirm_command.is_empty()) {
            (DialogOutcome::Confirmed, false) => Some(request.confirm_command),
            _ => None,
        };
        Some(Resolution { outcome, command })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(title: &str) -> DialogRequest {
        DialogRequest::new(title, "说明", "继续", DialogKind::Normal)
    }

    fn destructive(title: &str) -> DialogRequest {
        DialogRequest::new(title, "说明", "删除", DialogKind::Destructive)
    }

    #[test]
    fn test_first_dialog_is_shown_immediately() {
        let mut queue = DialogQueue::new();
        assert_eq!(queue.open(normal("归档计划？")), OpenOutcome::Shown);
        assert!(queue.is_open());
        assert_eq!(
            queue.current().map(|r| r.title.as_str()),
            Some("归档计划？")
        );
    }

    #[test]
    fn test_second_dialog_waits_until_first_resolves() {
        let mut queue = DialogQueue::new();
        queue.open(normal("第一个"));

        assert_eq!(queue.open(normal("第二个")), OpenOutcome::Queued);
        assert_eq!(queue.current().map(|r| r.title.as_str()), Some("第一个"));
        assert_eq!(queue.pending_count(), 1);

        queue.resolve(DialogOutcome::Cancelled);

        assert_eq!(queue.current().map(|r| r.title.as_str()), Some("第二个"));
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn test_all_requests_queue_in_fifo_order() {
        let mut queue = DialogQueue::new();
        queue.open(normal("第一个"));
        assert_eq!(queue.open(normal("第二个")), OpenOutcome::Queued);
        assert_eq!(queue.open(normal("第三个")), OpenOutcome::Queued);
        assert_eq!(queue.pending_count(), 2);

        queue.resolve(DialogOutcome::Cancelled);
        assert_eq!(queue.current().map(|r| r.title.as_str()), Some("第二个"));

        queue.resolve(DialogOutcome::Confirmed);
        assert_eq!(queue.current().map(|r| r.title.as_str()), Some("第三个"));
        assert_eq!(queue.pending_count(), 0);

        queue.resolve(DialogOutcome::Cancelled);
        assert!(!queue.is_open());
    }

    #[test]
    fn test_resolve_without_open_dialog_returns_none() {
        let mut queue = DialogQueue::new();
        assert_eq!(queue.resolve(DialogOutcome::Confirmed), None);
        assert!(!queue.is_open());
    }

    #[test]
    fn test_normal_dialog_defaults_focus_to_confirm() {
        let mut queue = DialogQueue::new();
        queue.open(normal("发布计划？"));
        assert_eq!(queue.default_focus(), DialogFocus::Confirm);
    }

    #[test]
    fn test_destructive_dialog_defaults_focus_to_cancel() {
        let mut queue = DialogQueue::new();
        queue.open(destructive("删除任务？"));
        assert_eq!(queue.default_focus(), DialogFocus::Cancel);
    }

    #[test]
    fn test_normal_dialog_can_be_closed_by_scrim() {
        let mut queue = DialogQueue::new();
        queue.open(normal("发布计划？"));
        assert!(queue.allows_scrim_close());
    }

    #[test]
    fn test_destructive_dialog_ignores_scrim() {
        let mut queue = DialogQueue::new();
        queue.open(destructive("删除任务？"));
        assert!(!queue.allows_scrim_close());
    }

    #[test]
    fn test_scrim_is_not_closable_without_dialog() {
        let queue = DialogQueue::new();
        assert!(!queue.allows_scrim_close());
    }

    #[test]
    fn test_confirm_returns_command_and_promotes_pending() {
        let mut queue = DialogQueue::new();
        queue.open(normal("归档计划？").with_command("archive:plan-1"));
        queue.open(destructive("删除任务？"));

        let resolution = queue.resolve(DialogOutcome::Confirmed);

        assert_eq!(
            resolution,
            Some(Resolution {
                outcome: DialogOutcome::Confirmed,
                command: Some("archive:plan-1".to_owned()),
            })
        );
        assert_eq!(
            queue.current().map(|r| r.title.as_str()),
            Some("删除任务？")
        );
    }

    #[test]
    fn test_cancel_drops_command() {
        let mut queue = DialogQueue::new();
        queue.open(normal("归档计划？").with_command("archive:plan-1"));

        let resolution = queue.resolve(DialogOutcome::Cancelled);

        assert_eq!(
            resolution,
            Some(Resolution {
                outcome: DialogOutcome::Cancelled,
                command: None,
            })
        );
        assert!(!queue.is_open());
    }

    #[test]
    fn test_confirm_without_command_only_closes() {
        let mut queue = DialogQueue::new();
        queue.open(normal("发布计划？"));

        let resolution = queue.resolve(DialogOutcome::Confirmed);

        assert_eq!(resolution.map(|r| r.command), Some(None));
    }

    #[test]
    fn test_new_request_uses_default_cancel_label() {
        let request = normal("删除任务？");
        assert_eq!(request.cancel_label, CANCEL_LABEL);
        assert!(request.detail.is_empty());
    }

    #[test]
    fn test_with_detail_keeps_body_and_detail_separate() {
        let request = destructive("删除 3 个任务？").with_detail("包含 2 个子任务");
        assert_eq!(request.body, "说明");
        assert_eq!(request.detail, "包含 2 个子任务");
    }
}
