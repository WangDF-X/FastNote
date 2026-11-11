//! 日记按本地日期唯一；正文为 Markdown 源码，空正文不落库（DIARY-01）。

use chrono::{DateTime, NaiveDate, Utc};
use thiserror::Error;

/// 日记正文的 Unicode 字符上限，不按 UTF-8 字节计算。
pub const MAX_JOURNAL_LENGTH: usize = 10_000;

/// 列表摘要保留的正文字符数，超出部分以省略号表示。
pub const JOURNAL_EXCERPT_LENGTH: usize = 30;

/// 日记实体；日期是业务主键，内部存储主键不暴露给界面。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalEntry {
    /// 设备本地日期。
    pub entry_date: NaiveDate,
    /// 原样保留的 Markdown 源码。
    pub content: String,
    /// 首次创建时间（UTC）。
    pub created_at: DateTime<Utc>,
    /// 最近一次真实变更时间（UTC）。
    pub updated_at: DateTime<Utc>,
}

/// 稳定领域错误；不携带正文或用户输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum JournalError {
    /// 空正文只用于删除条目，不能构造实体。
    #[error("E_JOURNAL_EMPTY")]
    Empty,
    /// 正文超过 10,000 字符。
    #[error("E_JOURNAL_TOO_LONG")]
    TooLong,
    /// 目标日期或附件没有日记。
    #[error("E_NOT_FOUND")]
    NotFound,
    /// 基线版本已过期或唯一约束冲突。
    #[error("E_CONFLICT")]
    Conflict,
    /// 图片格式、大小、像素或数量不合法。
    #[error(transparent)]
    Attachment(#[from] super::journal_attachment::AttachmentError),
}

impl JournalEntry {
    /// 构造有内容的日记；不生成 ID，也不读取系统时钟。
    pub fn new(
        entry_date: NaiveDate,
        content: String,
        now: DateTime<Utc>,
    ) -> Result<Self, JournalError> {
        validate_journal_content(&content)?;
        if content.trim().is_empty() {
            return Err(JournalError::Empty);
        }
        Ok(Self {
            entry_date,
            content,
            created_at: now,
            updated_at: now,
        })
    }

    /// 校验后生成新的正文版本；重复保存不改变时间。
    pub fn with_content(&self, content: String, now: DateTime<Utc>) -> Result<Self, JournalError> {
        validate_journal_content(&content)?;
        let mut next = self.clone();
        if content != self.content {
            next.content = content;
            next.updated_at = now.max(self.updated_at + chrono::Duration::microseconds(1));
        }
        Ok(next)
    }
}

/// 正文按 Unicode 字符计数；空白正文是合法的删除意图，由服务层判定。
pub fn validate_journal_content(content: &str) -> Result<(), JournalError> {
    if content.chars().take(MAX_JOURNAL_LENGTH + 1).count() > MAX_JOURNAL_LENGTH {
        return Err(JournalError::TooLong);
    }
    Ok(())
}

/// 列表摘要：去掉行首 Markdown 标记，折叠空白后取前 30 个字符。
///
/// 只解析到摘要长度为止，不物化整篇正文，符合 DIARY-01 的按需读取约束。
pub fn journal_excerpt(content: &str) -> String {
    let mut text = String::new();
    for line in content.lines() {
        if is_fence(line) {
            continue;
        }
        let cleaned = collapse_whitespace(strip_markdown_lead(line));
        if cleaned.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&cleaned);
        if text.chars().count() > JOURNAL_EXCERPT_LENGTH {
            break;
        }
    }
    let mut chars = text.chars();
    let excerpt: String = chars.by_ref().take(JOURNAL_EXCERPT_LENGTH).collect();
    if chars.next().is_some() {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn collapse_whitespace(content: &str) -> String {
    let mut out = String::with_capacity(content.len().min(256));
    let mut pending_space = false;
    for ch in content.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch);
        }
    }
    out
}

