use chrono::{DateTime, Utc};

use crate::{
    Note, NoteDetail, NoteError, NoteFileEntry, NoteId, NoteListPage, NoteListQuery, TaskClock,
};

/// 小记仓储；批量删除和恢复必须由实现保证原子性。
pub trait NoteRepository {
    /// 可追溯的存储错误，保留领域码。
    type Error: From<NoteError>;
    /// 读取无正文的目录树。
    fn note_file_tree(&self) -> Result<Vec<NoteFileEntry>, Self::Error>;
    /// 只读取有界摘要，按注入的设备时区分组。
    fn list_notes(
        &self,
        query: &NoteListQuery,
        clock: &impl TaskClock,
    ) -> Result<NoteListPage, Self::Error>;
    /// 单条详情，缺失返回 None。
    fn note_detail(&self, id: NoteId) -> Result<Option<NoteDetail>, Self::Error>;
    /// 原样新增正文。
    fn insert_note(&mut self, note: &Note) -> Result<(), Self::Error>;
    /// 在根目录创建有独立名称的小记；名称冲突必须拒绝。
    fn insert_named_note(&mut self, note: &Note, name: &str) -> Result<(), Self::Error>;
    /// 在指定目录新建小记，根目录以 None 表示。
    fn insert_note_in_folder(
        &mut self,
        note: &Note,
        parent: Option<&str>,
        name: &str,
    ) -> Result<(), Self::Error>;
    /// 创建目录；调用者提供 UUID v7。
    fn insert_note_folder(
        &mut self,
        id: &str,
        parent: Option<&str>,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Self::Error>;
    /// 原子移动或重命名文件树节点。
    fn relocate_note_entry(
        &mut self,
        id: &str,
        parent: Option<&str>,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Self::Error>;
    /// 比较完整原版本后替换正文，不改变关联或创建时间。
    fn replace_note(&mut self, expected: &Note, next: &Note) -> Result<(), Self::Error>;
    /// 硬删除一条或全部，最新批次短暂保存在数据库临时表，不返回正文集合。
    fn delete_notes(&mut self, id: Option<NoteId>) -> Result<u32, Self::Error>;
    /// 原子删除文件夹及其全部后代，并保留最新撤销批次。
    fn delete_note_folder(&mut self, id: &str) -> Result<u32, Self::Error>;
    /// 原子恢复最新删除批次，包括已有提醒及投递记录。
    fn restore_notes(&mut self) -> Result<u32, Self::Error>;
    /// 及时销毁临时撤销数据，不作为回收站或跨会话缓存。
    fn discard_note_undo(&mut self) -> Result<(), Self::Error>;
}
