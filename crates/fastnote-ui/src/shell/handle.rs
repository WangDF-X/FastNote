use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use fastnote_core::NaiveDate;
use slint::{ComponentHandle, ModelRc, Timer, VecModel};

use crate::{
    BannerRequest, BannerSet, DialogOutcome, DialogQueue, DialogRequest, MainWindow, ToastQueue,
    ToastRequest,
};

use super::{connect, RetainedFeedback, Shell, ShellUpdate, ShellView};

/// 可由 app 会话持有的外壳；组件、路由展示和反馈队列保留到句柄被丢弃。
///
/// `hide` 不是卸载：隐藏后仍强持有 MainWindow 和反馈模型。只有调用方移除并 drop
/// 所属会话，才释放本句柄对这些资源的持有；进程内存是否回落仍需实际量测。
pub struct ShellHandle {
    window: MainWindow,
    shell: Rc<Shell>,
}

impl ShellHandle {
    /// 创建 MainWindow 并装配回调，但不启动阻塞事件循环。
    /// 按需加载的调用方只能在确实需要主窗时调用；本方法不负责预热或热缓存计时。
    pub fn new(
        initial: ShellView,
        handle_action: impl FnMut(String) -> ShellUpdate + 'static,
    ) -> Result<Self, slint::PlatformError> {
        let window = MainWindow::new()?;
        initial.apply(&window);
        let shell = Rc::new(Shell {
            window: window.as_weak(),
            handler: RefCell::new(Box::new(handle_action)),
            toasts: RefCell::new(ToastQueue::new()),
            toast_model: Rc::new(VecModel::default()),
            toast_timer: Timer::default(),
            last_tick: Cell::new(Instant::now()),
            visible: Cell::new(false),
            dialogs: RefCell::new(DialogQueue::new()),
            dialog_observer: RefCell::new(Box::new(|_, _| {})),
            dialog_token: Cell::new(0),
            banners: RefCell::new(BannerSet::new()),
        });
        window.set_toasts(ModelRc::from(shell.toast_model.clone()));
        connect(&window, &shell);
        Ok(Self { window, shell })
    }

    /// 临时借用组件与 Slint Window；不要在会话外保存组件克隆或底层 winit Arc。
    /// 额外的强引用会延长组件/原生窗口寿命，使会话 drop 无法完成预期释放。
    pub fn component(&self) -> &MainWindow {
        &self.window
    }

    /// 投影应用层注入的本地今日日期，不加载业务模型、导航或移动焦点。
    /// 应用层在显式显示/恢复及既有时钟观察后调用，不为隐藏窗口创建会话。
    pub fn set_local_date(&self, date: NaiveDate) {
        super::date::apply_date(&self.window, date);
    }

    /// 显示已有组件，并从剩余时长恢复反馈计时。
    pub fn show(&self) -> Result<(), slint::PlatformError> {
        self.window.show()?;
        self.resume_timers();
        Ok(())
    }

    /// 隐藏主窗口，保留组件、视图与反馈模型；失败时不暂停仍可见窗口。
    /// 后端可能释放原生窗口，但这不等于业务模型/完整 UI 资源已经卸载。
    pub fn hide(&self) -> Result<(), slint::PlatformError> {
        self.window.hide()?;
        self.pause_timers();
        Ok(())
    }

    /// 主窗口隐藏、最小化或退出时停止反馈定时器。
    pub fn pause_timers(&self) {
        self.shell.visible.set(false);
        self.shell.toast_timer.stop();
    }

    /// 模态决策完成时通知 app 清理匹配许可；观察者不能持有组件或内核强引用。
    pub fn on_dialog_resolved(&self, observer: impl FnMut(&str, DialogOutcome) + 'static) {
        *self.shell.dialog_observer.borrow_mut() = Box::new(observer);
    }

    /// 只在无未决对话框时移动纯反馈状态；不重置 Toast 剩余时长，不保留模型。
    pub fn take_retained_feedback(&self) -> Option<RetainedFeedback> {
        if self.shell.dialogs.borrow().is_open() {
            return None;
        }
        self.pause_timers();
        let mut toasts = std::mem::take(&mut *self.shell.toasts.borrow_mut());
        toasts.retain_for_unload();
        Some(RetainedFeedback {
            toasts,
            banners: std::mem::take(&mut *self.shell.banners.borrow_mut()),
        })
    }

    /// 冷重建只恢复反馈状态，不分发确认或导航命令；计时由实际显示再恢复。
    pub fn restore_retained_feedback(&self, feedback: RetainedFeedback) {
        *self.shell.toasts.borrow_mut() = feedback.toasts;
        *self.shell.banners.borrow_mut() = feedback.banners;
        self.shell.sync_toasts();
        self.shell.sync_banner();
    }

    /// 恢复窗口后重启计时，不把隐藏期间的时长扣掉。
    pub fn resume_timers(&self) {
        self.shell.visible.set(true);
        self.shell.ensure_toast_timer();
    }

    /// 将入口/后台反馈送入既有 Toast 队列。
    pub fn show_toast(&self, request: ToastRequest) {
        self.shell.show_toast(request);
    }

    /// 使用既有对话框队列进行退出失败确认。
    pub fn show_dialog(&self, request: DialogRequest) {
        self.shell.show_dialog(request);
    }

    /// 展示可见性/持久化错误横幅。
    pub fn show_banner(&self, request: BannerRequest) {
        self.shell.show_banner(request);
    }

    /// 使用与 UI 点击相同的路由/动作入口。
    pub fn dispatch(&self, action: String) {
        self.shell.dispatch(action);
    }
}
