use crate::{BannerSet, ToastQueue};

/// 主窗卸载后可移交给 app 的纯反馈状态；没有组件、模型、计时器或回调引用。
/// Toast 最多 32 条，Banner 最多四种；模态决策不在白名单中，必须阻止卸载。
#[derive(Default)]
pub struct RetainedFeedback {
    pub(super) toasts: ToastQueue,
    pub(super) banners: BannerSet,
}

impl RetainedFeedback {
    /// 冷驻留导航与活动态导航相同：清除旧页面 Toast，不重新执行任何动作。
    pub fn discard_route_toasts(&mut self) {
        self.toasts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BannerKind, BannerRequest, ToastKind, ToastRequest};

    #[test]
    fn test_feedback_moves_without_ui_and_preserves_toast_time_and_banner() {
        let mut feedback = RetainedFeedback::default();
        feedback.toasts.push(ToastRequest {
            kind: ToastKind::Success,
            message: "已保存".to_owned(),
            action: None,
        });
        feedback.toasts.tick(1000);
        feedback
            .banners
            .show(BannerRequest::new(BannerKind::Warning, "安全提示"));
        let mut restored = std::mem::take(&mut feedback);
        assert!(feedback.toasts.is_empty());
        assert!(feedback.banners.is_empty());
        assert!(!restored.toasts.tick(1999));
        assert!(restored.toasts.tick(1));
        assert_eq!(restored.banners.len(), 1);
    }

    #[test]
    fn test_cold_route_change_clears_toasts_without_invoking_commands() {
        let mut feedback = RetainedFeedback::default();
        feedback.toasts.push(ToastRequest {
            kind: ToastKind::Info,
            message: "提示".to_owned(),
            action: None,
        });
        feedback.discard_route_toasts();
        assert!(feedback.toasts.is_empty());
        let source = include_str!("handle.rs");
        let take = source
            .split_once("fn take_retained_feedback(")
            .unwrap()
            .1
            .split_once("fn restore_retained_feedback(")
            .unwrap()
            .0;
        assert!(take.contains("dialogs.borrow().is_open()"));
        assert!(!take.contains(".dispatch("));
    }
}
