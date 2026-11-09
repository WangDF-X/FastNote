use std::rc::Rc;
use std::time::Duration;

use fastnote_core::{NoteCommand, NoteCommandResult, NoteError, NoteFileEntry, NoteId};
use fastnote_store::NoteStoreError;
use fastnote_ui::{NoteRowData, ToastKind};
use slint::{ModelRc, Timer, VecModel};

use super::runtime::Runtime;
use crate::notes::{NoteEditor, NoteRead, NoteReadResult, SavePlan, TreeState};

#[derive(Default)]
pub(super) struct NoteSession {
    editor: Option<NoteEditor>,
    list_request: u64,
    detail_request: u64,
    pending_open: Option<NoteId>,
    after_save: Option<String>,
    preview: bool,
    create_request: Option<u64>,
    tree_request: u64,
    tree: Vec<NoteFileEntry>,
    tree_state: TreeState,
    target_folder: Option<String>,
    rename_request: Option<NoteId>,
    delete_request: Option<(u64, String)>,
    undo_request: Option<u64>,
    undo_token: Option<u64>,
    move_request: Option<(u64, Option<NoteId>)>,
}

impl Runtime {
    pub(super) fn connect_notes(self: &Rc<Self>) {
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
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
                        runtime.open_note(id);
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
            window.on_note_select_root({
                let weak = self.ui_target();
                move || {
                    if let Some(runtime) = weak.upgrade() {
                        runtime.notes.borrow_mut().target_folder = None;
                        runtime.render_note_tree();
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
        });
    }

    fn move_note_entry(self: &Rc<Self>, id: String, destination: String) {
        if !self.note_visible() || self.notes.borrow().move_request.is_some() {
            return;
        }
        if self.defer_note_transition(format!("note-move:{id}:{destination}")) {
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
        let selected = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .is_some_and(|editor| editor.detail.note.id.to_string() == id);
        let selected_id = if selected { id.parse().ok() } else { None };
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        let command = NoteCommand::Relocate {
            id,
            parent: if destination.is_empty() {
                None
            } else {
                Some(destination)
            },
            name: entry.name,
        };
        if self.worker.note_execute(request, command).is_err() {
            self.notice(ToastKind::Error, "移动队列不可用，请重试。");
            return;
        }
        self.notes.borrow_mut().move_request = Some((request, selected_id));
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(true));
    }

    fn delete_note_entry(self: &Rc<Self>, id: String) {
        if !self.note_visible() || self.notes.borrow().delete_request.is_some() {
            return;
        }
        if self.defer_note_transition(format!("note-delete:{id}")) {
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
        self.notes.borrow_mut().delete_request = Some((request, id));
        let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(true));
    }

    fn undo_note_delete(&self) {
        let token = self.notes.borrow().undo_token;
        let Some(token) = token else {
            return;
        };
        if self.notes.borrow().undo_request.is_some() {
            return;
        }
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
    }

    fn begin_note_rename(&self, id: &str) {
        let entry = self
            .notes
            .borrow()
            .tree
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        if let Some(entry) = entry {
            let _ = self.with_ui(|ui| {
                let window = ui.shell.component();
                window.set_note_rename_id(entry.id.into());
                window.set_note_create_name(entry.name.into());
                window.set_note_create_error("".into());
                window.set_note_create_open(true);
            });
        }
    }

    fn toggle_note_folder(&self, id: &str) {
        let mut session = self.notes.borrow_mut();
        session.target_folder = Some(id.to_owned());
        let NoteSession {
            tree, tree_state, ..
        } = &mut *session;
        tree_state.toggle(tree, id);
        drop(session);
        self.render_note_tree();
    }

    fn render_note_tree(&self) {
        let session = self.notes.borrow();
        let rows: Vec<_> = session
            .tree_state
            .visible(&session.tree)
            .into_iter()
            .map(|(entry, depth, expanded)| NoteRowData {
                id: entry.id.clone().into(),
                title: entry.name.clone().into(),
                preview: "".into(),
                date: "".into(),
                is_folder: entry.is_folder,
                expanded,
                depth: depth as i32,
            })
            .collect();
        let total = session.tree.iter().filter(|entry| !entry.is_folder).count() as i32;
        let target = session
            .target_folder
            .as_ref()
            .and_then(|id| session.tree.iter().find(|e| &e.id == id && e.is_folder))
            .map_or("根目录", |entry| entry.name.as_str())
            .to_owned();
        drop(session);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_note_total(total);
            ui.shell.component().set_note_target_folder(target.into());
            ui.shell
                .component()
                .set_note_rows(ModelRc::new(VecModel::from(rows)));
        });
    }

