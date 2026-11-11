//! 日记列表与附件读模型；正文只在详情查询时读取。

use chrono::{DateTime, NaiveDate, Utc};

use crate::JournalAttachment;

/// 月份列表中的一条日记摘要，不携带完整正文与图片字节。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalSummary {
    /// 设备本地日期。
    pub entry_date: NaiveDate,
    /// 去掉 Markdown 行首标记后的前 30 个字符；正文为空时为空串。
    pub excerpt: String,
    /// 最近一次保存时间（UTC）。
    pub updated_at: DateTime<Utc>,
    /// 该日图片数量；列表据此显示「N 张图片」或「· N 图」。
    pub attachment_count: u32,
}

/// 单张附件的完整数据；只在打开查看器或生成缩略图时读取。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalAttachmentPayload {
    /// 元数据。
    pub attachment: JournalAttachment,
    /// 原图字节。
    pub bytes: Vec<u8>,
    /// 缩略图字节（JPEG）。
    pub thumbnail: Vec<u8>,
}

/// 全部日记图片的占用统计，供添加提示与设置页使用。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JournalImageUsage {
    /// 图片张数。
    pub count: u32,
    /// 原图 + 缩略图字节数。
    pub bytes: u64,
}
