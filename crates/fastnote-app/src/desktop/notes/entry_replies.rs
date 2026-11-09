use std::rc::Rc;
use std::time::Duration;

use fastnote_core::{NoteCommandResult, NoteError};
use fastnote_store::NoteStoreError;
use fastnote_ui::ToastKind;
use slint::Timer;

use super::super::runtime::Runtime;
use super::RelocateRequest;
use crate::notes::{in_subtree, NoteEditor};

impl Runtime {
    pub(super) fn note_created(self: &Rc<Self>, result: Result<NoteCommandResult, NoteStoreError>) {
        let pending = self.notes.borrow_mut().create_request.take();
        let Some((_, entry)) = pending else { return };
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
        match result {
            Ok(result) => {
                let _ = self.with_ui(|ui| {
                    ui.shell.component().set_note_create_name("".into());
                    ui.shell.component().set_note_create_open(false);
                });
                if self.note_visible() {
                    let mut session = self.notes.borrow_mut();
                    if !session.tree.iter().any(|existing| existing.id == entry.id) {
                        session.tree.push(entry.clone());
                    }
                    session.select(Some(&entry.id));
                    session.reveal_selection();
                    drop(session);
                    if let Some(detail) = result.detail {
                        if !self.note_protected() {
                            self.notes.borrow_mut().editor = Some(NoteEditor::new(detail));
                            self.notes.borrow_mut().preview = false;
                            let _ = self
                                .with_ui(|ui| ui.shell.component().set_note_preview_mode(false));
                            self.sync_note();
                        }
                    }
                    self.render_note_tree();
                }
                self.load_notes();
            }
            Err(error) => self.note_name_error(false, entry_error(&error, false)),
        }
    }

    pub(super) fn note_relocated(
        self: &Rc<Self>,
        result: Result<NoteCommandResult, NoteStoreError>,
    ) {
        let pending = self.notes.borrow_mut().relocate_request.take();
        let Some(RelocateRequest {
            entry, renaming, ..
        }) = pending
        else {
            return;
        };
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
        match result {
            Ok(_) => {
                if renaming {
                    self.notes.borrow_mut().rename_target = None;
                    let _ = self.with_ui(|ui| {
                        ui.shell.component().set_note_rename_id("".into());
                        ui.shell.component().set_note_rename_name("".into());
                        ui.shell.component().set_note_rename_error("".into());
                    });
                }
                let mut session = self.notes.borrow_mut();
                if let Some(existing) = session
                    .tree
                    .iter_mut()
                    .find(|existing| existing.id == entry.id)
                {
                    *existing = entry.clone();
                }
                session.reveal_selection();
                let refresh = session.editor.as_mut().and_then(|editor| {
                    if editor.detail.note.id.to_string() != entry.id {
                        return None;
                    }
                    editor.invalidate_version();
                    Some(editor.detail.note.id)
                });
                drop(session);
                self.render_note_tree();
                self.load_notes();
                if let Some(note) = refresh {
                    self.refresh_note_metadata(note);
                }
            }
            Err(error) if renaming => self.note_name_error(true, entry_error(&error, true)),
            Err(error) => self.notice(
                ToastKind::Error,
                if error.domain_error() == Some(NoteError::NameExists) {
                    "目标文件夹中已有同名文件或文件夹。"
                } else {
                    "移动失败，请检查目标和目录层级。"
                },
            ),
        }
    }

    pub(super) fn note_deleted(self: &Rc<Self>, result: Result<NoteCommandResult, NoteStoreError>) {
        let pending = self.notes.borrow_mut().delete_request.take();
        let Some((_, deleted_id)) = pending else {
            return;
        };
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
        match result {
            Ok(result) => {
                if self.note_entry_affects_editor(&deleted_id) {
                    self.clear_note_editor();
                }
                let mut session = self.notes.borrow_mut();
                let tree = session
                    .tree
                    .iter()
                    .filter(|entry| !in_subtree(&session.tree, &entry.id, &deleted_id))
                    .cloned()
                    .collect();
                session.replace_tree(tree);
                let ticket = result.undo;
                session.undo_token = ticket.map(|ticket| ticket.token);
                drop(session);
                self.render_note_tree();
                self.load_notes();
                let _ =
                    self.with_ui(|ui| ui.shell.component().set_note_undo_visible(ticket.is_some()));
                if let Some(ticket) = ticket {
                    let delay = (ticket.expires_at - chrono::Utc::now())
                        .to_std()
                        .unwrap_or(Duration::ZERO);
                    let weak = Rc::downgrade(self);
                    Timer::single_shot(delay, move || {
                        if let Some(runtime) = weak.upgrade() {
                            if runtime.notes.borrow().undo_token == Some(ticket.token) {
                                runtime.notes.borrow_mut().undo_token = None;
                                let _ = runtime.with_ui(|ui| {
                                    ui.shell.component().set_note_undo_visible(false)
                                });
                            }
                        }
                    });
                }
            }
            Err(_) => self.notice(ToastKind::Error, "删除失败，原有内容已保留。"),
        }
    }

    pub(super) fn note_restored(&self, result: Result<NoteCommandResult, NoteStoreError>) {
        let mut session = self.notes.borrow_mut();
        session.undo_request = None;
        session.undo_token = None;
        drop(session);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_note_create_busy(false);
            ui.shell.component().set_note_undo_visible(false);
        });
        match result {
            Ok(_) => self.load_notes(),
            Err(_) => self.notice(ToastKind::Error, "撤销失败或已过期。"),
        }
    }
}

fn entry_error(error: &NoteStoreError, renaming: bool) -> &'static str {
    match error.domain_error() {
        Some(NoteError::NameExists) => "同一目录已有同名文件或文件夹，请换一个名称。",
        Some(NoteError::InvalidName) => "名称无效，请修改后重试。",
        Some(NoteError::Conflict) => "目录最多三层，请选择其他位置。",
        Some(NoteError::NotFound) => "文件或目标文件夹已不存在，请重新选择。",
        _ if renaming => "重命名失败，请重试。",
        _ => "创建失败，请重试。",
    }
}
