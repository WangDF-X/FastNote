use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use slint::{ComponentHandle, ModelRc, Timer, VecModel};

use crate::{
    BannerRequest, BannerSet, DialogQueue, DialogRequest, MainWindow, ToastQueue, ToastRequest,
};

use super::{connect, Shell, ShellUpdate, ShellView};

/// 可由 app 生命周期持有的外壳；隐藏不销毁组件、路由展示和反馈队列。
pub struct ShellHandle {
    window: MainWindow,
    shell: Rc<Shell>,
}

impl ShellHandle {
    /// 装配窗口但不启动阻塞事件循环，显示/隐藏/退出由 app 决定。
    pub fn new(
        initial: ShellView,
        handle_action: impl FnMut(String) -> ShellUpdate + 'static,
    ) -> Result<Self, slint::PlatformError> {
        let window = MainWindow::new()?;
        initial.apply(&window);
        window.set_dev_tools(cfg!(debug_assertions));
        let shell = Rc::new(Shell {
            window: window.as_weak(),
            handler: RefCell::new(Box::new(handle_action)),
            toasts: RefCell::new(ToastQueue::new()),
            toast_model: Rc::new(VecModel::default()),
            toast_timer: Timer::default(),
            last_tick: Cell::new(Instant::now()),
            visible: Cell::new(false),
            dialogs: RefCell::new(DialogQueue::new()),
            dialog_token: Cell::new(0),
            banners: RefCell::new(BannerSet::new()),
        });
        window.set_toasts(ModelRc::from(shell.toast_model.clone()));
        connect(&window, &shell);
        Ok(Self { window, shell })
    }

    /// 临时访问组件与 Slint Window；不要保留底层 winit Arc，否则 hide 无法释放窗口。
    pub fn component(&self) -> &MainWindow {
        &self.window
    }

    /// 显示已有组件，并从剩余时长恢复反馈计时。
    pub fn show(&self) -> Result<(), slint::PlatformError> {
        self.window.show()?;
        self.resume_timers();
        Ok(())
    }

    /// 隐藏主窗口，保留视图与反馈状态；失败时不暂停仍可见窗口。
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
