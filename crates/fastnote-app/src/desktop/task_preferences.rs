use std::rc::Rc;

use fastnote_core::{DesktopSetting, TodayPreferences, TodaySort};
use fastnote_store::StoreError;
use fastnote_ui::{storage_message, ToastAction, ToastKind, ToastRequest};

use super::runtime::{Completion, Runtime};

const ACTION_PREFIX: &str = "settings:tasks:";
const REVEAL_ACTION: &str = "settings:tasks:reveal-unscheduled";

#[derive(Debug, PartialEq)]
struct PreferenceRequest {
    change: DesktopSetting,
    reveal_unscheduled: bool,
}

fn parse_request(action: &str) -> Option<PreferenceRequest> {
    let value = action.strip_prefix(ACTION_PREFIX)?;
    let reveal_unscheduled = value == "reveal-unscheduled";
    let change = if reveal_unscheduled {
        DesktopSetting::TodayShowUnscheduled(true)
    } else if let Some(value) = value.strip_prefix("sort:") {
        DesktopSetting::TodaySort(TodaySort::parse(value)?)
    } else {
        let (group, value) = value.split_once(':')?;
        let shown = match value {
            "true" => true,
            "false" => false,
            _ => return None,
        };
        match group {
            "in-progress" => DesktopSetting::TodayShowInProgress(shown),
            "waiting" => DesktopSetting::TodayShowWaiting(shown),
            "unscheduled" => DesktopSetting::TodayShowUnscheduled(shown),
            "completed" => DesktopSetting::TodayShowCompleted(shown),
            _ => return None,
        }
    };
    Some(PreferenceRequest {
        change,
        reveal_unscheduled,
    })
}

fn confirmed_value(
    preferences: TodayPreferences,
    change: &DesktopSetting,
) -> Option<DesktopSetting> {
    Some(match change {
        DesktopSetting::TodaySort(_) => DesktopSetting::TodaySort(preferences.sort),
        DesktopSetting::TodayShowInProgress(_) => {
            DesktopSetting::TodayShowInProgress(preferences.show_in_progress)
        }
        DesktopSetting::TodayShowWaiting(_) => {
            DesktopSetting::TodayShowWaiting(preferences.show_waiting)
        }
        DesktopSetting::TodayShowUnscheduled(_) => {
            DesktopSetting::TodayShowUnscheduled(preferences.show_unscheduled)
        }
        DesktopSetting::TodayShowCompleted(_) => {
            DesktopSetting::TodayShowCompleted(preferences.show_completed)
        }
        _ => return None,
    })
}

fn sort_index(sort: TodaySort) -> i32 {
    match sort {
        TodaySort::Priority => 0,
        TodaySort::DueDate => 1,
        TodaySort::CreatedDescending => 2,
    }
}

/// Q-SET-10：仅成功创建且未安排组被隐藏时提示；快捷恢复不包含任务正文。
pub(super) fn hidden_task_created(preferences: TodayPreferences) -> Option<ToastRequest> {
    (!preferences.show_unscheduled).then(|| ToastRequest {
        kind: ToastKind::Info,
        message: "任务已添加，未安排分组已隐藏。".to_owned(),
        action: Some(ToastAction {
            label: "撤销隐藏分组".to_owned(),
            command: REVEAL_ACTION.to_owned(),
        }),
    })
}

impl Runtime {
    pub(super) fn sync_task_preferences(&self) {
        let preferences = self.settings.borrow().today_preferences;
        let error = self
            .task_preferences_error
            .borrow()
            .clone()
            .unwrap_or_default();
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            // 对话框由 Slint 的实时绑定禁用；不把临时模态状态写进确认投影，避免关闭后仍禁用。
            window.set_task_preferences_enabled(
                self.ready.get() && !self.finished.get() && !self.closing.get() && !self.busy.get(),
            );
            window.set_task_preferences_error(error.into());
            window.set_task_sort_index(sort_index(preferences.sort));
            window.set_task_show_in_progress(preferences.show_in_progress);
            window.set_task_show_waiting(preferences.show_waiting);
            window.set_task_show_unscheduled(preferences.show_unscheduled);
            window.set_task_show_completed(preferences.show_completed);
        });
    }

    pub(super) fn handle_task_preferences_action(self: &Rc<Self>, action: &str) -> bool {
        if !action.starts_with(ACTION_PREFIX) {
            return false;
        }
        let Some(request) = parse_request(action) else {
            self.notice(ToastKind::Error, "任务偏好选项无效，请重新选择。");
            return true;
        };
        if !self.can_task_interact() {
            self.notice(ToastKind::Info, "当前操作尚未完成，请稍后再修改任务偏好。");
            return true;
        }
        let preferences = self.settings.borrow().today_preferences;
        if confirmed_value(preferences, &request.change).as_ref() == Some(&request.change) {
            self.task_preferences_error.borrow_mut().take();
            self.sync_task_preferences();
            if request.reveal_unscheduled {
                self.tasks.borrow_mut().request_unscheduled_reveal();
                self.request_today();
            }
            return true;
        }
        self.task_preferences_error.borrow_mut().take();
        if self.queue(
            vec![request.change],
            Completion::TaskPreference {
                reveal_unscheduled: request.reveal_unscheduled,
            },
            false,
        ) {
            self.set_busy(true);
        } else {
            *self.task_preferences_error.borrow_mut() =
                Some("保存队列不可用，偏好未修改，请重试。".to_owned());
            self.sync_task_preferences();
        }
        true
    }

    pub(super) fn task_preference_failed(&self, error: &StoreError) {
        let message = if matches!(error, StoreError::NewerSchema { .. }) {
            "数据由更高版本创建，请升级应用。"
        } else {
            storage_message(error.code())
        };
        *self.task_preferences_error.borrow_mut() =
            Some(format!("{message}\n日志 ID：{}", error.log_id()));
        self.set_busy(false);
        self.notice(ToastKind::Error, message);
        self.sync_settings();
        self.restore_preference_focus();
    }

    pub(super) fn task_preference_committed(&self, reveal_unscheduled: bool) {
        self.task_preferences_error.borrow_mut().take();
        if reveal_unscheduled {
            self.tasks.borrow_mut().request_unscheduled_reveal();
        }
        self.set_busy(false);
        // 与设置共用 FIFO 连接，读模型只能看见已经提交的偏好。
        self.request_today();
        self.restore_preference_focus();
    }

    fn restore_preference_focus(&self) {
        let _ = self.with_ui(|ui| {
            if !self.closing.get()
                && !self.finished.get()
                && !ui.shell.component().get_dialog_visible()
            {
                ui.shell.component().invoke_restore_task_focus();
            }
        });
    }
}

#[cfg(test)]
mod tests;
