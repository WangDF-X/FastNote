//! Toast 队列的纯逻辑（SHELL-02 §3）。
//!
//! 队列不读取系统时钟：调用方用 [`ToastQueue::tick`] 传入经过的毫秒数，
//! 这样时长、悬停暂停和去重都可以在单元测试里确定性地验证。

use std::collections::VecDeque;

/// 同时可见的 Toast 上限。
pub const MAX_VISIBLE_TOASTS: usize = 3;
/// 相同文案在该窗口内只显示一条。
pub const DEDUP_WINDOW_MS: u64 = 2_000;
/// P3：隐藏/卸载期间只保留有界的安全反馈，不改变活动态队列的 FIFO 行为。
pub const MAX_RETAINED_TOASTS: usize = 32;

const SUCCESS_MS: u64 = 3_000;
const INFO_MS: u64 = 3_000;
const WARNING_MS: u64 = 5_000;
const ERROR_MS: u64 = 6_000;
const WITH_ACTION_MS: u64 = 6_000;

/// Toast 类型，决定图标、颜色和显示时长。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastKind {
    /// 操作成功。
    Success,
    /// 中性信息。
    Info,
    /// 非阻断警告。
    Warning,
    /// 非阻断失败。
    Error,
}

impl ToastKind {
    /// 返回显示时长；带操作的 Toast 统一停留 6 秒。
    pub fn duration_ms(self, has_action: bool) -> u64 {
        if has_action {
            return WITH_ACTION_MS;
        }
        match self {
            Self::Success => SUCCESS_MS,
            Self::Info => INFO_MS,
            Self::Warning => WARNING_MS,
            Self::Error => ERROR_MS,
        }
    }
}

/// Toast 上的可选操作。点击后 `command` 交给应用的统一动作入口处理。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastAction {
    /// 按钮文案，例如「查看」。
    pub label: String,
    /// 动作字符串，与导航使用同一套格式，例如 `navigate:今日任务`。
    pub command: String,
}

/// 调用方提交的 Toast 请求。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastRequest {
    /// 类型。
    pub kind: ToastKind,
    /// 文案；不得包含任务标题、小记正文等敏感内容之外的内部信息。
    pub message: String,
    /// 可选操作。
    pub action: Option<ToastAction>,
}

/// 正在显示的 Toast。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastItem {
    /// 队列内唯一 ID。
    pub id: u64,
    /// 类型。
    pub kind: ToastKind,
    /// 文案。
    pub message: String,
    /// 可选操作。
    pub action: Option<ToastAction>,
    remaining_ms: u64,
    hovered: bool,
}

/// [`ToastQueue::push`] 的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushOutcome {
    /// 已立即显示。
    Shown(u64),
    /// 可见区已满，已排队等待。
    Queued,
    /// 2 秒内出现过相同文案，被丢弃。
    Duplicate,
}

/// Toast 队列：最多 3 条可见，其余排队；相同文案去重；悬停时暂停计时。
#[derive(Debug, Default)]
pub struct ToastQueue {
    visible: Vec<ToastItem>,
    pending: VecDeque<ToastRequest>,
    recent: Vec<(String, u64)>,
    clock_ms: u64,
    next_id: u64,
}

impl ToastQueue {
    /// 创建空队列。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前可见的 Toast，最旧的在前，新的在后（显示在底部）。
    pub fn visible(&self) -> &[ToastItem] {
        &self.visible
    }

