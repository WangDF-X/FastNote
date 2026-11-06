use std::rc::Rc;

use fastnote_core::EntryMode;
use fastnote_ui::ToastKind;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;

use crate::desktop::presentation::Presentation;
use crate::desktop::runtime::Runtime;

/// 已有可访问主窗或入口可保留重试；全部不可达时不能留下不可见进程。
fn must_exit_after_show_failure(main_reachable: bool, entry_reachable: bool) -> bool {
    !main_reachable && !entry_reachable
}

fn entry_can_recover(
    mode: Option<EntryMode>,
    view: Presentation,
    ball_visible: bool,
    has_tray: bool,
) -> bool {
    match mode {
        Some(EntryMode::Ball) => view == Presentation::Ball && ball_visible,
        Some(EntryMode::Tray) => has_tray,
        None => false,
    }
}

impl Runtime {
    /// ARCH-01 §3.3：仅数据/入口故障按需打开错误界面，不作为正常隐藏预热。
    pub(in crate::desktop) fn show_recovery_main(self: &Rc<Self>, phase: &'static str) {
        if let Err(error) = self.try_show_main() {
            tracing::error!(phase, error_kind = ?error, "required recovery interface unavailable");
            if !self.exit_if_unreachable(phase) {
                self.notice(
                    ToastKind::Error,
                    "错误界面暂时无法显示，请通过现有入口重试。",
                );
            }
        }
    }

    /// 恢复判定读取实际窗口/当前入口；已登记但未恢复的球不能充当可访问入口。
    pub(in crate::desktop) fn exit_if_unreachable(&self, phase: &'static str) -> bool {
        let ball_visible = self.presentation.get() == Presentation::Ball
            && self.ball.window().is_visible()
            && self
                .ball
                .window()
                .with_winit_window(|window| window.is_visible() != Some(false))
                .unwrap_or(false);
        let entry_reachable = entry_can_recover(
            self.entry.get(),
            self.presentation.get(),
            ball_visible,
            self.tray.borrow().is_some(),
        );
        if must_exit_after_show_failure(self.main_surface_reachable(), entry_reachable) {
            tracing::error!(
                phase,
                error_kind = "no_reachable_surface",
                "main and entry unavailable"
            );
            eprintln!(
                "FastNote：主窗口和桌面入口均不可用，应用已退出。请重新启动重试；数据文件未重建。"
            );
            self.finish_exit();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failed_mandatory_recovery_exits_only_when_main_and_entry_are_unreachable() {
        assert!(must_exit_after_show_failure(false, false));
        assert!(!must_exit_after_show_failure(true, false));
        assert!(!must_exit_after_show_failure(false, true));
        assert!(!must_exit_after_show_failure(true, true));
    }

    #[test]
    fn test_storage_and_entry_failures_use_explicit_recovery_without_querying_pages() {
        let lifecycle = include_str!("../lifecycle.rs");
        for phase in [
            "storage_error_recovery",
            "startup_entry_recovery",
            "preference_entry_recovery",
        ] {
            assert!(
                lifecycle.contains(&format!("self.show_recovery_main(\"{phase}\")")),
                "lifecycle.rs: {phase}"
            );
        }
        let source = include_str!("recovery.rs")
            .split_once("#[cfg(test)]")
            .unwrap()
            .0;
        assert!(
            source.contains("self.try_show_main()"),
            "lifecycle/recovery.rs"
        );
        assert!(
            source.contains("self.finish_exit();"),
            "lifecycle/recovery.rs"
        );
        assert!(!source.contains("request_today("), "lifecycle/recovery.rs");
    }

    #[test]
    fn test_registered_but_hidden_or_unrestored_ball_is_not_a_recovery_entry() {
        assert!(entry_can_recover(
            Some(EntryMode::Ball),
            Presentation::Ball,
            true,
            false
        ));
        for (view, visible) in [
            (Presentation::Hidden, false),
            (Presentation::Hidden, true),
            (Presentation::Ball, false),
            (Presentation::Main, false),
        ] {
            assert!(!entry_can_recover(
                Some(EntryMode::Ball),
                view,
                visible,
                true
            ));
        }
    }

    #[test]
    fn test_only_current_live_tray_can_keep_failed_show_recoverable() {
        assert!(entry_can_recover(
            Some(EntryMode::Tray),
            Presentation::Hidden,
            false,
            true
        ));
        assert!(!entry_can_recover(
            Some(EntryMode::Tray),
            Presentation::Hidden,
            false,
            false
        ));
        assert!(!entry_can_recover(None, Presentation::Ball, true, true));
    }

    #[test]
    fn test_normal_failed_show_and_required_recovery_check_actual_surfaces_without_creating_ui() {
        let source = include_str!("recovery.rs")
            .split_once("#[cfg(test)]")
            .unwrap()
            .0;
        assert!(
            source.contains("self.main_surface_reachable()"),
            "lifecycle/recovery.rs"
        );
        assert!(
            source.contains("self.ball.window().is_visible()"),
            "lifecycle/recovery.rs"
        );
        assert!(
            source.contains("self.tray.borrow().is_some()"),
            "lifecycle/recovery.rs"
        );
        let show = include_str!("../presentation/mod.rs")
            .split_once("fn show_main(")
            .unwrap()
            .1
            .split_once("fn try_show_main(")
            .unwrap()
            .0;
        assert!(
            show.contains("self.exit_if_unreachable(\"main_show_failure\")"),
            "presentation/mod.rs"
        );
    }
}
