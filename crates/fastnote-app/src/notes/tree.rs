use std::collections::HashSet;

use fastnote_core::{domain::MAX_NOTE_TREE_DEPTH, NoteFileEntry};

/// 目录展开状态；收起时递归移除后代的展开状态。
#[derive(Default)]
pub(crate) struct TreeState {
    expanded: HashSet<String>,
}

impl TreeState {
    /// 单独切换展开状态，不改变目录选中项或新建目标。
    pub fn toggle(&mut self, tree: &[NoteFileEntry], id: &str) {
        if !tree.iter().any(|e| e.id == id && e.is_folder) {
            return;
        }
        if self.expanded.remove(id) {
            let mut pending = vec![id.to_owned()];
            while let Some(parent) = pending.pop() {
                for child in tree
                    .iter()
                    .filter(|e| e.parent.as_deref() == Some(parent.as_str()) && e.is_folder)
                {
                    self.expanded.remove(&child.id);
                    pending.push(child.id.clone());
                }
            }
        } else {
            self.expanded.insert(id.to_owned());
        }
    }

    /// 展开选中项的祖先，用于历史导航及新建后定位。
    pub fn reveal(&mut self, tree: &[NoteFileEntry], id: Option<&str>) {
        let mut cursor = id;
        for _ in 0..MAX_NOTE_TREE_DEPTH {
            let parent = cursor
                .and_then(|id| tree.iter().find(|entry| entry.id == id))
                .and_then(|entry| entry.parent.as_deref());
            if let Some(parent) = parent {
                self.expanded.insert(parent.to_owned());
            }
            cursor = parent;
        }
    }

    /// 删除节点刷新后丢弃无效的展开标识。
    pub fn reconcile(&mut self, tree: &[NoteFileEntry]) {
        self.expanded
            .retain(|id| tree.iter().any(|entry| &entry.id == id && entry.is_folder));
    }

    /// 只投影已展开的目录，文件夹优先，同级名称按大小写不敏感排序。
    pub fn visible<'a>(&self, tree: &'a [NoteFileEntry]) -> Vec<(&'a NoteFileEntry, usize, bool)> {
        fn append<'a>(
            tree: &'a [NoteFileEntry],
            state: &TreeState,
            parent: Option<&str>,
            depth: usize,
            rows: &mut Vec<(&'a NoteFileEntry, usize, bool)>,
        ) {
            if depth >= MAX_NOTE_TREE_DEPTH {
                return;
            }
            let mut children: Vec<_> = tree
                .iter()
                .filter(|e| e.parent.as_deref() == parent)
                .collect();
            children.sort_by(|a, b| {
                b.is_folder
                    .cmp(&a.is_folder)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then_with(|| a.id.cmp(&b.id))
            });
            for child in children {
                let expanded = state.expanded.contains(&child.id);
                rows.push((child, depth, expanded));
                if child.is_folder && expanded {
                    append(tree, state, Some(&child.id), depth + 1, rows);
                }
            }
        }
        let mut rows = Vec::new();
        append(tree, self, None, 0, &mut rows);
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_collapsing_folder_resets_expanded_descendants() {
        let entry = |id: &str, parent: Option<&str>, is_folder| NoteFileEntry {
            id: id.into(),
            parent: parent.map(str::to_owned),
            name: id.into(),
            is_folder,
        };
        let tree = vec![
            entry("a", None, true),
            entry("b", Some("a"), true),
            entry("c", Some("b"), false),
        ];
        let mut state = TreeState::default();
        assert_eq!(state.visible(&tree).len(), 1);
        state.toggle(&tree, "a");
        state.toggle(&tree, "b");
        assert_eq!(state.visible(&tree).len(), 3);
        state.toggle(&tree, "a");
        state.toggle(&tree, "a");
        assert_eq!(state.visible(&tree).len(), 2);
        assert!(!state.visible(&tree)[1].2);
    }

    #[test]
    fn test_reveal_expands_only_ancestors_and_reconcile_discards_removed_folders() {
        let entry = |id: &str, parent: Option<&str>, is_folder| NoteFileEntry {
            id: id.into(),
            parent: parent.map(str::to_owned),
            name: id.into(),
            is_folder,
        };
        let tree = vec![
            entry("a", None, true),
            entry("b", Some("a"), true),
            entry("c", Some("b"), false),
            entry("other", None, true),
        ];
        let mut state = TreeState::default();
        state.reveal(&tree, Some("c"));
        assert_eq!(state.visible(&tree).len(), 4);
        assert!(state.expanded.contains("a"));
        assert!(state.expanded.contains("b"));
        assert!(!state.expanded.contains("other"));
        state.reconcile(&tree[..1]);
        assert!(state.expanded.contains("a"));
        assert!(!state.expanded.contains("b"));
        state.reveal(&tree, Some("missing"));
        state.reveal(&tree, None);
        assert_eq!(state.expanded.len(), 1);
    }
}
