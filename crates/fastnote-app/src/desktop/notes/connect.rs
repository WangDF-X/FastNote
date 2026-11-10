use fastnote_ui::ToastKind;
use std::rc::Rc;

use super::super::runtime::Runtime;

impl Runtime {
    /// 连接小记页面回调；所有窗口访问仍经过当前 UI 会话。
    pub(in crate::desktop) fn connect_notes(self: &Rc<Self>) {
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.on_note_create_start({
                let weak = self.ui_target();
                move |id, folder| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.begin_note_create(&id, folder);
                    }
                }
            });
            window.on_note_create({
                let weak = self.ui_target();
                move |name| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.create_note(name.to_string());
                    }
                }
            });
            window.on_note_open({
                let weak = self.ui_target();
                move |id| {
                    if let (Some(runtime), Ok(id)) = (weak.upgrade(), id.parse()) {
                        runtime.open_note(id, true);
                    }
                }
            });
            window.on_note_select_entry({
                let weak = self.ui_target();
                move |id| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.select_note_entry(&id);
                    }
                }
            });
            window.on_note_navigate({
                let weak = self.ui_target();
                move |direction| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.navigate_note_tree(&direction);
                    }
                }
            });
            window.on_note_toggle_folder({
                let weak = self.ui_target();
                move |id| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.toggle_note_folder(&id);
                    }
                }
            });
            window.on_note_rename_start({
                let weak = self.ui_target();
                move |id| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.begin_note_rename(&id);
                    }
                }
            });
            window.on_note_rename_submit({
                let weak = self.ui_target();
                move |name| {
                    if let Some(runtime) = weak.upgrade() {
                        let id = runtime.notes.borrow().rename_target.clone();
                        if let Some(id) = id {
                            runtime.rename_note_entry(id, name.to_string());
                        }
                    }
                }
            });
            window.on_note_rename_cancel({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.cancel_note_rename();
                    }
                }
            });
            window.on_note_delete({
                let weak = self.ui_target();
                move |id| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.delete_note_entry(id.to_string());
                    }
                }
            });
            window.on_note_undo({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.undo_note_delete();
                    }
                }
            });
            window.on_note_move({
                let weak = self.ui_target();
                move |id, destination| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.move_note_entry(id.to_string(), destination.to_string());
                    }
                }
            });
            window.on_note_edit({
                let weak = self.ui_target();
                move |value| {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.edit_note(value.to_string());
                    }
                }
            });
            window.on_note_flush({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.flush_note();
                    }
                }
            });
            window.on_note_discard({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.discard_note();
                    }
                }
            });
            window.on_note_toggle_preview({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.toggle_note_preview();
                    }
                }
            });
            window.on_note_open_link({
                let weak = self.ui_target();
                move |url| {
                    if let Some(runtime) = weak.upgrade() {
                        if !fastnote_platform::open_web_link(&url) {
                            runtime.notice(ToastKind::Error, "无法打开链接，仅支持网页链接。");
                        }
                    }
                }
            });
        });
    }
}
