//! 主窗口装配：把 Slint 回调接到应用动作入口和反馈组件的视图模型上。

mod handle;
mod preview;
mod view;

#[cfg(test)]
mod focus_contract;

pub use handle::ShellHandle;
pub use view::{NavigationIssue, ShellUpdate, ShellView};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{Model, Timer, TimerMode, VecModel};

use crate::feedback::banner::{BannerKind, BannerRequest, BannerSet};
use crate::feedback::dialog::{DialogFocus, DialogKind, DialogOutcome, DialogQueue, DialogRequest};
use crate::feedback::toast::{ToastItem, ToastKind, ToastQueue, ToastRequest};
use crate::{
    BannerData, BannerTone, DialogData, DialogTone as UiDialogTone, MainWindow, ToastData,
    ToastTone,
};

const TICK: Duration = Duration::from_millis(100);

type ActionHandler = Box<dyn FnMut(String) -> ShellUpdate>;

struct Shell {
    window: slint::Weak<MainWindow>,
    handler: RefCell<ActionHandler>,
    toasts: RefCell<ToastQueue>,
    toast_model: Rc<VecModel<ToastData>>,
    toast_timer: Timer,
    last_tick: Cell<Instant>,
    visible: Cell<bool>,
    dialogs: RefCell<DialogQueue>,
    /// 每次显示对话框自增，用来让 Slint 侧重新聚焦默认按钮。
    dialog_token: Cell<i32>,
    banners: RefCell<BannerSet>,
}

/// Starts the main window and forwards navigation actions to the application controller.
///
/// The handler returns a presentation snapshot and whether the complete route changed.
/// Toast, Banner and Dialog actions are dispatched through the same handler.
pub fn run(
    initial_view: ShellView,
    handle_action: impl FnMut(String) -> ShellUpdate + 'static,
) -> Result<(), slint::PlatformError> {
    use slint::ComponentHandle;
    let shell = ShellHandle::new(initial_view, handle_action)?;
    shell.component().set_desktop_state(1);
    shell.resume_timers();
    shell.component().run()
}

/// 回调只持有 `Shell` 的弱引用，避免窗口与 `Shell` 互相持有。
fn connect(window: &MainWindow, shell: &Rc<Shell>) {
    let weak = Rc::downgrade(shell);
    window.on_route_action({
        let weak = weak.clone();
        move |action| {
            if let Some(shell) = weak.upgrade() {
                shell.dispatch(action.to_string());
            }
        }
    });
    window.on_toast_hover({
        let weak = weak.clone();
        move |id, hovered| {
            if let (Some(shell), Ok(id)) = (weak.upgrade(), u64::try_from(id)) {
                shell.toasts.borrow_mut().set_hovered(id, hovered);
            }
        }
    });
    window.on_toast_action({
        let weak = weak.clone();
        move |id| {
            let Some(shell) = weak.upgrade() else {
                return;
            };
            let command = u64::try_from(id)
                .ok()
                .and_then(|id| shell.toasts.borrow_mut().take_action(id));
            shell.sync_toasts();
            if let Some(command) = command {
                shell.dispatch(command);
            }
        }
    });
    window.on_dialog_confirm({
        let weak = weak.clone();
        move || {
            if let Some(shell) = weak.upgrade() {
                shell.resolve_dialog(DialogOutcome::Confirmed);
            }
        }
    });
    window.on_dialog_cancel({
        let weak = weak.clone();
        move || {
            if let Some(shell) = weak.upgrade() {
                shell.resolve_dialog(DialogOutcome::Cancelled);
            }
        }
    });
    window.on_banner_action({
        let weak = weak.clone();
        move || {
            if let Some(shell) = weak.upgrade() {
                shell.banner_action();
            }
        }
    });
    window.on_banner_dismissed({
        let weak = weak.clone();
        move || {
            if let Some(shell) = weak.upgrade() {
                shell.dismiss_visible_banner();
            }
        }
    });
    window.on_preview_feedback(move |item| {
        if let Some(shell) = weak.upgrade() {
            shell.preview(item.as_str());
        }
    });
}
impl Shell {
    fn dispatch(self: &Rc<Self>, action: String) {
        let update = (self.handler.borrow_mut())(action);
        if update.route_changed {
            // AC-SHELL-10：完整路由变化（包括日期/ID 参数）清除旧 Toast。
            self.toasts.borrow_mut().clear();
            self.sync_toasts();
        }
        if let Some(window) = self.window.upgrade() {
            update.view.apply(&window);
            if update.route_changed {
                window.invoke_focus_route_header();
            }
        }
        if let Some(issue) = update.issue {
            self.show_toast(ToastRequest {
                kind: ToastKind::Error,
                message: issue.message().to_owned(),
                action: None,
            });
        }
    }

    fn show_toast(self: &Rc<Self>, request: ToastRequest) {
        self.toasts.borrow_mut().push(request);
        self.sync_toasts();
        self.ensure_toast_timer();
    }

    fn ensure_toast_timer(self: &Rc<Self>) {
        if !self.visible.get() || self.toast_timer.running() || self.toasts.borrow().is_empty() {
            return;
        }
        self.last_tick.set(Instant::now());
        let shell = Rc::downgrade(self);
        self.toast_timer.start(TimerMode::Repeated, TICK, move || {
            if let Some(shell) = shell.upgrade() {
                shell.tick_toasts();
            }
        });
    }