fn strip_markdown_lead(text: &str) -> &str {
    let mut rest = text;
    loop {
        let current = rest.trim_start();
        if let Some(after) = current.strip_prefix('>') {
            rest = after;
            continue;
        }
        if current.starts_with('#') {
            rest = current.trim_start_matches('#').trim_start();
            continue;
        }
        if let Some(after) = strip_bullet(current) {
            rest = after;
            continue;
        }
        if let Some(after) = strip_task_box(current) {
            rest = after;
            continue;
        }
        return current;
    }
}

fn strip_bullet(text: &str) -> Option<&str> {
    let mut chars = text.chars();
    if !matches!(chars.next()?, '-' | '*' | '+') {
        return None;
    }
    let after = chars.as_str();
    (after.is_empty() || after.starts_with(char::is_whitespace)).then_some(after)
}

fn strip_task_box(text: &str) -> Option<&str> {
    for marker in ["[ ]", "[x]", "[X]"] {
        if let Some(after) = text.strip_prefix(marker) {
            if after.is_empty() || after.starts_with(char::is_whitespace) {
                return Some(after);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
    }

    fn instant() -> DateTime<Utc> {
        "2026-10-23T06:32:00Z".parse().unwrap()
    }

    #[test]
    fn test_excerpt_strips_heading_and_list_markers() {
        assert_eq!(journal_excerpt("# 今天\n- [x] 完成报告"), "今天 完成报告");
        assert_eq!(journal_excerpt("> 引用\n- 事项"), "引用 事项");
        assert_eq!(journal_excerpt("## 一级标题"), "一级标题");
    }

    #[test]
    fn test_excerpt_collapses_whitespace_and_blank_lines() {
        assert_eq!(
            journal_excerpt(" \n\t第一段\n\n   第二段  "),
            "第一段 第二段"
        );
        assert_eq!(journal_excerpt("   "), "");
    }

    #[test]
    fn test_excerpt_truncates_after_thirty_unicode_chars() {
        let content = "一二三四五六七八九十一二三四五六七八九十一二三四五六七八九十多出来的";
        assert_eq!(
            journal_excerpt(content),
            "一二三四五六七八九十一二三四五六七八九十一二三四五六七八九十…"
        );
    }

    #[test]
    fn test_excerpt_skips_code_fences() {
        assert_eq!(journal_excerpt("```rs\nlet x = 1;\n```"), "let x = 1;");
    }

    #[test]
    fn test_content_limit_counts_unicode_chars_not_bytes() {
        let at_limit = "日".repeat(MAX_JOURNAL_LENGTH);
        assert_eq!(validate_journal_content(&at_limit), Ok(()));
        let over = "日".repeat(MAX_JOURNAL_LENGTH + 1);
        assert_eq!(validate_journal_content(&over), Err(JournalError::TooLong));
    }

    #[test]
    fn test_new_rejects_blank_content() {
        assert_eq!(
            JournalEntry::new(date(), "  \n\t".into(), instant()),
            Err(JournalError::Empty)
        );
        assert!(JournalEntry::new(date(), "有内容".into(), instant()).is_ok());
    }

    #[test]
    fn test_with_content_only_bumps_time_on_real_change() {
        let entry = JournalEntry::new(date(), "第一版".into(), instant()).unwrap();
        let later = instant() + chrono::Duration::seconds(30);
        let same = entry.with_content("第一版".into(), later).unwrap();
        assert_eq!(same.updated_at, entry.updated_at);
        let next = entry.with_content("第二版".into(), later).unwrap();
        assert!(next.updated_at > entry.updated_at);
        assert_eq!(next.created_at, entry.created_at);
    }

    #[test]
    fn test_with_content_rejects_too_long_and_keeps_original() {
        let entry = JournalEntry::new(date(), "第一版".into(), instant()).unwrap();
        let over = "长".repeat(MAX_JOURNAL_LENGTH + 1);
        assert_eq!(
            entry.with_content(over, instant()),
            Err(JournalError::TooLong)
        );
        assert_eq!(entry.content, "第一版");
    }
}
