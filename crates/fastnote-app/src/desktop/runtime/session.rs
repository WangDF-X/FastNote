use std::cell::RefCell;
use std::rc::Rc;

use fastnote_ui::{ShellHandle, ShellUpdate};

use crate::desktop::effects::UiEffect;
use crate::desktop::presentation::SwitchError;
use crate::desktop::ui_session::UiSession;

use super::Runtime;

impl Runtime {
    /// 只有显式显示/错误恢复才创建主窗；已存在则复用，不重放业务动作或重置路由。
    pub(in crate::desktop) fn ensure_ui(self: &Rc<Self>) -> Result<(), SwitchError> {
        if self.finished.get() {
            return Err(SwitchError::ShowTarget);
        }
        self.check_ui_lease();
        let mut epoch = self.ui_epoch.get();
        if self.ui.borrow().is_none() && !epoch.mount() {
            return Err(SwitchError::ShowTarget);
        }
        let generation = epoch.session().ok_or(SwitchError::ShowTarget)?;
        let created = ensure_session(&self.ui, || {
            let initial = self.controller.borrow().shell_view();
            let target = Rc::new(RefCell::new(Rc::downgrade(self)));
            let shell = ShellHandle::new(initial.clone(), {
                let target = target.clone();
                move |action| {
                    let runtime = target.borrow().upgrade().filter(|runtime| runtime.ui_epoch.get().session() == Some(generation));
                    match runtime {
                        Some(runtime) => runtime.handle_action(&action),
                        None => ShellUpdate { view: initial.clone(), route_changed: false, issue: None },
                    }
                }
            }).map_err(|error| {
                tracing::error!(phase = "ui_session_create", error_kind = ?error, "main session construction failed");
                SwitchError::ShowTarget
            })?;
            Ok::<_, SwitchError>(UiSession::new(shell))
        })?;
        if created {
            self.ui_epoch.set(epoch);
            self.ui_mounted();
            self.main_icon.set(0);
            self.main_caption.set(0);
            self.main_frame.set(None);
            self.connect_ui();
            if let Some(feedback) = self.retained_feedback.borrow_mut().take() {
                let _ = self.with_ui(|ui| ui.shell.restore_retained_feedback(feedback));
            }
            self.sync_settings();
            self.sync_entry_availability();
            self.emit_ui(UiEffect::SetBusy(self.busy.get()));
            self.flush_ui_effects();
            tracing::info!(
                phase = "ui_session_created",
                session_generation = generation,
                "main session constructed on demand"
            );
        }
        // D-SHELL-02：显式冷/热恢复先同步真实今日，加载/错误态也不沿用隐藏前日期。
        let date = self.observe_task_clock().sample.local.date();
        let _ = self.with_ui(|ui| ui.shell.set_local_date(date));
        self.ensure_current_page_resources();
        Ok(())
    }

    /// 首次显示/可见路由进入才加载当前页面；数据失败界面不装配今日模型。
    pub(in crate::desktop) fn ensure_current_page_resources(&self) {
        if !self.ready.get() || self.finished.get() {
            return;
        }
        let page = self.controller.borrow().shell_view().page_label;
        if self.with_ui(|ui| ui.ensure_page_resources(page)) == Some(true) {
            tracing::info!(phase = "page_resources_created", query_scope = "today");
        }
    }
}

/// 不跨工厂借用可选槽；失败不留下半个会话，也不修改已经存在的资源。
fn ensure_session<T, E>(
    slot: &RefCell<Option<T>>,
    create: impl FnOnce() -> Result<T, E>,
) -> Result<bool, E> {
    if slot.borrow().is_some() {
        return Ok(false);
    }
    let session = create()?;
    *slot.borrow_mut() = Some(session);
    Ok(true)
}

#[cfg(test)]
mod tests;
