use std::rc::Rc;
use std::time::Duration;

use fastnote_core::NoteId;
use fastnote_ui::ToastKind;
use slint::{ModelRc, Timer};

use super::super::runtime::Runtime;
use crate::notes::{NoteEditor, NoteRead, SavePlan, TreeState};

impl Runtime {
    pub(super) fn toggle_note_preview(self: &Rc<Self>) {
        if self.notes.borrow().preview {
            self.notes.borrow_mut().preview = false;
            let _ = self.with_ui(|ui| ui.shell.component().set_note_preview_mode(false));
            return;
        }
        if !self.defer_note_transition("note-preview".to_owned()) {
            self.show_note_preview();
        }
    }

    pub(super) fn show_note_preview(&self) {
        let Some(content) = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .map(|editor| editor.content.clone())
        else {
            return;
        };
        let (preview, warning) = match slint::StyledText::from_markdown(&content) {
            Ok(preview) => (preview, ""),
            Err(_) => (
                slint::StyledText::from_plain_text(&content),
                "当前预览暂不支持此格式，显示原文。",
            ),
        };
        self.notes.borrow_mut().preview = true;
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_preview_text(preview);
            window.set_note_preview_warning(warning.into());
            window.set_note_preview_mode(true);
        });
    }

    pub(super) fn note_visible(&self) -> bool {
        self.main_content_visible()
            && self.controller.borrow().shell_view().page_label == "小记"
            && self
                .with_ui(|ui| ui.shell.component().get_current_page() == "小记")
                .unwrap_or(false)
    }

    /// 仅可见页面读取目录和统计，不在隐藏时装载历史正文。
    pub(in crate::desktop) fn load_notes(&self) {
        if !self.ready.get() || !self.note_visible() {
            return;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self
            .worker
            .note_query(id, NoteRead::List(Default::default()))
            .is_ok()
        {
            self.notes.borrow_mut().list_request = id;
        } else {
            self.notice(ToastKind::Error, "小记读取队列不可用，请稍后重试。");
        }
        let tree_id = self.next_id.get();
        self.next_id.set(tree_id.saturating_add(1));
        if self.worker.note_query(tree_id, NoteRead::Tree).is_ok() {
            self.notes.borrow_mut().tree_request = tree_id;
        }
    }

    pub(super) fn open_note(self: &Rc<Self>, note: NoteId, select: bool) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        if select {
            self.notes.borrow_mut().select(Some(&note.to_string()));
            self.render_note_tree();
        }
        let current = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .is_some_and(|editor| editor.detail.note.id == note);
        if current && !self.note_protected() {
            self.notes.borrow_mut().preview = false;
            let _ = self.with_ui(|ui| ui.shell.component().set_note_preview_mode(false));
            return;
        }
        if self.defer_note_transition(format!("open:{note}")) {
            return;
        }
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self.worker.note_query(id, NoteRead::Detail(note)).is_ok() {
            let mut session = self.notes.borrow_mut();
            session.detail_request = id;
            session.pending_open = Some(note);
        } else {
            self.notice(ToastKind::Error, "小记读取队列不可用，请稍后重试。");
        }
    }

    pub(super) fn edit_note(self: &Rc<Self>, content: String) {
        if self.note_syncing.get() || !self.note_visible() {
            return;
        }
        if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
            editor.edit(content);
        }
        self.sync_note();
        // 编辑持续进行时仍按十秒周期保存，不因每次输入推迟计时。
        if !self.note_save_timer.running() {
            let weak = Rc::downgrade(self);
            self.note_save_timer.start(
                slint::TimerMode::SingleShot,
                Duration::from_secs(10),
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.flush_note();
                    }
                },
            );
        }
    }

    /// 冲刷当前草稿；文件版本变更时先刷新基线，不使用旧时间戳保存。
    pub(in crate::desktop) fn flush_note(self: &Rc<Self>) -> bool {
        self.note_save_timer.stop();
        if self.note_syncing.get() || self.notes.borrow().relocate_request.is_some() {
            return false;
        }
        let plan = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .map(NoteEditor::save_plan);
        match plan {
            None | Some(SavePlan::Clean) => true,
            Some(SavePlan::Waiting) => false,
            Some(SavePlan::Refresh(note)) => {
                self.refresh_note_metadata(note);
                false
            }
            Some(SavePlan::Invalid(error)) => {
                let message = match error {
                    fastnote_core::NoteError::TooLong => "小记不能超过 20,000 字符",
                    _ => "小记内容无效，请修改后重试。",
                };
                if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
                    editor.error = Some(message.to_owned());
                }
                self.sync_note();
                false
            }
            Some(SavePlan::Submit(command)) => {
                let id = self.next_id.get();
                self.next_id.set(id.saturating_add(1));
                if self.worker.note_execute(id, command).is_ok() {
                    if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
                        editor.pending = Some(id);
                    }
                } else if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
                    editor.error = Some("保存队列不可用，编辑内容已保留。".to_owned());
                }
                self.sync_note();
                false
            }
        }
    }

    pub(super) fn discard_note(self: &Rc<Self>) {
        let discarded = self
            .notes
            .borrow_mut()
            .editor
            .as_mut()
            .is_some_and(NoteEditor::discard);
        if !discarded {
            return;
        }
        self.note_save_timer.stop();
        self.sync_note();
        let action = self.notes.borrow_mut().after_save.take();
        if let Some(action) = action {
            self.resume_note_action(action);
        }
    }

    pub(super) fn resume_note_action(self: &Rc<Self>, action: String) {
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                if action == "note-preview" {
                    runtime.show_note_preview();
                } else if let Some(name) = action.strip_prefix("note-create:") {
                    runtime.create_note(name.to_owned());
                } else if let Some((id, name)) = action
                    .strip_prefix("note-rename:")
                    .and_then(|value| value.split_once(':'))
                {
                    runtime.rename_note_entry(id.to_owned(), name.to_owned());
                } else if let Some(id) = action.strip_prefix("note-delete:") {
                    runtime.delete_note_entry(id.to_owned());
                } else if let Some((id, destination)) = action
                    .strip_prefix("note-move:")
                    .and_then(|value| value.split_once(':'))
                {
                    runtime.move_note_entry(id.to_owned(), destination.to_owned());
                } else if let Some(id) = action
                    .strip_prefix("open:")
                    .and_then(|value| value.parse().ok())
                {
                    // 保存过程中左侧可能已经导航，打开请求不能把目录选择改回去。
                    runtime.open_note(id, false);
                } else {
                    runtime.dispatch_action(action);
                }
            }
        });
    }

    /// 草稿成功保存前保留要执行的动作，失败时不丢弃编辑内容。
    pub(in crate::desktop) fn defer_note_transition(self: &Rc<Self>, action: String) -> bool {
        if !self.note_protected() {
            return false;
        }
        self.notes.borrow_mut().after_save = Some(action);
        if self.flush_note() {
            self.notes.borrow_mut().after_save = None;
            return false;
        }
        true
    }

    /// 是否存在未保存、失败或正在刷新版本的正文。
    pub(in crate::desktop) fn note_protected(&self) -> bool {
        self.notes
            .borrow()
            .editor
            .as_ref()
            .is_some_and(NoteEditor::protected)
    }

    /// 清空右侧文件，但不改变独立的左侧目录选择。
    pub(super) fn clear_note_editor(&self) {
        let mut session = self.notes.borrow_mut();
        session.editor = None;
        session.preview = false;
        session.invalidate_open();
        drop(session);
        self.note_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_content("".into());
            window.set_note_has_selection(false);
            window.set_note_current_name("".into());
            window.set_note_error("".into());
            window.set_note_preview_mode(false);
            window.set_note_preview_text(Default::default());
        });
        self.note_syncing.set(false);
    }

    /// 回收重资源，保留有界目录历史和选中标识；迟到查询不能重新装载页面。
    pub(in crate::desktop) fn release_notes(&self) {
        if self.note_protected() {
            return;
        }
        self.note_save_timer.stop();
        self.clear_note_editor();
        let mut session = self.notes.borrow_mut();
        session.list_request = 0;
        session.tree_request = 0;
        session.tree_loaded = false;
        session.tree.clear();
        session.tree_state = TreeState::default();
        session.rename_target = None;
        drop(session);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_rows(ModelRc::default());
            window.set_note_selected_id("".into());
            window.set_note_selected_path("小记".into());
            window.set_note_can_back(false);
            window.set_note_can_forward(false);
            window.set_note_can_parent(false);
            window.set_note_rename_id("".into());
            window.set_note_rename_name("".into());
            window.set_note_rename_error("".into());
            window.set_note_create_open(false);
        });
    }

    /// 同步右侧草稿，不能覆盖左侧目录选中项。
    pub(super) fn sync_note(&self) {
        let session = self.notes.borrow();
        let Some(editor) = session.editor.as_ref() else {
            return;
        };
        let id = editor.detail.note.id.to_string();
        let name = session
            .tree
            .iter()
            .find(|entry| entry.id == id)
            .map_or("", |entry| entry.name.as_str())
            .to_owned();
        let content = editor.content.clone();
        let error = editor.error.clone().unwrap_or_default();
        let label = if editor.pending.is_some() {
            "正在保存…"
        } else if editor.dirty() {
            "未保存"
        } else {
            "已保存"
        };
        let created = editor
            .detail
            .note
            .created_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string();
        let updated = editor
            .detail
            .note
            .updated_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string();
        drop(session);
        self.note_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_has_selection(true);
            window.set_note_current_name(name.into());
            window.set_note_content(content.into());
            window.set_note_error(error.into());
            window.set_note_save_label(label.into());
            window.set_note_created_date(created.into());
            window.set_note_updated_date(updated.into());
        });
        self.note_syncing.set(false);
    }
}