    /// 队列中没有可见或排队的 Toast。
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.pending.is_empty()
    }

    /// 提交一条 Toast。
    pub fn push(&mut self, request: ToastRequest) -> PushOutcome {
        if self.is_duplicate(&request.message) {
            return PushOutcome::Duplicate;
        }
        self.recent.push((request.message.clone(), self.clock_ms));

        if self.visible.len() < MAX_VISIBLE_TOASTS {
            PushOutcome::Shown(self.show(request))
        } else {
            self.pending.push_back(request);
            PushOutcome::Queued
        }
    }

    /// 推进时间。返回可见列表是否发生变化（过期移除或排队项上屏）。
    pub fn tick(&mut self, elapsed_ms: u64) -> bool {
        self.clock_ms = self.clock_ms.saturating_add(elapsed_ms);
        self.recent
            .retain(|(_, at)| self.clock_ms.saturating_sub(*at) < DEDUP_WINDOW_MS);

        let before = self.visible.len();
        for item in self.visible.iter_mut().filter(|item| !item.hovered) {
            item.remaining_ms = item.remaining_ms.saturating_sub(elapsed_ms);
        }
        self.visible.retain(|item| item.remaining_ms > 0);
        let removed = before != self.visible.len();

        let promoted = self.promote_pending();
        removed || promoted
    }

    /// 设置鼠标悬停状态；悬停期间该条不计时。
    pub fn set_hovered(&mut self, id: u64, hovered: bool) {
        if let Some(item) = self.visible.iter_mut().find(|item| item.id == id) {
            item.hovered = hovered;
        }
    }

    /// 关闭一条 Toast。返回是否存在该 ID。
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.visible.len();
        self.visible.retain(|item| item.id != id);
        let removed = before != self.visible.len();
        self.promote_pending();
        removed
    }

    /// 执行 Toast 的操作：关闭该条并返回要分发的动作字符串。
    pub fn take_action(&mut self, id: u64) -> Option<String> {
        let command = self
            .visible
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.action.as_ref())
            .map(|action| action.command.clone())?;
        self.dismiss(id);
        Some(command)
    }

    /// 清空可见和排队的 Toast（页面切换时使用，AC-SHELL-10）。
    pub fn clear(&mut self) {
        self.visible.clear();
        self.pending.clear();
    }

    /// 移交冷驻留前收紧容量，保留可见项的剩余时长/ID；重建后不保留旧鼠标悬停。
    pub fn retain_for_unload(&mut self) {
        let limit = MAX_RETAINED_TOASTS.saturating_sub(self.visible.len());
        while self.pending.len() > limit {
            self.pending.pop_front();
        }
        if self.recent.len() > MAX_RETAINED_TOASTS {
            self.recent.drain(..self.recent.len() - MAX_RETAINED_TOASTS);
        }
        for item in &mut self.visible {
            item.hovered = false;
        }
        self.visible.shrink_to_fit();
        self.pending.shrink_to_fit();
        self.recent.shrink_to_fit();
    }

    fn is_duplicate(&self, message: &str) -> bool {
        self.recent.iter().any(|(seen, _)| seen == message)
    }

    fn show(&mut self, request: ToastRequest) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let remaining_ms = request.kind.duration_ms(request.action.is_some());
        self.visible.push(ToastItem {
            id,
            kind: request.kind,
            message: request.message,
            action: request.action,
            remaining_ms,
            hovered: false,
        });
        id
    }

    fn promote_pending(&mut self) -> bool {
        let mut promoted = false;
        while self.visible.len() < MAX_VISIBLE_TOASTS {
            let Some(request) = self.pending.pop_front() else {
                break;
            };
            self.show(request);
            promoted = true;
        }
        promoted
    }
}

