//! 小记树的层级与移动规则；不依赖数据库或界面。

use std::collections::{HashMap, HashSet};

/// 小记文件或文件夹最多占用的路径层数，根目录不计入。
pub const MAX_NOTE_TREE_DEPTH: usize = 3;

/// 层级或移动操作不合法。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoteTreeError {
    /// 父节点不存在或不是文件夹。
    InvalidParent,
    /// 文件夹不能移动到自身或后代。
    Cycle,
    /// 新路径或移动后的某个后代路径超过三层。
    TooDeep,
    /// 被移动的节点不存在。
    NotFound,
    /// 存储中的父链已经存在循环。
    CorruptTree,
}

/// 仓储提供的轻量树节点；不包含正文。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteTreeNode<Id> {
    /// 节点标识。
    pub id: Id,
    /// 父文件夹标识；根目录为 None。
    pub parent: Option<Id>,
    /// 是否为文件夹。
    pub is_folder: bool,
}

/// 校验将一个节点及其所有后代移动到目标文件夹后是否仍是合法三层树。
///
/// 调用者应在同一数据库事务内读取节点、执行校验并写入，以免并发修改导致失效。
pub fn validate_note_move<Id: Clone + Eq + std::hash::Hash>(
    nodes: &[NoteTreeNode<Id>],
    id: &Id,
    destination: Option<&Id>,
) -> Result<(), NoteTreeError> {
    let index: HashMap<_, _> = nodes.iter().map(|node| (&node.id, node)).collect();
    if index.len() != nodes.len() {
        return Err(NoteTreeError::CorruptTree);
    }
    if !index.contains_key(id) {
        return Err(NoteTreeError::NotFound);
    }
    if let Some(parent) = destination {
        if !index.get(parent).is_some_and(|node| node.is_folder) {
            return Err(NoteTreeError::InvalidParent);
        }
    }
    for node in nodes {
        let mut seen = HashSet::new();
        let mut cursor = Some(&node.id);
        let mut depth = 0;
        while let Some(current) = cursor {
            if !seen.insert(current) {
                return Err(if current == id {
                    NoteTreeError::Cycle
                } else {
                    NoteTreeError::CorruptTree
                });
            }
            depth += 1;
            // 已经超限的父链无需继续扫描；避免损坏数据造成二次方工作量。
            if depth > MAX_NOTE_TREE_DEPTH {
                return Err(NoteTreeError::TooDeep);
            }
            let current_node = index.get(current).ok_or(NoteTreeError::InvalidParent)?;
            cursor = if current == id {
                destination
            } else {
                current_node.parent.as_ref()
            };
            if let Some(parent) = cursor {
                if !index
                    .get(parent)
                    .is_some_and(|candidate| candidate.is_folder)
                {
                    return Err(NoteTreeError::InvalidParent);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<NoteTreeNode<u8>> {
        vec![
            NoteTreeNode {
                id: 1,
                parent: None,
                is_folder: true,
            },
            NoteTreeNode {
                id: 2,
                parent: Some(1),
                is_folder: true,
            },
            NoteTreeNode {
                id: 3,
                parent: Some(2),
                is_folder: false,
            },
            NoteTreeNode {
                id: 4,
                parent: None,
                is_folder: true,
            },
        ]
    }

    #[test]
    fn test_move_validates_all_descendant_depths_and_cycles() {
        let nodes = tree();
        assert_eq!(
            validate_note_move(&nodes, &1, Some(&4)),
            Err(NoteTreeError::TooDeep)
        );
        assert_eq!(
            validate_note_move(&nodes, &1, Some(&2)),
            Err(NoteTreeError::Cycle)
        );
        assert_eq!(
            validate_note_move(&nodes, &1, Some(&1)),
            Err(NoteTreeError::Cycle)
        );
        assert_eq!(validate_note_move(&nodes, &3, Some(&4)), Ok(()));
        assert_eq!(validate_note_move(&nodes, &2, None), Ok(()));
    }

    #[test]
    fn test_move_rejects_missing_nodes_and_files_as_parents() {
        let nodes = tree();
        assert_eq!(
            validate_note_move(&nodes, &9, None),
            Err(NoteTreeError::NotFound)
        );
        assert_eq!(
            validate_note_move(&nodes, &3, Some(&9)),
            Err(NoteTreeError::InvalidParent)
        );
        assert_eq!(
            validate_note_move(&nodes, &4, Some(&3)),
            Err(NoteTreeError::InvalidParent)
        );
    }
}
