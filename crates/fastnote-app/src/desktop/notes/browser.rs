use fastnote_ui::NoteRowData;
use slint::{Model, ModelRc, VecModel};

use super::super::runtime::Runtime;
use super::NoteSession;
use crate::notes::{creation_parent, virtual_path, NoteNavigation};

impl Runtime {
    pub(super) fn select_note_entry(&self, id: &str) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        self.notes
            .borrow_mut()
            .select(if id.is_empty() { None } else { Some(id) });
        self.render_note_tree();
    }

    /// 用户明确导航只影响左侧：此路径不调用 open_note、flush_note 或 sync_note。
    pub(super) fn navigate_note_tree(&self, direction: &str) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        let direction = match direction {
            "back" => NoteNavigation::Back,
            "forward" => NoteNavigation::Forward,
            "up" => NoteNavigation::Parent,
            _ => return,
        };
        self.notes.borrow_mut().navigate(direction);
        self.render_note_tree();
    }

    pub(super) fn toggle_note_folder(&self, id: &str) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        let mut session = self.notes.borrow_mut();
        let NoteSession {
            tree, tree_state, ..
        } = &mut *session;
        tree_state.toggle(tree, id);
        drop(session);
        self.render_note_tree();
    }

    pub(super) fn render_note_tree(&self) {
        if !self.note_visible() {
            return;
        }
        let session = self.notes.borrow();
        let selected = session.browser.selected().unwrap_or_default().to_owned();
        let path = virtual_path(&session.tree, session.browser.selected());
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
        let reveal_index = rows
            .iter()
            .position(|row| row.id.as_str() == selected)
            .map_or(-1, |index| index as i32);
        let total = session.tree.iter().filter(|entry| !entry.is_folder).count() as i32;
        let active_id = session
            .editor
            .as_ref()
            .map(|editor| editor.detail.note.id.to_string());
        let current_name = active_id
            .as_deref()
            .and_then(|id| session.tree.iter().find(|entry| entry.id == id))
            .map_or("", |entry| entry.name.as_str())
            .to_owned();
        let can_back = session.browser.can_back();
        let can_forward = session.browser.can_forward();
        let can_parent = session.browser.selected().is_some();
        let reveal_tick = session.reveal_tick;
        drop(session);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_total(total);
            window.set_note_selected_id(selected.into());
            window.set_note_selected_path(path.into());
            window.set_note_current_name(current_name.into());
            window.set_note_can_back(can_back);
            window.set_note_can_forward(can_forward);
            window.set_note_can_parent(can_parent);
            let current = window.get_note_rows();
            let changed = current.row_count() != rows.len()
                || current.iter().zip(&rows).any(|(old, new)| {
                    old.id != new.id
                        || old.title != new.title
                        || old.is_folder != new.is_folder
                        || old.expanded != new.expanded
                        || old.depth != new.depth
                });
            // 只更新选中路径时保留行实例，避免销毁键盘焦点和原位输入控件。
            if changed {
                window.set_note_rows(ModelRc::new(VecModel::from(rows)));
            }
            window.set_note_reveal_index(reveal_index);
            window.set_note_reveal_tick(reveal_tick);
        });
    }

    pub(super) fn begin_note_create(&self, context: &str, folder: bool) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        let session = self.notes.borrow();
        let parent = if context == session.browser.selected().unwrap_or_default() {
            session.browser.creation_parent(&session.tree)
        } else {
            creation_parent(
                &session.tree,
                if context.is_empty() {
                    None
                } else {
                    Some(context)
                },
            )
        };
        let path = virtual_path(&session.tree, parent.as_deref());
        drop(session);
        self.notes.borrow_mut().create_parent = parent;
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_create_name("".into());
            window.set_note_create_error("".into());
            window.set_note_create_folder(folder);
            window.set_note_create_path(path.into());
            window.set_note_create_open(true);
        });
    }

    pub(super) fn begin_note_rename(&self, id: &str) {
        if !self.note_visible() || self.notes.borrow().tree_busy() {
            return;
        }
        let entry = self
            .notes
            .borrow()
            .tree
            .iter()
            .find(|entry| entry.id == id)
            .cloned();
        let Some(entry) = entry else { return };
        self.notes.borrow_mut().select(Some(id));
        self.notes.borrow_mut().rename_target = Some(id.to_owned());
        self.render_note_tree();
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_rename_name(entry.name.into());
            window.set_note_rename_error("".into());
            window.set_note_rename_id(id.into());
        });
    }

    pub(super) fn cancel_note_rename(&self) {
        if self.notes.borrow().tree_busy() {
            return;
        }
        let mut session = self.notes.borrow_mut();
        session.rename_target = None;
        if session
            .after_save
            .as_deref()
            .is_some_and(|action| action.starts_with("note-rename:"))
        {
            session.after_save = None;
        }
        drop(session);
        let _ = self.with_ui(|ui| {
            let window = ui.shell.component();
            window.set_note_rename_id("".into());
            window.set_note_rename_name("".into());
            window.set_note_rename_error("".into());
            window.set_note_create_busy(false);
        });
    }
}
