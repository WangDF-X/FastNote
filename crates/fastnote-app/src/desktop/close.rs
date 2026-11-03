use std::rc::Rc;

use fastnote_core::{CloseBehavior, DesktopSetting};
use fastnote_ui::{DialogKind, DialogRequest, ToastKind};

use super::presentation::minimize_to_ball;
use super::runtime::{Completion, Runtime};
use crate::tasks::Transition;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClosePlan {
    Hide,
    Exit,
}

fn close_plan(explicit_exit: bool, behavior: CloseBehavior, has_entry: bool) -> ClosePlan {
    if explicit_exit || behavior == CloseBehavior::Quit || !has_entry {
        ClosePlan::Exit
    } else {
        ClosePlan::Hide
    }
}

impl Runtime {
    pub(super) fn request_close(self: &Rc<Self>, explicit_exit: bool) {
        if self.finished.get() || self.closing.get() {
            return;
        }
        if self.shell.component().get_dialog_visible() {
            return;
        }
        if self.busy.get() {
            self.notice(ToastKind::Info, "正在保存设置，请稍候再关闭。");
            return;
        }
        if self.defer_task_transition(Transition::Close {
            exit: explicit_exit,
        }) {
            return;
        }
        let plan = close_plan(
            explicit_exit,
            self.settings.borrow().close_behavior,
            self.entry.get().is_some(),
        );
        if !self.ready.get() {
            self.finish_exit();
            return;
        }
        self.begin_close(plan);
    }

    /// 系统最小化切回球，不把关闭设置 `quit` 误当作退出命令。
    pub(super) fn request_minimize(self: &Rc<Self>) {
        if !minimize_to_ball(self.presentation.get(), self.entry.get())
            || self.finished.get()
            || self.closing.get()
            || self.busy.get()
        {
            return;
        }
        if !self.ready.get() {
            self.show_main();
            return;
        }
        if self.defer_task_transition(Transition::Minimize) {
            return;
        }
        self.begin_close(ClosePlan::Hide);
    }

    fn begin_close(self: &Rc<Self>, plan: ClosePlan) {
        self.hide_menu();
        self.geometry_timer.stop();
        if self.capture_main().is_err() {
            self.close_failed(plan == ClosePlan::Exit, "无法读取窗口位置，暂未关闭。", "");
            return;
        }
        if self.capture_ball_position().is_err() {
            self.close_failed(plan == ClosePlan::Exit, "无法读取入口位置，暂未关闭。", "");
            return;
        }
        let changes = self.close_changes();
        let exit = plan == ClosePlan::Exit;
        if self.queue(changes, Completion::Close { exit }, exit) {
            self.closing.set(true);
            self.set_busy(true);
        } else if plan == ClosePlan::Hide {
            // 系统已最小化时也必须恢复主窗，让用户看见保存队列错误。
            self.show_main();
        }
    }

    pub(super) fn complete_close(self: &Rc<Self>, exit: bool, changes: &[DesktopSetting]) {
        // 保存等待期间仍可能收到系统拖动/最大化；切换呈现前重新核对最终矩形。
        if self.capture_main().is_err() || self.capture_ball_position().is_err() {
            self.closing.set(false);
            self.set_busy(false);
            self.close_failed(exit, "无法读取最终窗口位置，暂未关闭。", "");
            return;
        }
        if !matches_final_geometry(
            changes,
            self.geometry.get(),
            self.ball_position.borrow().as_ref(),
        ) {
            if !self.queue(self.close_changes(), Completion::Close { exit }, exit) {
                self.closing.set(false);
                self.set_busy(false);
                self.close_failed(exit, "最终窗口位置未能保存，暂未关闭。", "");
            }
            return;
        }
        self.closing.set(false);
        self.set_busy(false);
        self.preference_repair.set(false);
        if exit {
            self.finish_exit();
        } else if self.hide_to_entry().is_err() {
            self.close_failed(false, "无法切换到桌面入口，请在主窗口重试。", "");
        }
    }

    fn close_changes(&self) -> Vec<DesktopSetting> {
        let mut changes = Vec::new();
        if let Some(geometry) = self.geometry.get() {
            changes.push(DesktopSetting::MainGeometry(Some(geometry)));
        }
        if let Some(position) = self.ball_position.borrow().clone() {
            changes.push(DesktopSetting::BallPosition(Some(position)));
        }
        if self.preference_repair.get() {
            changes.push(DesktopSetting::Theme(self.settings.borrow().theme));
            changes.push(DesktopSetting::EntryMode(self.settings.borrow().entry_mode));
            changes.push(DesktopSetting::CloseBehavior(
                self.settings.borrow().close_behavior,
            ));
        }
        changes
    }

    pub(super) fn close_failed(self: &Rc<Self>, exit: bool, message: &str, log_id: &str) {
        self.show_main();
        self.notice(ToastKind::Error, message);
        if exit && (!self.force_allowed.get() || !self.shell.component().get_dialog_visible()) {
            self.force_allowed.set(true);
            self.shell.show_dialog(DialogRequest::new("退出准备未完成，仍要退出？",
                "窗口配置保存或数据库收尾未能完成。仍然退出可能无法恢复本次位置或入口配置变化；不会重建或覆盖数据文件。",
                "仍然退出", DialogKind::Destructive).with_detail(log_id).with_command("desktop:force-quit"));
        }
    }
}

fn matches_final_geometry(
    changes: &[DesktopSetting],
    geometry: Option<fastnote_core::WindowGeometry>,
    ball: Option<&fastnote_core::BallPosition>,
) -> bool {
    let saved_geometry = changes.iter().find_map(|change| match change {
        DesktopSetting::MainGeometry(geometry) => *geometry,
        _ => None,
    });
    let saved_ball = changes.iter().find_map(|change| match change {
        DesktopSetting::BallPosition(ball) => ball.as_ref(),
        _ => None,
    });
    saved_geometry == geometry && saved_ball == ball
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_close_policy_and_missing_entry_never_leave_an_invisible_process() {
        assert_eq!(
            close_plan(false, CloseBehavior::Entry, true),
            ClosePlan::Hide
        );
        assert_eq!(
            close_plan(false, CloseBehavior::Quit, true),
            ClosePlan::Exit
        );
        assert_eq!(
            close_plan(true, CloseBehavior::Entry, true),
            ClosePlan::Exit
        );
        assert_eq!(
            close_plan(false, CloseBehavior::Entry, false),
            ClosePlan::Exit
        );
    }

    #[test]
    fn test_close_reflushes_geometry_changed_while_waiting_for_storage() {
        let geometry = fastnote_core::WindowGeometry {
            x: 10.0,
            y: 20.0,
            w: 1280.0,
            h: 800.0,
            maximized: false,
        };
        let ball = fastnote_core::BallPosition {
            x: 10.0,
            y: 20.0,
            monitor: "fake".to_owned(),
        };
        let changes = [
            DesktopSetting::MainGeometry(Some(geometry)),
            DesktopSetting::BallPosition(Some(ball.clone())),
        ];
        assert!(matches_final_geometry(
            &changes,
            Some(geometry),
            Some(&ball)
        ));
        assert!(!matches_final_geometry(
            &changes,
            Some(fastnote_core::WindowGeometry {
                x: 30.0,
                ..geometry
            }),
            Some(&ball)
        ));
        assert!(!matches_final_geometry(&changes, Some(geometry), None));
        assert!(matches_final_geometry(&[], None, None));
    }
}
