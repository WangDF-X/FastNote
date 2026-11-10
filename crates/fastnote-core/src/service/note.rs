use chrono::{DateTime, Utc};

use crate::{AppEvent, Note, NoteDetail, NoteError, NoteId, NoteRepository, TaskClock};

use super::{NoteUndoState, NoteUndoTicket};

/// 小记写命令，均由唯一 DB 工作线程执行。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NoteCommand {
    /// 快速捕获新增正文。
    Create { id: NoteId, content: String },
    /// 在根目录创建独立名称的空白小记。
    CreateNamed { id: NoteId, name: String },
    /// 在指定目录创建空白小记。
    CreateFile {
        id: NoteId,
        parent: Option<String>,
        name: String,
    },
    /// 创建目录。
    CreateFolder {
        id: String,
        parent: Option<String>,
        name: String,
    },
    /// 原子移动或重命名文件或目录。
    Relocate {
        id: String,
        parent: Option<String>,
        name: String,
    },
    /// 自动保存，旧版本不能覆盖新正文。
    Update {
        id: NoteId,
        content: String,
        expected_updated_at: DateTime<Utc>,
    },
    /// 删除单条或全部，令牌只用于最新批次的两秒撤销。
    Delete { id: Option<NoteId>, token: u64 },
    /// 删除已由调用方确认的文件夹及后代。
    DeleteFolder { id: String, token: u64 },
    /// 恢复最新批次，过期或旧令牌拒绝。
    Undo { token: u64 },
}

/// 已提交的变更；不返回批量正文，不携带敏感日志数据。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoteCommandResult {
    /// 当前单条详情的保存确认。
    pub detail: Option<NoteDetail>,
    /// 删除/恢复条数。
    pub count: u32,
    /// 两秒撤销令牌。
    pub undo: Option<NoteUndoTicket>,
    /// 事务提交后的跨页面失效事件。
    pub events: Vec<AppEvent>,
}

/// 注入仓储、时钟和工作线程持有的短期撤销状态。
pub struct NoteService<'a, R: NoteRepository, C: TaskClock> {
    repository: &'a mut R,
    clock: C,
    undo: &'a mut NoteUndoState,
}

impl<'a, R: NoteRepository, C: TaskClock> NoteService<'a, R, C> {
    /// 不自行访问 UI、生成 ID 或启动线程。
    pub fn new(repository: &'a mut R, clock: C, undo: &'a mut NoteUndoState) -> Self {
        Self {
            repository,
            clock,
            undo,
        }
    }

    /// 即使没有新命令，也由工作线程在截止时间调用以销毁临时批次。
    pub fn expire_undo(&mut self) -> Result<(), R::Error> {
        if self.undo.expired(self.clock.now()) {
            self.repository.discard_note_undo()?;
            self.undo.clear();
        }
        Ok(())
    }

    /// 执行一次写操作，只有成功提交后才产生事件。
    pub fn execute(&mut self, command: NoteCommand) -> Result<NoteCommandResult, R::Error> {
        self.expire_undo()?;
        let now = self.clock.now();
        let mut result = NoteCommandResult::default();
        match command {
            NoteCommand::Create { id, content } => {
                if content.trim().is_empty() {
                    return Err(NoteError::Empty.into());
                }
                let note = Note::new(id, content, now)?;
                self.repository.insert_note(&note)?;
                result.detail = Some(NoteDetail { note });
            }
            NoteCommand::CreateNamed { id, name } => {
                if crate::domain::validate_note_name(&name).is_err() {
                    return Err(NoteError::InvalidName.into());
                }
                let note = Note::new(id, String::new(), now)?;
                self.repository.insert_named_note(&note, &name)?;
                result.detail = Some(NoteDetail { note });
            }
            NoteCommand::CreateFile { id, parent, name } => {
                let note = Note::new(id, String::new(), now)?;
                self.repository
                    .insert_note_in_folder(&note, parent.as_deref(), &name)?;
                result.detail = Some(NoteDetail { note });
            }
            NoteCommand::CreateFolder { id, parent, name } => {
                self.repository
                    .insert_note_folder(&id, parent.as_deref(), &name, now)?;
            }
            NoteCommand::Relocate { id, parent, name } => {
                self.repository
                    .relocate_note_entry(&id, parent.as_deref(), &name, now)?;
            }
            NoteCommand::Update {
                id,
                content,
                expected_updated_at,
            } => {
                let mut detail = self.detail(id)?;
                if detail.note.updated_at != expected_updated_at {
                    return Err(NoteError::Conflict.into());
                }
                let next = detail.note.with_content(content, now)?;
                if next == detail.note {
                    result.detail = Some(detail);
                    return Ok(result);
                }
                self.repository.replace_note(&detail.note, &next)?;
                detail.note = next;
                result.detail = Some(detail);
            }
            NoteCommand::Delete { id, token } => {
                result.count = self.repository.delete_notes(id)?;
                if result.count == 0 {
                    return Ok(result);
                }
                result.undo = Some(self.undo.activate(token, self.clock.now()));
                result
                    .events
                    .push(AppEvent::RemindersChanged { owners: Vec::new() });
            }
            NoteCommand::DeleteFolder { id, token } => {
                result.count = self.repository.delete_note_folder(&id)?;
                result.undo = Some(self.undo.activate(token, self.clock.now()));
                result
                    .events
                    .push(AppEvent::RemindersChanged { owners: Vec::new() });
            }
            NoteCommand::Undo { token } => {
                if !self.undo.permits(token, now) {
                    return Err(NoteError::UndoExpired.into());
                }
                result.count = self.repository.restore_notes()?;
                self.undo.clear();
                result
                    .events
                    .push(AppEvent::RemindersChanged { owners: Vec::new() });
            }
        }
        result.events.push(AppEvent::NotesChanged);
        Ok(result)
    }

    fn detail(&self, id: NoteId) -> Result<NoteDetail, R::Error> {
        self.repository
            .note_detail(id)?
            .ok_or_else(|| NoteError::NotFound.into())
    }
}