    fn create_note(self: &Rc<Self>, name: String) {
        if !self.note_visible() || self.notes.borrow().create_request.is_some() {
            return;
        }
        if fastnote_core::domain::validate_note_name(&name).is_err() {
            let _ = self.with_ui(|ui| {
                ui.shell.component().set_note_create_error(
                    "文件名无效：请检查特殊字符、保留名称及末尾空格或句点。".into(),
                )
            });
            return;
        }
        let folder_mode = self
            .with_ui(|ui| ui.shell.component().get_note_create_folder())
            .unwrap_or(false);
        let rename_id = self
            .with_ui(|ui| ui.shell.component().get_note_rename_id().to_string())
            .unwrap_or_default();
        if (!folder_mode || !rename_id.is_empty())
            && self.defer_note_transition(format!("note-create:{name}"))
        {
            return;
        }
        let Ok(note_id) = NoteId::try_from(uuid::Uuid::now_v7()) else {
            self.notice(ToastKind::Error, "无法生成小记标识，请重试。");
            return;
        };
        let request = self.next_id.get();
        self.next_id.set(request.saturating_add(1));
        let parent = self.notes.borrow().target_folder.clone();
        let command = if !rename_id.is_empty() {
            let entry = self
                .notes
                .borrow()
                .tree
                .iter()
                .find(|entry| entry.id == rename_id)
                .cloned();
            let Some(entry) = entry else {
                self.notice(ToastKind::Error, "文件或文件夹已不存在。");
                return;
            };
            let selected = self
                .notes
                .borrow()
                .editor
                .as_ref()
                .is_some_and(|editor| editor.detail.note.id.to_string() == entry.id);
            self.notes.borrow_mut().rename_request = if selected {
                entry.id.parse().ok()
            } else {
                None
            };
            NoteCommand::Relocate {
                id: entry.id,
                parent: entry.parent,
                name,
            }
        } else if folder_mode {
            NoteCommand::CreateFolder {
                id: note_id.to_string(),
                parent,
                name,
            }
        } else {
            NoteCommand::CreateFile {
                id: note_id,
                parent,
                name,
            }
        };
        if self.worker.note_execute(request, command).is_err() {
            self.notice(ToastKind::Error, "小记创建队列不可用，请重试。");
            return;
        }
        self.notes.borrow_mut().create_request = Some(request);
        let _ = self.with_ui(|ui| {
            ui.shell.component().set_note_create_error("".into());
            ui.shell.component().set_note_create_busy(true);
        });
    }

    fn toggle_note_preview(self: &Rc<Self>) {
        if self.notes.borrow().preview {
            self.notes.borrow_mut().preview = false;
            let _ = self.with_ui(|ui| ui.shell.component().set_note_preview_mode(false));
            return;
        }
        if self.defer_note_transition("note-preview".to_owned()) {
            return;
        }
        self.show_note_preview();
    }

