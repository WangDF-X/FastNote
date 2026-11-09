use std::collections::VecDeque;

use fastnote_core::{domain::MAX_NOTE_TREE_DEPTH, NoteFileEntry};

const MAX_HISTORY: usize = 64;

/// NOTE-03 §2.1：左侧目录导航，刻意不持有编辑器或任何正文。
#[derive(Default)]
pub(crate) struct NoteBrowser {
    selected: Option<String>,
    back: VecDeque<Option<String>>,
    forward: Vec<Option<String>>,
}

/// 只改变目录选中位置的导航方向。
pub(crate) enum NoteNavigation {
    Back,
    Forward,
    Parent,
}

impl NoteBrowser {
    /// 当前目录选中项；None 表示根目录。
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// 是否存在可返回的位置。
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// 是否存在可前进的位置。
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// 选择已有节点或根目录；重复选择不增加历史。
    pub fn select(&mut self, tree: &[NoteFileEntry], id: Option<&str>) -> bool {
        if !exists(tree, id) || self.selected.as_deref() == id {
            return false;
        }
        self.back.push_back(self.selected.take());
        if self.back.len() > MAX_HISTORY {
            self.back.pop_front();
        }
        self.selected = id.map(str::to_owned);
        self.forward.clear();
        true
    }

    /// 恢复左侧历史或选择父目录，不发出打开文件的动作。
    pub fn navigate(&mut self, tree: &[NoteFileEntry], direction: NoteNavigation) -> bool {
        match direction {
            NoteNavigation::Back => {
                let Some(next) = self.back.pop_back() else {
                    return false;
                };
                self.forward.push(self.selected.take());
                self.selected = next;
            }
            NoteNavigation::Forward => {
                let Some(next) = self.forward.pop() else {
                    return false;
                };
                self.back.push_back(self.selected.take());
                self.selected = next;
            }
            NoteNavigation::Parent => {
                let Some(entry) = self
                    .selected()
                    .and_then(|id| tree.iter().find(|entry| entry.id == id))
                else {
                    return false;
                };
                let parent = entry.parent.clone();
                return self.select(tree, parent.as_deref());
            }
        }
        true
    }

    /// 新建目标：文件夹自身或所选文件的父目录。
    pub fn creation_parent(&self, tree: &[NoteFileEntry]) -> Option<String> {
        creation_parent(tree, self.selected())
    }

    /// 数据刷新后移除已删除的历史，选择最近的存活祖先。
    pub fn reconcile(&mut self, old: &[NoteFileEntry], tree: &[NoteFileEntry]) {
        for _ in 0..MAX_NOTE_TREE_DEPTH {
            if exists(tree, self.selected()) {
                break;
            }
            self.selected = self
                .selected()
                .and_then(|id| old.iter().find(|entry| entry.id == id))
                .and_then(|entry| entry.parent.clone());
        }
        if !exists(tree, self.selected()) {
            self.selected = None;
        }
        self.back.retain(|id| exists(tree, id.as_deref()));
        self.forward.retain(|id| exists(tree, id.as_deref()));
        while self.back.back() == Some(&self.selected) {
            self.back.pop_back();
        }
        while self.forward.last() == Some(&self.selected) {
            self.forward.pop();
        }
    }
}

/// 根据明确的上下文节点计算新建目录，不借用最后展开的文件夹。
pub(crate) fn creation_parent(tree: &[NoteFileEntry], id: Option<&str>) -> Option<String> {
    let entry = id.and_then(|id| tree.iter().find(|entry| entry.id == id))?;
    if entry.is_folder {
        Some(entry.id.clone())
    } else {
        entry.parent.clone()
    }
}

/// 生成 SQLite 虚拟目录的完整路径，根目录统一显示为“小记”。
pub(crate) fn virtual_path(tree: &[NoteFileEntry], id: Option<&str>) -> String {
    let mut parts = Vec::new();
    let mut cursor = id;
    for _ in 0..MAX_NOTE_TREE_DEPTH {
        let Some(entry) = cursor.and_then(|id| tree.iter().find(|entry| entry.id == id)) else {
            break;
        };
        parts.push(entry.name.as_str());
        cursor = entry.parent.as_deref();
    }
    parts.push("小记");
    parts.reverse();
    parts.join("\\")
}

/// 判断节点是否位于指定子树内，包含节点本身。
pub(crate) fn in_subtree(tree: &[NoteFileEntry], id: &str, ancestor: &str) -> bool {
    let mut cursor = Some(id);
    for _ in 0..MAX_NOTE_TREE_DEPTH {
        let Some(id) = cursor else { break };
        if id == ancestor {
            return true;
        }
        cursor = tree
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.parent.as_deref());
    }
    false
}

fn exists(tree: &[NoteFileEntry], id: Option<&str>) -> bool {
    id.is_none_or(|id| tree.iter().any(|entry| entry.id == id))
}

#[cfg(test)]
mod tests;
