use std::rc::Rc;

use fastnote_core::{NoteCommandResult, NoteId};
use fastnote_store::NoteStoreError;
use fastnote_ui::ToastKind;

use super::super::runtime::Runtime;
use crate::notes::{NoteEditor, NoteRead, NoteReadResult};

impl Runtime {
    /// 根据请求标识分发小记命令回包，不让重命名冒充新建或正文保存。
    pub(in crate::desktop) fn note_executed(
        self: &Rc<Self>,
        id: u64,
        result: Result<NoteCommandResult, NoteStoreError>,
    ) {
        let session = self.notes.borrow();
        let created = session
            .create_request
            .as_ref()
            .is_some_and(|(request, _)| *request == id);
        let relocated = session
            .relocate_request
            .as_ref()
            .is_some_and(|pending| pending.request == id);
        let deleted = session
            .delete_request
            .as_ref()
            .is_some_and(|(request, _)| *request == id);
        let restored = session.undo_request == Some(id);
        let saved = session
            .editor
            .as_ref()
            .is_some_and(|editor| editor.pending == Some(id));
        drop(session);
        if created {
            self.note_created(result);
        } else if relocated {
            self.note_relocated(result);
        } else if deleted {
            self.note_deleted(result);
        } else if restored {
            self.note_restored(result);
        } else if saved {
            self.note_saved(id, result);
        } else {
            tracing::warn!(request_id = id, "unexpected note command reply");
        }
    }

    fn note_saved(self: &Rc<Self>, id: u64, result: Result<NoteCommandResult, NoteStoreError>) {
        let detail = result.ok().and_then(|result| result.detail);
        let accepted = detail.is_some_and(|detail| {
            self.notes
                .borrow_mut()
                .editor
                .as_mut()
                .is_some_and(|editor| editor.acknowledge(id, detail))
        });
        if !accepted {
            if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
                editor.fail(id, "保存失败，编辑内容已保留。".to_owned());
            }
            self.sync_note();
            self.notice(ToastKind::Error, "小记保存失败，编辑内容已保留。");
            return;
        }
        self.sync_note();
        self.load_notes();
        if self.note_protected() {
            self.flush_note();
        } else {
            let action = self.notes.borrow_mut().after_save.take();
            if let Some(action) = action {
                self.resume_note_action(action);
            }
        }
    }

    /// 重命名/移动当前文件后刷新版本，始终保留右侧文件、内容和模式。
    pub(super) fn refresh_note_metadata(&self, note: NoteId) {
        if self.notes.borrow().metadata_request.is_some() {
            return;
        }
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        if self
            .worker
            .note_query(request, NoteRead::Detail(note))
            .is_err()
        {
            self.note_metadata_failed();
            return;
        }
        self.notes.borrow_mut().metadata_request = Some((request, note));
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(true));
    }

    fn note_metadata_failed(&self) {
        if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
            editor.error = Some("文件保存状态刷新失败，内容已保留，请重试。".to_owned());
        }
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
        self.sync_note();
    }

    /// 可见且属于当前请求的回包才可装载目录或正文。
    pub(in crate::desktop) fn note_queried(
        self: &Rc<Self>,
        id: u64,
        result: Result<NoteReadResult, NoteStoreError>,
    ) {
        let metadata = self.notes.borrow().metadata_request;
        if let Some((_, target)) = metadata.filter(|(request, _)| *request == id) {
            self.notes.borrow_mut().metadata_request = None;
            let detail = match result {
                Ok(NoteReadResult::Detail(Some(detail))) if detail.note.id == target => {
                    Some(*detail)
                }
                _ => None,
            };
            let accepted = detail.is_some_and(|detail| {
                self.notes
                    .borrow_mut()
                    .editor
                    .as_mut()
                    .is_some_and(|editor| editor.refresh_metadata(detail))
            });
            let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
            if accepted {
                self.sync_note();
                if self.note_protected() {
                    self.flush_note();
                } else {
                    let action = self.notes.borrow_mut().after_save.take();
                    if let Some(action) = action {
                        self.resume_note_action(action);
                    }
                }
            } else {
                self.note_metadata_failed();
            }
            return;
        }
        if self.notes.borrow().tree_request == id {
            self.notes.borrow_mut().tree_request = 0;
            if self.note_visible() {
                match result {
                    Ok(NoteReadResult::Tree(tree)) => {
                        let fresh = !self.notes.borrow().tree_loaded;
                        self.notes.borrow_mut().tree_loaded = true;
                        if fresh || self.notes.borrow().tree != tree {
                            self.notes.borrow_mut().replace_tree(tree);
                            self.render_note_tree();
                        }
                    }
                    _ => self.notice(ToastKind::Error, "小记目录读取失败，请重试。"),
                }
            }
            return;
        }
        let (list, is_detail, target) = {
            let mut session = self.notes.borrow_mut();
            if session.list_request == id {
                session.list_request = 0;
                (true, false, None)
            } else if session.detail_request == id {
                session.detail_request = 0;
                (false, true, session.pending_open.take())
            } else {
                return;
            }
        };
        if !self.note_visible() {
            return;
        }
        match result {
            Ok(NoteReadResult::List(page)) if list => {
                let _ = self.with_ui(|ui| ui.shell.component().set_note_total(page.total as i32));
            }
            Ok(NoteReadResult::Detail(Some(detail)))
                if is_detail && Some(detail.note.id) == target =>
            {
                if self.note_protected() {
                    return;
                }
                self.notes.borrow_mut().editor = Some(NoteEditor::new(*detail));
                self.notes.borrow_mut().preview = false;
                let _ = self.with_ui(|ui| ui.shell.component().set_note_preview_mode(false));
                self.sync_note();
            }
            _ => self.notice(ToastKind::Error, "小记读取失败，请重试。"),
        }
    }
}
