use chrono::{DateTime, NaiveDate, Utc};

use crate::{Note, NoteId};

/// 列表用小记摘要，不包含完整正文。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteSummary {
    /// 小记标识。
    pub id: NoteId,
    /// 首个非空行，最多 80 字。
    pub title: String,
    /// 最多两行、120 字。
    pub preview: String,
    /// 最近变更 UTC 时间。
    pub updated_at: DateTime<Utc>,
}

/// 一个设备本地日期分组；只装载当前的 20 条摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteDateGroup {
    /// 最近更新日期（本地）。
    pub date: NaiveDate,
    /// 该日期所有条目数。
    pub total: u32,
    /// 该组当前页偏移量。
    pub offset: u32,
    /// 有界的当前页摘要。
    pub items: Vec<NoteSummary>,
}

/// 按日期分页的小记列表；仅编辑器读取正文。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoteListPage {
    /// 日期分组，按更新日期倒序。
    pub groups: Vec<NoteDateGroup>,
    /// 小记总数。
    pub total: u32,
    /// 日期分组总数，供前后页导航。
    pub group_total: u32,
}

/// 一个正在编辑的小记详情。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteDetail {
    /// 完整实体。
    pub note: Note,
}

/// 分组页查询：每页至多十个日期，每个日期二十条，不累计历史正文。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoteListQuery {
    /// 日期分组偏移量。
    pub group_offset: u32,
    /// 当前各日期组的条目偏移量；其他组从零开始。
    pub offsets: Vec<(NaiveDate, u32)>,
}
/// 文件树中的轻量节点；不包含正文。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteFileEntry {
    /// UUID 字符串。
    pub id: String,
    /// 父文件夹，根目录为 None。
    pub parent: Option<String>,
    /// 文件或文件夹的独立名称。
    pub name: String,
    /// 是否为文件夹。
    pub is_folder: bool,
}
