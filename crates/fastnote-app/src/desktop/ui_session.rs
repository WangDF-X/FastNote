use std::cell::{Cell, RefCell};

use super::presentation::activation::MainActivation;
use fastnote_core::TodayView;
use fastnote_ui::{ShellHandle, ShellView, TodayPresenter};
use slint::Timer;

/// SHELL-03 T1.1：主窗及页面资源的唯一持有者，不强持有 Runtime。
pub(super) struct UiSession {
    pub shell: ShellHandle,
    pub today_presenter: RefCell<Option<TodayPresenter>>,
    pub task_save_timer: Timer,
    pub task_feedback_timer: Timer,
    pub task_saving_visible: Cell<bool>,
    /// 原生与任务回调只绑定一次；重建会话会重新初始化此标记。
    pub callbacks_connected: Cell<bool>,
    /// 只跟踪原生内容的恢复边沿，避免每次 worker 唤醒都重启页面查询。
    pub content_active: Cell<bool>,
    pub main_activation: Cell<MainActivation>,
}

impl UiSession {
    /// 主窗先独立装配；页面模型只在当前路由确实需要时创建。
    pub(super) fn new(shell: ShellHandle) -> Self {
        Self {
            shell,
            today_presenter: RefCell::new(None),
            task_save_timer: Timer::default(),
            task_feedback_timer: Timer::default(),
            task_saving_visible: Cell::new(false),
            callbacks_connected: Cell::new(false),
            content_active: Cell::new(false),
            main_activation: Cell::new(MainActivation::Idle),
        }
    }

    /// SHELL-03 T2.2：仅已请求的今日路由安装列表/日期模型，复用现有实例。
    pub(super) fn ensure_page_resources(&self, page: &str) -> bool {
        let Self { shell, .. } = self;
        ensure_page_resource(&self.today_presenter, page, || {
            let presenter = TodayPresenter::new(shell.component());
            shell.component().set_today_mounted(true);
            presenter
        })
    }

    /// P4：先丢弃页面绑定及重模型，再允许外壳进入限时热驻留。
    pub(super) fn release_page_resources(&self) {
        let Self { shell, .. } = self;
        if let Some(presenter) = self.today_presenter.borrow_mut().take() {
            presenter.release(shell.component());
        }
        self.release_editor_resources();
        shell.component().set_today_state(0);
        self.task_save_timer.stop();
        self.task_feedback_timer.stop();
    }

    /// 关闭已保存详情也必须清空控件绑定的正文，不能仅让抽屉不可见。
    pub(super) fn release_editor_resources(&self) {
        let Self { shell, .. } = self;
        let window = shell.component();
        window.set_task_detail_visible(false);
        window.set_task_detail_loading(false);
        window.set_task_title("".into());
        window.set_task_description("".into());
        window.set_task_start_date("".into());
        window.set_task_start_time("".into());
        window.set_task_due_date("".into());
        window.set_task_due_time("".into());
        window.set_task_context("".into());
        window.set_task_selected_id("".into());
        window.set_task_error("".into());
        window.set_task_save_label("".into());
    }

    /// 回包只能投影已经安装的模型，不能在此顺手加载页面。
    pub(super) fn apply_today(&self, view: &TodayView, collapsed: [Option<bool>; 8]) -> bool {
        let Self { shell, .. } = self;
        self.today_presenter
            .borrow()
            .as_ref()
            .map(|presenter| presenter.apply(shell.component(), view, collapsed))
            .is_some()
    }

    /// 仅应用已由内核确认的路由展示，不能重新分发导航/保存/退出动作。
    pub(super) fn apply_route_view(&self, view: &ShellView) {
        let Self { shell, .. } = self;
        let window = shell.component();
        window.set_current_page(view.page_label.into());
        window.set_page_title(view.title.into());
        window.set_route_context(view.context.clone().into());
        window.set_route_debug(view.route_debug.clone().into());
        window.set_can_go_back(view.can_go_back);
        window.set_plan_detail(view.is_plan_detail);
        window.set_general_settings(view.is_general_settings);
        window.set_settings_section(view.settings_section.clone().into());
        shell.set_local_date(view.local_today);
        window.invoke_focus_route_content();
    }
}

fn ensure_page_resource<T>(
    slot: &RefCell<Option<T>>,
    page: &str,
    create: impl FnOnce() -> T,
) -> bool {
    if page != "今日任务" || slot.borrow().is_some() {
        return false;
    }
    let resource = create();
    *slot.borrow_mut() = Some(resource);
    true
}

impl Drop for UiSession {
    fn drop(&mut self) {
        // 内核的 30 秒日期检测不属于页面；这里只停止页面保存与反馈计时。
        self.task_save_timer.stop();
        self.task_feedback_timer.stop();
    }
}

/// 临时只读访问会话；缺失时不调用投影，也不创建资源。
pub(super) fn with_session<T, R>(
    session: &RefCell<Option<T>>,
    f: impl FnOnce(&T) -> R,
) -> Option<R> {
    session.borrow().as_ref().map(f)
}

#[cfg(test)]
mod tests;
