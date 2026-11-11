//! 领域基础类型；实体标识见 ARCH-03 §3。

mod id;
mod journal;
mod journal_attachment;
mod note;
mod note_name;
mod note_tree;
mod task;
mod task_types;

pub use id::{IdError, NoteId, PlanId, TaskId};
pub use journal::{
    journal_excerpt, validate_journal_content, JournalEntry, JournalError, JOURNAL_EXCERPT_LENGTH,
    MAX_JOURNAL_LENGTH,
};
pub use journal_attachment::{
    validate_attachment, AttachmentError, ImageMediaType, JournalAttachment,
    JOURNAL_ATTACHMENT_THUMB_EDGE, JOURNAL_ATTACHMENT_VIEW_EDGE, MAX_JOURNAL_ATTACHMENTS,
    MAX_JOURNAL_ATTACHMENT_BYTES, MAX_JOURNAL_ATTACHMENT_PIXELS,
};
pub use note::{note_excerpt, validate_note_content, Note, NoteError, MAX_NOTE_LENGTH};
pub use note_name::{validate_note_name, NoteNameError};
pub use note_tree::{validate_note_move, NoteTreeError, NoteTreeNode, MAX_NOTE_TREE_DEPTH};
pub use task::{parse_task_date, parse_task_time, Patch, Task, TaskPatch};
pub use task_types::{Priority, TaskError, TaskKind, TaskStatus, WarningCode};
