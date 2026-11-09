use std::rc::Rc;

use fastnote_core::{NoteCommand, NoteFileEntry, NoteId};
use fastnote_ui::ToastKind;

use super::super::runtime::Runtime;
use super::RelocateRequest;
use crate::notes::in_subtree;

impl Runtime {
    pub(super) fn create_note(self: &Rc<Self>, name: String) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        if fastnote_core::domain::validate_note_name(&name).is_err() {
            self.note_name_error(
                false,
                "文件名无效，请检查特殊字符、保留名称及末尾空格或句点。",
            );
            return;
        }
        let folder = self
            .with_ui(|ui| ui.shell.component().get_note_create_folder())
            .unwrap_or(false);
        if !folder && self.defer_note_transition(format!("note-create:{name}")) {
            return;
        }
        let Ok(id) = NoteId::try_from(uuid::Uuid::now_v7()) else {
            self.note_name_error(false, "无法生成标识，请重试。");
            return;
        };
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        let parent = self.notes.borrow().create_parent.clone();
        let command = if folder {
            NoteCommand::CreateFolder {
                id: id.to_string(),
                parent: parent.clone(),
                name: name.clone(),
            }
        } else {
            NoteCommand::CreateFile {
                id,
                parent: parent.clone(),
                name: name.clone(),
            }
        };
        if self.worker.note_execute(request, command).is_err() {
            self.note_name_error(false, "创建队列不可用，请重试。");
            return;
        }
        let mut session = self.notes.borrow_mut();
        session.invalidate_open();
        session.create_request = Some((
            request,
            NoteFileEntry {
                id: id.to_string(),
                parent,
                name,
                is_folder: folder,
            },
        ));
        drop(session);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_note_create_error("".into());
            ui.shell.component().set_note_create_busy(true);
        });
    }

    pub(super) fn rename_note_entry(self: &Rc<Self>, id: String, name: String) {
        if !self.note_visible()
            || self.notes.borrow().tree_busy()
            || self.notes.borrow().rename_target.as_deref() != Some(id.as_str())
        {
            return;
        }
        if fastnote_core::domain::validate_note_name(&name).is_err() {
            self.note_name_error(true, "名称无效，请检查特殊字符、保留名称及末尾空格或句点。");
            return;
        }
        let entry = self
            .notes
            .borrow()
            .tree
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        let Some(mut entry) = entry else {
            self.note_name_error(true, "文件或文件夹已不存在。");
            return;
        };
        if self.note_entry_affects_editor(&id)
            && self.defer_note_transition(format!("note-rename:{id}:{name}"))
        {
            return;
        }
        entry.name = name;
        self.relocate_note_entry(entry, true);
    }

    pub(super) fn move_note_entry(self: &Rc<Self>, id: String, destination: String) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        if self.note_entry_affects_editor(&id)
            && self.defer_note_transition(format!("note-move:{id}:{destination}"))
        {
            return;
        }
        let entry = self
            .notes
            .borrow()
            .tree
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        let Some(mut entry) = entry else {
            self.notice(ToastKind::Error, "文件或文件夹已不存在。");
            return;
        };
        entry.parent = if destination.is_empty() {
            None
        } else {
            Some(destination)
        };
        self.relocate_note_entry(entry, false);
    }

    fn relocate_note_entry(&self, entry: NoteFileEntry, renaming: bool) {
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        let command = NoteCommand::Relocate {
            id: entry.id.clone(),
            parent: entry.parent.clone(),
            name: entry.name.clone(),
        };
        if self.worker.note_execute(request, command).is_err() {
            if renaming {
                self.note_name_error(true, "重命名队列不可用，请重试。");
            } else {
                self.notice(ToastKind::Error, "移动队列不可用，请重试。");
            }
            return;
        }
        let mut session = self.notes.borrow_mut();
        session.invalidate_open();
        session.relocate_request = Some(RelocateRequest {
            request,
            entry,
            renaming,
        });
        drop(session);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_note_rename_error("".into());
            ui.shell.component().set_note_create_busy(true);
        });
    }

    pub(super) fn delete_note_entry(self: &Rc<Self>, id: String) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        if self.note_entry_affects_editor(&id)
            && self.defer_note_transition(format!("note-delete:{id}"))
        {
            return;
        }
        let entry = self
            .notes
            .borrow()
            .tree
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        let Some(entry) = entry else {
            self.notice(ToastKind::Error, "文件或文件夹已不存在。");
            return;
        };
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        let command = if entry.is_folder {
            NoteCommand::DeleteFolder {
                id: id.clone(),
                token: request,
            }
        } else if let Ok(note) = id.parse() {
            NoteCommand::Delete {
                id: Some(note),
                token: request,
            }
        } else {
            self.notice(ToastKind::Error, "小记标识无效。");
            return;
        };
        if self.worker.note_execute(request, command).is_err() {
            self.notice(ToastKind::Error, "删除队列不可用，请重试。");
            return;
        }
        let mut session = self.notes.borrow_mut();
        session.invalidate_open();
        session.delete_request = Some((request, id));
        drop(session);
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(true));
    }

    pub(super) fn undo_note_delete(&self) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        let Some(token) = self.notes.borrow().undo_token else {
            return;
        };
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        if self
            .worker
            .note_execute(id, NoteCommand::Undo { token })
            .is_err()
        {
            self.notice(ToastKind::Error, "撤销队列不可用，请重试。");
            return;
        }
        self.notes.borrow_mut().undo_request = Some(id);
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(true));
    }

    pub(super) fn note_entry_affects_editor(&self, id: &str) -> bool {
        let session = self.notes.borrow();
        session
            .editor
            .as_ref()
            .is_some_and(|editor| in_subtree(&session.tree, &editor.detail.note.id.to_string(), id))
    }

    pub(super) fn note_name_error(&self, renaming: bool, message: &str) {
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            if renaming {
                window.set_note_rename_error(message.into());
            } else {
                window.set_note_create_error(message.into());
            }
        });
    }
}
