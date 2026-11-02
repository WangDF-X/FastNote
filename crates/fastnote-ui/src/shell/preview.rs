//! debug 预览条的数据源（SHELL-01 §… 仅开发期使用）。
//!
//! 这里只是把验收用的假数据翻译成真实的反馈请求，不参与产品逻辑。
//! 业务模块接入后整个文件连同预览条一起删除。

use std::rc::Rc;

use crate::feedback::banner::{BannerKind, BannerRequest};
use crate::feedback::dialog::{DialogKind, DialogRequest};
use crate::feedback::toast::{ToastAction, ToastKind, ToastRequest};

use super::Shell;

impl Shell {
    /// 按预览项标识触发对应的反馈组件。
    pub(super) fn preview(self: &Rc<Self>, item: &str) {
        match item {
            "toast-success" => self.show_toast(plain(ToastKind::Success, "已创建任务")),
            "toast-info" => self.show_toast(plain(ToastKind::Info, "已切换到状态栏模式")),
            "toast-warning" => self.show_toast(plain(
                ToastKind::Warning,
                "前置任务尚未完成，已按你的选择开始",
            )),
            "toast-error" => self.show_toast(plain(ToastKind::Error, "保存失败，请重试")),
            "toast-action" => self.show_toast(ToastRequest {
                kind: ToastKind::Success,
                message: "已创建任务".to_owned(),
                action: Some(ToastAction {
                    label: "查看".to_owned(),
                    command: "navigate:日历?selected=2026-10-23".to_owned(),
                }),
            }),
            "toast-burst" => {
                for index in 1..=4 {
                    self.show_toast(plain(ToastKind::Info, &format!("第 {index} 条提示")));
                }
            }
            "dialog-normal" => self.show_dialog(DialogRequest::new(
                "归档计划？",
                "仍有 3 个未完成任务；归档后从今日与日历隐藏，数据保留。",
                "归档",
                DialogKind::Normal,
            )),
            "dialog-destructive" => self.show_dialog(
                DialogRequest::new(
                    "删除任务？",
                    "「写周报」将被永久删除，无法恢复。",
                    "删除",
                    DialogKind::Destructive,
                )
                .with_detail("包含 2 个子任务，将一并删除。"),
            ),
            "dialog-queue" => {
                self.show_dialog(DialogRequest::new(
                    "标记为完成？",
                    "仍有 2 个子任务未完成；父任务会显示为已完成。",
                    "仍要完成",
                    DialogKind::Normal,
                ));
                self.show_dialog(DialogRequest::new(
                    "永久删除计划？",
                    "计划及 5 个任务将被删除，无法恢复。",
                    "永久删除",
                    DialogKind::Destructive,
                ));
            }
            "banner-error" => self.show_banner(
                BannerRequest::new(
                    BannerKind::Error,
                    "数据库文件异常，已切换到只读模式。请从备份恢复。",
                )
                .with_action("打开数据管理", "navigate:设置?section=backup"),
            ),
            "banner-warning" => self.show_banner(BannerRequest::new(
                BannerKind::Warning,
                "磁盘空间不足，修改可能无法保存。",
            )),
            "banner-info" => self.show_banner(BannerRequest::new(
                BannerKind::Info,
                "当前桌面环境不支持全局快捷键。",
            )),
            "banner-readonly" => self.show_banner(BannerRequest::new(
                BannerKind::ReadOnly,
                "计划已归档，仅供查看。",
            )),
            "banner-priority" => {
                // 先放信息条，再放警告条：只显示警告，关掉后信息条上浮。
                self.show_banner(BannerRequest::new(
                    BannerKind::Info,
                    "这是被更高优先级盖住的信息条。",
                ));
                self.show_banner(BannerRequest::new(
                    BannerKind::Warning,
                    "磁盘空间不足，修改可能无法保存。",
                ));
            }
            "banner-clear" => self.clear_banners(),
            "route-calendar" => self.dispatch("navigate:日历?selected=2026-10-23".to_owned()),
            "route-notes" => self.dispatch(
                "navigate:小记?selected=019a0fff-aaaa-7000-8000-000000000003".to_owned(),
            ),
            "route-journal" => self.dispatch("navigate:日记?date=2026-10-24".to_owned()),
            "route-plans" => self.dispatch("navigate:计划板?tab=archived".to_owned()),
            "route-detail" => self.dispatch(
                "navigate:计划详情?plan_id=019a0fff-aaaa-7000-8000-000000000001&focus=019a0fff-aaaa-7000-8000-000000000002".to_owned(),
            ),
            "route-settings" => self.dispatch("navigate:设置?section=backup".to_owned()),
            "route-invalid" => self.dispatch("navigate:日历?selected=2026-02-29".to_owned()),
            _ => {}
        }
    }
}

fn plain(kind: ToastKind, message: &str) -> ToastRequest {
    ToastRequest {
        kind,
        message: message.to_owned(),
        action: None,
    }
}