    fn tick_toasts(self: &Rc<Self>) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick.replace(now));
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);

        let changed = self.toasts.borrow_mut().tick(elapsed_ms);
        if changed {
            self.sync_toasts();
        }
        if self.toasts.borrow().is_empty() {
            self.toast_timer.stop();
        }
    }

    /// 只增删发生变化的行，让仍在显示的 Toast 保持同一个界面元素（保留悬停状态）。
    fn sync_toasts(&self) {
        let queue = self.toasts.borrow();
        let visible = queue.visible();

        for index in (0..self.toast_model.row_count()).rev() {
            let keep = self
                .toast_model
                .row_data(index)
                .is_some_and(|row| visible.iter().any(|item| to_row_id(item.id) == row.id));
            if !keep {
                self.toast_model.remove(index);
            }
        }
        for item in visible {
            let present = self
                .toast_model
                .iter()
                .any(|row| row.id == to_row_id(item.id));
            if !present {
                self.toast_model.push(to_toast_data(item));
            }
        }
    }

    /// SHELL-02 §6：把请求交给队列，队列决定立即显示还是排队。
    fn show_dialog(&self, request: DialogRequest) {
        self.dialogs.borrow_mut().open(request);
        self.sync_dialog();
    }

    /// 结束当前对话框；确认时把动作交给统一入口，排队的对话框随后补位。
    fn resolve_dialog(self: &Rc<Self>, outcome: DialogOutcome) {
        let resolution = self.dialogs.borrow_mut().resolve(outcome);
        self.sync_dialog();
        if let Some(command) = resolution.and_then(|resolved| resolved.command) {
            self.dispatch(command);
        }
    }

    /// 把队列算出的显示状态同步到 Slint：内容、默认焦点、遮罩可关闭性。
    fn sync_dialog(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let dialogs = self.dialogs.borrow();
        let Some(request) = dialogs.current() else {
            window.set_dialog_visible(false);
            return;
        };

        window.set_dialog(DialogData {
            title: request.title.clone().into(),
            body: request.body.clone().into(),
            detail: request.detail.clone().into(),
            confirm_label: request.confirm_label.clone().into(),
            tone: match request.kind {
                DialogKind::Normal => UiDialogTone::Normal,
                DialogKind::Destructive => UiDialogTone::Destructive,
            },
        });
        window.set_dialog_focus_cancel(dialogs.default_focus() == DialogFocus::Cancel);
        window.set_dialog_scrim_closable(dialogs.allows_scrim_close());
        window.set_dialog_visible(true);

        // token 在 visible 之后自增：Slint 侧靠它重新聚焦默认按钮，
        // 排队对话框补位时 visible 没有变化，只有 token 能触发热点。
        let token = self.dialog_token.get().wrapping_add(1);
        self.dialog_token.set(token);
        window.set_dialog_token(token);
    }

    /// SHELL-02 §5：提交一条横幅；集合负责去重与优先级。
    fn show_banner(&self, request: BannerRequest) {
        self.banners.borrow_mut().show(request);
        self.sync_banner();
    }

    /// 关闭当前显示的那条横幅（只有 Warning / Info 允许）。
    fn dismiss_visible_banner(&self) {
        let kind = self.banners.borrow().visible().map(|banner| banner.kind);
        if let Some(kind) = kind {
            self.banners.borrow_mut().dismiss(kind);
            self.sync_banner();
        }
    }

    /// 清空所有横幅（状态解除时调用）。
    fn clear_banners(&self) {
        self.banners.borrow_mut().clear();
        self.sync_banner();
    }

    /// 执行横幅上的操作链接。
    fn banner_action(self: &Rc<Self>) {
        let command = self
            .banners
            .borrow()
            .visible()
            .and_then(|banner| banner.action.as_ref())
            .map(|action| action.command.clone());
        if let Some(command) = command {
            self.dispatch(command);
        }
    }

    /// 把集合选出的那条横幅同步到 Slint。
    fn sync_banner(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let banners = self.banners.borrow();
        let Some(banner) = banners.visible() else {
            window.set_banner_visible(false);
            return;
        };

        window.set_banner(BannerData {
            tone: match banner.kind {
                BannerKind::Error => BannerTone::Error,
                BannerKind::Warning => BannerTone::Warning,
                BannerKind::Info => BannerTone::Info,
                BannerKind::ReadOnly => BannerTone::ReadOnly,
            },
            message: banner.message.clone().into(),
            action_label: banner
                .action
                .as_ref()
                .map(|action| action.label.clone())
                .unwrap_or_default()
                .into(),
            closable: banner.kind.closable(),
        });
        window.set_banner_visible(true);
    }
}

fn to_row_id(id: u64) -> i32 {
    i32::try_from(id).unwrap_or(i32::MAX)
}

fn to_toast_data(item: &ToastItem) -> ToastData {
    ToastData {
        id: to_row_id(item.id),
        tone: match item.kind {
            ToastKind::Success => ToastTone::Success,
            ToastKind::Info => ToastTone::Info,
            ToastKind::Warning => ToastTone::Warning,
            ToastKind::Error => ToastTone::Error,
        },
        message: item.message.clone().into(),
        action_label: item
            .action
            .as_ref()
            .map(|action| action.label.clone())
            .unwrap_or_default()
            .into(),
    }
}
