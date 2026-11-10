//! 领域基础类型；实体标识见 ARCH-03 §3。

mod id;
mod note;
mod note_name;
mod note_tree;
mod task;
mod task_types;

pub use id::{IdError, NoteId, PlanId, TaskId};
pub use note::{note_excerpt, validate_note_content, Note, NoteError, MAX_NOTE_LENGTH};
pub use note_name::{validate_note_name, NoteNameError};
pub use note_tree::{validate_note_move, NoteTreeError, NoteTreeNode, MAX_NOTE_TREE_DEPTH};
pub use task::{parse_task_date, parse_task_time, Patch, Task, TaskPatch};
pub use task_types::{Priority, TaskError, TaskKind, TaskStatus, WarningCode};
