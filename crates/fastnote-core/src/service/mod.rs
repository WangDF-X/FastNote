//! 在注入仓储/时钟上执行统一业务规则。

mod note;
mod note_undo;
mod task;

pub use note::{NoteCommand, NoteCommandResult, NoteService};
pub use note_undo::{NoteUndoState, NoteUndoTicket};
pub use task::TaskService;
