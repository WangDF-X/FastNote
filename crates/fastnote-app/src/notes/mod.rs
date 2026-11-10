//! 小记读写通道与可独立测试的当前草稿；不持有历史正文集合。

mod browser;
mod editor;
mod markdown;
mod tree;
mod worker;

pub(crate) use browser::{
    breadcrumb_path, creation_parent, in_subtree, virtual_path, NoteBrowser, NoteNavigation,
};
pub(crate) use editor::{NoteEditor, SavePlan};
pub(crate) use markdown::preview_blocks;
pub(crate) use tree::TreeState;
pub(crate) use worker::{execute, expire, query, NoteRead, NoteReadResult};