    fn show_note_preview(&self) {
        let Some(content) = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .map(|e| e.content.clone())
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

    fn note_visible(&self) -> bool {
        self.main_content_visible()
            && self.controller.borrow().shell_view().page_label == "小记"
            && self
                .with_ui(|ui| ui.shell.component().get_current_page() == "小记")
                .unwrap_or(false)
    }

    pub(super) fn load_notes(&self) {
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

    fn open_note(self: &Rc<Self>, note: NoteId) {
        if !self.note_visible() {
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

    fn edit_note(self: &Rc<Self>, content: String) {
        if self.note_syncing.get() || !self.note_visible() {
            return;
        }
        if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
            editor.edit(content);
        }
        self.sync_note();
        // 编辑持续进行时也按周期保存，而不是每次输入都重置计时。
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

    pub(super) fn flush_note(self: &Rc<Self>) -> bool {
        self.note_save_timer.stop();
        if self.note_syncing.get() {
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
            Some(SavePlan::Invalid(error)) => {
                let message = match error {
                    fastnote_core::NoteError::Empty => "请输入内容",
                    _ => "小记不能超过 20,000 字符",
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

    fn discard_note(self: &Rc<Self>) {
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
        if let Some(action) = self.notes.borrow_mut().after_save.take() {
            self.resume_note_action(action);
        }
    }

    fn resume_note_action(self: &Rc<Self>, action: String) {
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::ZERO, move || {
            if let Some(runtime) = weak.upgrade() {
                if action == "note-preview" {
                    runtime.show_note_preview();
                } else if let Some(name) = action.strip_prefix("note-create:") {
                    runtime.create_note(name.to_owned());
                } else if let Some(id) = action.strip_prefix("note-delete:") {
                    runtime.delete_note_entry(id.to_owned());
                } else if let Some((id, destination)) = action
                    .strip_prefix("note-move:")
                    .and_then(|s| s.split_once(':'))
                {
                    runtime.move_note_entry(id.to_owned(), destination.to_owned());
                } else if let Some(id) = action.strip_prefix("open:").and_then(|s| s.parse().ok()) {
                    runtime.open_note(id);
                } else {
                    runtime.dispatch_action(action);
                }
            }
        });
    }

    pub(super) fn defer_note_transition(self: &Rc<Self>, action: String) -> bool {
        let protected = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .is_some_and(NoteEditor::protected);
        if !protected {
            return false;
        }
        self.notes.borrow_mut().after_save = Some(action);
        if self.flush_note() {
            self.notes.borrow_mut().after_save = None;
            return false;
        }
        true
    }

    pub(super) fn note_protected(&self) -> bool {
        self.notes
            .borrow()
            .editor
            .as_ref()
            .is_some_and(NoteEditor::protected)
    }

    pub(super) fn release_notes(&self) {
        if self.note_protected() {
            return;
        }
        self.note_save_timer.stop();
        let mut session = self.notes.borrow_mut();
        session.editor = None;
        session.list_request = 0;
        session.detail_request = 0;
        session.pending_open = None;
        session.tree_request = 0;
        session.tree.clear();
        session.tree_state = TreeState::default();
        session.target_folder = None;
        session.preview = false;
        drop(session);
        self.note_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let w = ui.shell.component();
            w.set_note_rows(ModelRc::default());
            w.set_note_content("".into());
            w.set_note_has_selection(false);
            w.set_note_preview_mode(false);
            w.set_note_preview_text(Default::default());
        });
        self.note_syncing.set(false);
    }

    fn sync_note(&self) {
        let Some((content, id, error, label, created, updated)) =
            self.notes.borrow().editor.as_ref().map(|e| {
                (
                    e.content.clone(),
                    e.detail.note.id.to_string(),
                    e.error.clone().unwrap_or_default(),
                    if e.pending.is_some() {
                        "正在保存…"
                    } else if e.dirty() {
                        "未保存"
                    } else {
                        "已保存"
                    },
                    e.detail
                        .note
                        .created_at
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string(),
                    e.detail
                        .note
                        .updated_at
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string(),
                )
            })
        else {
            return;
        };
        self.note_syncing.set(true);
        let _ = self.with_ui(|ui| {
            let w = ui.shell.component();
            w.set_note_has_selection(true);
            w.set_note_content(content.into());
            w.set_note_selected_id(id.into());
            w.set_note_error(error.into());
            w.set_note_save_label(label.into());
            w.set_note_created_date(created.into());
            w.set_note_updated_date(updated.into());
        });
        self.note_syncing.set(false);
    }

    pub(super) fn note_executed(
        self: &Rc<Self>,
        id: u64,
        result: Result<NoteCommandResult, NoteStoreError>,
    ) {
        let moving = self.notes.borrow().move_request;
        if let Some((_, selected)) = moving.filter(|(request, _)| *request == id) {
            self.notes.borrow_mut().move_request = None;
            let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
            match result {
                Ok(_) => {
                    self.load_notes();
                    if let Some(note) = selected {
                        self.open_note(note);
                    }
                }
                Err(error) => {
                    let text = if error.domain_error() == Some(NoteError::NameExists) {
                        "目标文件夹中已有同名文件或文件夹。"
                    } else {
                        "移动失败：请检查目标和目录层级。"
                    };
                    self.notice(ToastKind::Error, text);
                }
            }
            return;
        }
        let deleted = self.notes.borrow().delete_request.clone();
        if let Some((request, deleted_id)) = deleted.filter(|(request, _)| *request == id) {
            let _ = request;
            let mut session = self.notes.borrow_mut();
            session.delete_request = None;
            let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
            match result {
                Ok(result) => {
                    let selected = session
                        .editor
                        .as_ref()
                        .map(|editor| editor.detail.note.id.to_string());
                    let mut cursor = selected.as_deref();
                    let mut affected = false;
                    for _ in 0..3 {
                        let Some(current) = cursor else {
                            break;
                        };
                        if current == deleted_id {
                            affected = true;
                            break;
                        }
                        cursor = session
                            .tree
                            .iter()
                            .find(|entry| entry.id == current)
                            .and_then(|entry| entry.parent.as_deref());
                    }
                    if affected {
                        session.editor = None;
                        session.preview = false;
                        let _ = self.with_ui(|ui| {
                            let w = ui.shell.component();
                            w.set_note_has_selection(false);
                            w.set_note_content("".into());
                            w.set_note_selected_id("".into());
                            w.set_note_preview_mode(false);
                        });
                    }
                    if session.target_folder.as_deref() == Some(&deleted_id) {
                        session.target_folder = None;
                    }
                    let ticket = result.undo;
                    session.undo_token = ticket.map(|ticket| ticket.token);
                    drop(session);
                    self.load_notes();
                    if let Some(ticket) = ticket {
                        let _ = self.with_ui(|ui| ui.shell.component().set_note_undo_visible(true));
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
                Err(_) => {
                    drop(session);
                    self.notice(ToastKind::Error, "删除失败，原有内容已保留。");
                }
            }
            return;
        }
        if self.notes.borrow().undo_request == Some(id) {
            let mut session = self.notes.borrow_mut();
            session.undo_request = None;
            session.undo_token = None;
            drop(session);
            let _ = self.with_ui(|ui| ui.shell.component().set_note_undo_visible(false));
            match result {
                Ok(_) => self.load_notes(),
                Err(_) => self.notice(ToastKind::Error, "撤销失败或已过期。"),
            }
            return;
        }
        if self.notes.borrow().create_request == Some(id) {
            self.notes.borrow_mut().create_request = None;
            let _ = self.with_ui(|ui| ui.shell.component().set_note_create_busy(false));
            match result {
                Ok(result) => {
                    let _ = self.with_ui(|ui| {
                        ui.shell.component().set_note_create_name("".into());
                        ui.shell.component().set_note_create_open(false);
                    });
                    self.load_notes();
                    let renamed = self.notes.borrow_mut().rename_request.take();
                    if let Some(id) = renamed {
                        self.open_note(id);
                    }
                    if let Some(detail) = result.detail {
                        if !self.note_protected() && self.note_visible() {
                            self.notes.borrow_mut().editor = Some(NoteEditor::new(detail));
                            self.notes.borrow_mut().preview = false;
                            let _ = self
                                .with_ui(|ui| ui.shell.component().set_note_preview_mode(false));
                            self.sync_note();
                        }
                    }
                }
                Err(error) => {
                    self.notes.borrow_mut().rename_request = None;
                    let message = match error.domain_error() {
                        Some(NoteError::NameExists) => {
                            "同一目录已有同名文件或文件夹，请换一个名称。"
                        }
                        Some(NoteError::InvalidName) => "文件名无效，请修改后重试。",
                        _ => "创建失败，请重试。",
                    };
                    let _ = self
                        .with_ui(|ui| ui.shell.component().set_note_create_error(message.into()));
                }
            }
            return;
        }
        let matches = self
            .notes
            .borrow()
            .editor
            .as_ref()
            .is_some_and(|e| e.pending == Some(id));
        if !matches {
            tracing::warn!(request_id = id, "unexpected note command reply");
            return;
        }
        match result {
            Ok(result)
                if result.detail.as_ref().is_some_and(|detail| {
                    self.notes
                        .borrow()
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.detail.note.id == detail.note.id)
                }) =>
            {
                if let Some(detail) = result.detail {
                    if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
                        editor.acknowledge(id, detail);
                    }
                    self.sync_note();
                    self.load_notes();
                    if self.note_protected() {
                        self.flush_note();
                        return;
                    }
                    let action = self.notes.borrow_mut().after_save.take();
                    if let Some(action) = action {
                        self.resume_note_action(action);
                    }
                }
            }
            _ => {
                if let Some(editor) = self.notes.borrow_mut().editor.as_mut() {
                    editor.fail(id, "保存失败，编辑内容已保留。".to_owned());
                }
                self.sync_note();
                self.notice(ToastKind::Error, "小记保存失败，编辑内容已保留。");
            }
        }
    }

    pub(super) fn note_queried(&self, id: u64, result: Result<NoteReadResult, NoteStoreError>) {
        if self.notes.borrow().tree_request == id {
            self.notes.borrow_mut().tree_request = 0;
            if self.note_visible() {
                match result {
                    Ok(NoteReadResult::Tree(tree)) => {
                        self.notes.borrow_mut().tree = tree;
                        self.render_note_tree();
                    }
                    _ => self.notice(ToastKind::Error, "小记目录读取失败，请重试。"),
                }
            }
            return;
        }
        let (list, is_detail, target) = {
            let mut s = self.notes.borrow_mut();
            if s.list_request == id {
                s.list_request = 0;
                (true, false, None)
            } else if s.detail_request == id {
                s.detail_request = 0;
                (false, true, s.pending_open.take())
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
