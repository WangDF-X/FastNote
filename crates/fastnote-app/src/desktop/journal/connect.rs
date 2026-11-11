use std::rc::Rc;

use fastnote_ui::ToastKind;

use super::super::runtime::Runtime;

impl Runtime {
    /// 连接日记页面回调；所有窗口访问仍经过当前 UI 会话。
    pub(in crate::desktop) fn connect_journal(self: &Rc<Self>) {
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.on_journal_open({
                let weak = self.ui_target();
                move |date| {
                    if let (Some(runtime), Ok(date)) = (weak.upgrade(), date.parse()) {
                        runtime.open_journal(date);
                    }
                }
            });
            window.on_journal_navigate({
                let weak = self.ui_target();
                move |delta| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.journal_navigate(delta);
                    }
                }
            });
            window.on_journal_shift({
                let weak = self.ui_target();
                move |delta| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.shift_journal_range(delta);
                    }
                }
            });
            window.on_journal_view_mode({
                let weak = self.ui_target();
                move |mode| {
                    if let Some(runtime) = weak.upgrade() {
                        let view = if mode == 1 {
                            super::JournalView::Week
                        } else {
                            super::JournalView::Month
                        };
                        runtime.set_journal_view(view);
                    }
                }
            });
            window.on_journal_today({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.journal_today();
                    }
                }
            });
            window.on_journal_new_today({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.journal_today();
                    }
                }
            });
            window.on_journal_close({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.close_journal();
                    }
                }
            });
            window.on_journal_edit({
                let weak = self.ui_target();
                move |value| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.edit_journal(value.to_string());
                    }
                }
            });
            window.on_journal_flush({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.flush_journal();
                    }
                }
            });
            window.on_journal_toggle_preview({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.toggle_journal_preview();
                    }
                }
            });
            window.on_journal_retry({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.retry_journal();
                    }
                }
            });
            window.on_journal_open_link({
                let weak = self.ui_target();
                move |url| {
                    if let Some(runtime) = weak.upgrade() {
                        if !fastnote_platform::open_web_link(&url) {
                            runtime.notice(ToastKind::Error, "无法打开链接，仅支持网页链接。");
                        }
                    }
                }
            });
            window.on_journal_add_image({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.pick_journal_image();
                    }
                }
            });
            window.on_journal_confirm_image({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.confirm_journal_image();
                    }
                }
            });
            window.on_journal_cancel_image({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.cancel_journal_image();
                    }
                }
            });
            window.on_journal_viewer_open({
                let weak = self.ui_target();
                move |index| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.open_journal_viewer(index);
                    }
                }
            });
            window.on_journal_viewer_prev({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.step_journal_viewer(-1);
                    }
                }
            });
            window.on_journal_viewer_next({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.step_journal_viewer(1);
                    }
                }
            });
            window.on_journal_viewer_close({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.close_journal_viewer();
                    }
                }
            });
            window.on_journal_viewer_delete({
                let weak = self.ui_target();
                move |id| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.remove_journal_image(id.to_string());
                    }
                }
            });
            window.on_journal_image_delete({
                let weak = self.ui_target();
                move |id| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.remove_journal_image(id.to_string());
                    }
                }
            });
        });
    }
}
