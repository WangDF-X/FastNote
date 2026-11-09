//! NOTE-03：编辑器与左侧目录会话独立，导航不能隐式打开文件。

mod browser;
mod connect;
mod editing;
mod entries;
mod entry_replies;
mod replies;

use fastnote_core::{NoteFileEntry, NoteId};

use crate::notes::{NoteBrowser, NoteEditor, NoteNavigation, TreeState};

#[derive(Default)]
pub(super) struct NoteSession {
    editor: Option<NoteEditor>,
    list_request: u64,
    detail_request: u64,
    pending_open: Option<NoteId>,
    after_save: Option<String>,
    preview: bool,
    tree_request: u64,
    tree_loaded: bool,
    tree: Vec<NoteFileEntry>,
    tree_state: TreeState,
    browser: NoteBrowser,
    reveal_tick: i32,
    create_parent: Option<String>,
    create_request: Option<(u64, NoteFileEntry)>,
    rename_target: Option<String>,
    relocate_request: Option<RelocateRequest>,
    metadata_request: Option<(u64, NoteId)>,
    delete_request: Option<(u64, String)>,
    undo_request: Option<u64>,
    undo_token: Option<u64>,
}

struct RelocateRequest {
    request: u64,
    entry: NoteFileEntry,
    renaming: bool,
}

impl NoteSession {
    fn tree_busy(&self) -> bool {
        self.create_request.is_some()
            || self.relocate_request.is_some()
            || self.metadata_request.is_some()
            || self.delete_request.is_some()
            || self.undo_request.is_some()
    }

    fn select(&mut self, id: Option<&str>) {
        if self.browser.select(&self.tree, id) {
            self.reveal_selection();
        }
    }

    fn navigate(&mut self, direction: NoteNavigation) {
        if self.browser.navigate(&self.tree, direction) {
            self.reveal_selection();
        }
    }

    fn reveal_selection(&mut self) {
        self.tree_state.reveal(&self.tree, self.browser.selected());
        self.reveal_tick = self.reveal_tick.wrapping_add(1);
    }

    fn replace_tree(&mut self, tree: Vec<NoteFileEntry>) {
        self.browser.reconcile(&self.tree, &tree);
        self.tree_state.reconcile(&tree);
        self.tree = tree;
        self.reveal_selection();
    }

    fn invalidate_open(&mut self) {
        self.detail_request = 0;
        self.pending_open = None;
    }
}

#[cfg(test)]
mod tests;
