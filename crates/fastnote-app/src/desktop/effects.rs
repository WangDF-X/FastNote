use std::cell::RefCell;
use std::collections::VecDeque;

use fastnote_ui::ToastRequest;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use super::runtime::Runtime;
use super::ui_session::UiSession;

/// SHELL-03 T1.2：无主窗时只保留有界的待呈现提示，不新增设置项。
const MAX_PENDING_TOASTS: usize = 32;

/// 焦点意图；其它页面焦点在后续接线批次按需扩展。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FocusTarget {
    Main,
}

/// 只描述待呈现效果，不持有主窗或组件；按 T1.2 批次逐项接入。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum UiEffect {
    Toast(ToastRequest),
    Focus(FocusTarget),
    SetDesktopState(u8),
    SetBusy(bool),
}

impl UiEffect {
    fn same_key(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Toast(left), Self::Toast(right)) => {
                left.kind == right.kind
                    && left.message == right.message
                    && left.action.as_ref().map(|action| &action.command)
                        == right.action.as_ref().map(|action| &action.command)
            }
            (Self::Focus(_), Self::Focus(_))
            | (Self::SetDesktopState(_), Self::SetDesktopState(_))
            | (Self::SetBusy(_), Self::SetBusy(_)) => true,
            _ => false,
        }
    }
}

/// 状态/忙碌/焦点各保留最新一条；Toast 最多 32 条，满时替换最旧 Toast。
#[derive(Default)]
pub(super) struct PendingEffects {
    entries: VecDeque<UiEffect>,
}

impl PendingEffects {
    fn push(&mut self, effect: UiEffect) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.same_key(&effect))
        {
            *existing = effect;
            return;
        }
        if matches!(effect, UiEffect::Toast(_))
            && self
                .entries
                .iter()
                .filter(|entry| matches!(entry, UiEffect::Toast(_)))
                .count()
                >= MAX_PENDING_TOASTS
        {
            if let Some(index) = self
                .entries
                .iter()
                .position(|entry| matches!(entry, UiEffect::Toast(_)))
            {
                self.entries.remove(index);
            }
        }
        self.entries.push_back(effect);
    }

    /// 完整退出时丢弃尚未呈现的意图，不调用窗口或动作入口。
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// 内核保留的最新启动展示状态；只含安全中文提示与日志 ID，不含错误源。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DesktopFeedback {
    state: u8,
    storage_message: String,
    storage_log_id: String,
    hotkey_status: String,
}

impl Default for DesktopFeedback {
    fn default() -> Self {
        Self {
            state: 0,
            storage_message: String::new(),
            storage_log_id: String::new(),
            hotkey_status: "正在注册…".to_owned(),
        }
    }
}

impl DesktopFeedback {
    fn set_state(&mut self, state: u8) {
        self.state = state;
        if state == 1 {
            self.storage_message.clear();
            self.storage_log_id.clear();
        }
    }

    fn storage_failed(&mut self, message: &str, log_id: &str) {
        self.state = 2;
        self.storage_message = message.to_owned();
        self.storage_log_id = log_id.to_owned();
    }
}

fn dispatch_effect(
    pending: &RefCell<PendingEffects>,
    effect: UiEffect,
    present: impl FnOnce(&UiEffect) -> bool,
) {
    // 先尝试呈现，再借用队列；Slint 回调可以同步产生其它效果。
    if !present(&effect) {
        pending.borrow_mut().push(effect);
    }
}

fn replay_effects(pending: &RefCell<PendingEffects>, mut present: impl FnMut(&UiEffect) -> bool) {
    loop {
        let effect = pending.borrow_mut().entries.pop_front();
        let Some(effect) = effect else {
            break;
        };
        // 不跨回调持有队列借用；主窗缺失时原位保留并停止，不能循环重新入队。
        if !present(&effect) {
            pending.borrow_mut().entries.push_front(effect);
            break;
        }
    }
}

impl Runtime {
    /// 无会话只入队；既有会话可以立即呈现，绝不在此创建主窗。
    pub(super) fn emit_ui(&self, effect: UiEffect) {
        if self.finished.get() {
            return;
        }
        dispatch_effect(&self.pending_effects, effect, |effect| {
            self.with_ui(|ui| self.apply_ui_effect(ui, effect))
                .is_some()
        });
    }

    /// 主窗显示前补齐待呈现意图；无会话则保留队列，不自动建窗。
    pub(super) fn flush_ui_effects(&self) {
        if self.finished.get() {
            return;
        }
        replay_effects(&self.pending_effects, |effect| {
            self.with_ui(|ui| self.apply_ui_effect(ui, effect))
                .is_some()
        });
    }

    /// 先更新内核确认状态，再尝试投影。
    pub(super) fn set_desktop_state(&self, state: u8) {
        self.desktop_feedback.borrow_mut().set_state(state);
        self.emit_ui(UiEffect::SetDesktopState(state));
    }

    /// 无主窗时也保留启动错误文案，重建后仍能展示并重试。
    pub(super) fn set_storage_error(&self, message: &str, log_id: &str) {
        self.desktop_feedback
            .borrow_mut()
            .storage_failed(message, log_id);
        self.emit_ui(UiEffect::SetDesktopState(2));
    }

    /// 热键注册属于内核，展示状态不依赖主窗是否存在。
    pub(super) fn set_hotkey_status(&self, status: String) {
        let state = {
            let mut feedback = self.desktop_feedback.borrow_mut();
            feedback.hotkey_status = status;
            feedback.state
        };
        self.emit_ui(UiEffect::SetDesktopState(state));
    }

    fn apply_ui_effect(&self, ui: &UiSession, effect: &UiEffect) {
        match effect {
            UiEffect::Toast(request) => ui.shell.show_toast(request.clone()),
            UiEffect::Focus(FocusTarget::Main) => {
                let window = ui.shell.component().window();
                window.set_minimized(false);
                window.with_winit_window(|window| window.focus_window());
                ui.shell.resume_timers();
            }
            UiEffect::SetDesktopState(state) => {
                let feedback = self.desktop_feedback.borrow().clone();
                let window = ui.shell.component();
                window.set_desktop_state(i32::from(*state));
                window.set_storage_error_message(feedback.storage_message.into());
                window.set_storage_error_log_id(feedback.storage_log_id.into());
                window.set_show_hotkey_status(feedback.hotkey_status.into());
            }
            UiEffect::SetBusy(value) => ui.shell.component().set_desktop_busy(*value),
        }
    }
}

#[cfg(test)]
mod tests;