#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: ToastKind, message: &str) -> ToastRequest {
        ToastRequest {
            kind,
            message: message.to_owned(),
            action: None,
        }
    }

    #[test]
    fn test_duration_depends_on_kind_and_action() {
        assert_eq!(ToastKind::Success.duration_ms(false), 3_000);
        assert_eq!(ToastKind::Info.duration_ms(false), 3_000);
        assert_eq!(ToastKind::Warning.duration_ms(false), 5_000);
        assert_eq!(ToastKind::Error.duration_ms(false), 6_000);
        assert_eq!(ToastKind::Success.duration_ms(true), 6_000);
    }

    #[test]
    fn test_toast_expires_after_its_duration() {
        let mut queue = ToastQueue::new();
        queue.push(request(ToastKind::Success, "任务已创建"));

        assert!(!queue.tick(2_999));
        assert_eq!(queue.visible().len(), 1);
        assert!(queue.tick(1));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_error_toast_stays_longer_than_success() {
        let mut queue = ToastQueue::new();
        queue.push(request(ToastKind::Success, "已保存"));
        queue.push(request(ToastKind::Error, "保存失败，请重试"));

        queue.tick(3_000);

        assert_eq!(queue.visible().len(), 1);
        assert_eq!(queue.visible()[0].kind, ToastKind::Error);
    }

    #[test]
    fn test_fourth_toast_is_queued_until_a_slot_frees() {
        let mut queue = ToastQueue::new();
        for index in 0..3 {
            queue.push(request(ToastKind::Error, &format!("错误 {index}")));
        }

        assert_eq!(
            queue.push(request(ToastKind::Info, "第四条")),
            PushOutcome::Queued
        );
        assert_eq!(queue.visible().len(), MAX_VISIBLE_TOASTS);

        assert!(queue.tick(6_000));
        assert_eq!(queue.visible().len(), 1);
        assert_eq!(queue.visible()[0].message, "第四条");
    }

    #[test]
    fn test_same_message_within_two_seconds_is_dropped() {
        let mut queue = ToastQueue::new();
        queue.push(request(ToastKind::Success, "已归档"));
        queue.tick(1_999);

        assert_eq!(
            queue.push(request(ToastKind::Success, "已归档")),
            PushOutcome::Duplicate
        );
        assert_eq!(queue.visible().len(), 1);
    }

    #[test]
    fn test_same_message_is_allowed_after_dedup_window() {
        let mut queue = ToastQueue::new();
        queue.push(request(ToastKind::Success, "已归档"));
        queue.tick(2_000);

        assert!(matches!(
            queue.push(request(ToastKind::Success, "已归档")),
            PushOutcome::Shown(_)
        ));
    }

    #[test]
    fn test_hovered_toast_does_not_expire() {
        let mut queue = ToastQueue::new();
        let PushOutcome::Shown(id) = queue.push(request(ToastKind::Success, "已保存")) else {
            panic!("toast should be shown immediately");
        };
        queue.set_hovered(id, true);

        queue.tick(10_000);
        assert_eq!(queue.visible().len(), 1);

        queue.set_hovered(id, false);
        queue.tick(3_000);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_hover_only_pauses_the_hovered_toast() {
        let mut queue = ToastQueue::new();
        let PushOutcome::Shown(first) = queue.push(request(ToastKind::Success, "第一条")) else {
            panic!("toast should be shown immediately");
        };
        queue.push(request(ToastKind::Success, "第二条"));
        queue.set_hovered(first, true);

        queue.tick(3_000);

        assert_eq!(queue.visible().len(), 1);
        assert_eq!(queue.visible()[0].message, "第一条");
    }

    #[test]
    fn test_clear_removes_visible_and_pending() {
        let mut queue = ToastQueue::new();
        for index in 0..5 {
            queue.push(request(ToastKind::Error, &format!("错误 {index}")));
        }

        queue.clear();

        assert!(queue.is_empty());
        assert!(!queue.tick(100));
    }

    #[test]
    fn test_dismiss_promotes_pending_toast() {
        let mut queue = ToastQueue::new();
        let mut first_id = 0;
        for index in 0..4 {
            if let PushOutcome::Shown(id) =
                queue.push(request(ToastKind::Error, &format!("错误 {index}")))
            {
                if index == 0 {
                    first_id = id;
                }
            }
        }

        assert!(queue.dismiss(first_id));

        assert_eq!(queue.visible().len(), MAX_VISIBLE_TOASTS);
        assert_eq!(queue.visible()[2].message, "错误 3");
    }
}
