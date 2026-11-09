use std::collections::HashSet;

use fastnote_core::NoteFileEntry;

/// 目录展开状态；收起时递归移除后代的展开状态。
#[derive(Default)]
pub(crate) struct TreeState {
    expanded: HashSet<String>,
}

impl TreeState {
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

    pub fn visible<'a>(&self, tree: &'a [NoteFileEntry]) -> Vec<(&'a NoteFileEntry, usize, bool)> {
        fn append<'a>(
            tree: &'a [NoteFileEntry],
            state: &TreeState,
            parent: Option<&str>,
            depth: usize,
            rows: &mut Vec<(&'a NoteFileEntry, usize, bool)>,
        ) {
            if depth >= 3 {
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
}
