use std::rc::Rc;

use fastnote_ui::ToastKind;

use crate::desktop::runtime::Runtime;

/// 已有可访问主窗或入口可保留重试；全部不可达时不能留下不可见进程。
fn must_exit_after_show_failure(main_visible: bool, has_entry: bool) -> bool {
    !main_visible && !has_entry
}

impl Runtime {
    /// ARCH-01 §3.3：仅数据/入口故障按需打开错误界面，不作为正常隐藏预热。
    pub(in crate::desktop) fn show_recovery_main(self: &Rc<Self>, phase: &'static str) {
        if let Err(error) = self.try_show_main() {
            tracing::error!(phase, error_kind = ?error, "required recovery interface unavailable");
            if must_exit_after_show_failure(self.main_content_visible(), self.entry.get().is_some())
            {
                eprintln!("FastNote：主窗口和桌面入口均不可用，应用已退出。请重新启动重试；数据文件未重建。");
                self.finish_exit();
            } else {
                self.notice(
                    ToastKind::Error,
                    "错误界面暂时无法显示，请通过现有入口重试。",
                );
            }
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
}
