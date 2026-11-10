//! 将 Markdown 分块交给原生预览，不执行 HTML 或读取远程资源。

use fastnote_ui::NotePreviewBlock;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Default)]
struct Builder {
    blocks: Vec<NotePreviewBlock>,
    text: String,
    plain: String,
    kind: i32,
    quote_depth: i32,
    lists: Vec<Option<u64>>,
    prefix: String,
    code: bool,
    links: Vec<String>,
    number_indent: i32,
    soft_break: bool,
}

impl Builder {
    fn push_text(&mut self, text: &str) {
        self.plain.push_str(text);
        for c in text.chars() {
            if "\\`*_[]<>~".contains(c) {
                self.text.push('\\');
            }
            self.text.push(c);
        }
    }

    fn flush(&mut self) {
        if self.text.is_empty() && self.prefix.is_empty() {
            return;
        }
        let markdown = format!("{}{}", self.prefix, self.text);
        let plain = format!("{}{}", self.prefix, self.plain);
        let styled = if self.code {
            slint::StyledText::from_plain_text(&plain)
        } else {
            slint::StyledText::from_markdown(&markdown)
                .unwrap_or_else(|_| slint::StyledText::from_plain_text(&plain))
        };
        self.blocks.push(NotePreviewBlock {
            text: styled,
            kind: self.kind,
            indent: self.quote_depth
                + self.lists.len().saturating_sub(1) as i32
                + self.number_indent,
        });
        self.text.clear();
        self.plain.clear();
        self.prefix.clear();
        self.number_indent = 0;
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.kind = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
            }
            Tag::Paragraph => {}
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
                self.prefix = "│ ".into();
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.kind = 7;
                self.code = true;
            }
            Tag::List(start) => self.lists.push(start),
            Tag::Item => {
                self.flush();
                self.prefix = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let current = *n;
                        *n = n.saturating_add(1);
                        format!("{current}. ")
                    }
                    _ => "• ".to_owned(),
                };
            }
            Tag::Strong => self.text.push_str("**"),
            Tag::Emphasis => self.text.push('*'),
            Tag::Strikethrough => self.text.push_str("~~"),
            Tag::Link { dest_url, .. } => {
                self.text.push('[');
                self.links.push(dest_url.to_string());
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.flush();
                self.kind = 0;
            }
            TagEnd::Paragraph | TagEnd::Item => self.flush(),
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.code = false;
                self.kind = 0;
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Strong => self.text.push_str("**"),
            TagEnd::Emphasis => self.text.push('*'),
            TagEnd::Strikethrough => self.text.push_str("~~"),
            TagEnd::Link => {
                if let Some(url) = self.links.pop() {
                    if url.starts_with("https://") || url.starts_with("http://") {
                        self.text.push_str("](");
                        self.text.push_str(&url.replace(')', "%29"));
                        self.text.push(')');
                    } else {
                        self.text.push_str("](about:blank)");
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn preview_blocks(source: &str) -> Vec<NotePreviewBlock> {
    let mut blocks = Vec::new();
    let mut pending = String::new();
    let mut in_list = false;
    let mut fence: Option<(char, usize)> = None;
    for line in source.split_inclusive('\n') {
        let spaces = line.bytes().take_while(|byte| *byte == b' ').count();
        let trimmed = &line[spaces..];
        if spaces <= 3 {
            let c = trimmed.chars().next();
            if matches!(c, Some('`' | '~')) {
                let c = c.unwrap_or('`');
                let count = trimmed.chars().take_while(|ch| *ch == c).count();
                if count >= 3 {
                    match fence {
                        None => fence = Some((c, count)),
                        Some((open, n))
                            if c == open && count >= n && trimmed[count..].trim().is_empty() =>
                        {
                            fence = None
                        }
                        _ => {}
                    }
                    pending.push_str(line);
                    continue;
                }
            }
        }
        if fence.is_none() {
            if trimmed.trim().is_empty() {
                in_list = false;
            } else if let Some((marker_end, chinese)) = list_marker(trimmed) {
                if spaces == 0 || in_list {
                    in_list = true;
                    if spaces > 0 || chinese {
                        blocks.extend(parse_markdown(&pending));
                        pending.clear();
                        let marker = &trimmed[..marker_end];
                        let body = &trimmed[marker_end..];
                        // Escape list punctuation so Slint preserves the user's exact marker.
                        let marker = marker.replace('.', "\\.");
                        let mut item = parse_markdown(&format!("{marker}{body}"));
                        for block in &mut item {
                            block.indent = (spaces / 2).min(12) as i32;
                        }
                        blocks.extend(item);
                        continue;
                    }
                }
            } else if spaces == 0 {
                in_list = false;
            }
        }
        pending.push_str(line);
    }
    blocks.extend(parse_markdown(&pending));
    blocks
}

// Returns the byte offset after a numbered marker and whether it is Chinese.
fn list_marker(line: &str) -> Option<(usize, bool)> {
    let mut chars = line.char_indices().peekable();
    let (first_at, first) = chars.next()?;
    if first_at != 0 {
        return None;
    }
    let parenthesized = first == '(' || first == '（';
    let start = if parenthesized {
        chars.next()?.1
    } else {
        first
    };
    let chinese = "零〇一二三四五六七八九十百千万两".contains(start);
    if !chinese && !start.is_ascii_digit() {
        return None;
    }
    let mut end = if parenthesized {
        first.len_utf8() + start.len_utf8()
    } else {
        start.len_utf8()
    };
    while let Some(&(index, ch)) = chars.peek() {
        if (chinese && "零〇一二三四五六七八九十百千万两".contains(ch))
            || (!chinese && ch.is_ascii_digit())
        {
            chars.next();
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    let (index, delimiter) = chars.next()?;
    if parenthesized {
        if !matches!((first, delimiter), ('(', ')') | ('（', '）')) {
            return None;
        }
    } else if !matches!(delimiter, '.' | '、' | '．' | ')') {
        return None;
    }
    end = end.max(index + delimiter.len_utf8());
    if !chinese
        && !parenthesized
        && delimiter == '.'
        && !line[end..].starts_with(char::is_whitespace)
    {
        return None;
    }
    Some((end, chinese))
}

fn parse_markdown(source: &str) -> Vec<NotePreviewBlock> {
    let mut builder = Builder::default();
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) => builder.start(tag),
            Event::End(tag) => builder.end(tag),
            Event::Text(text) => {
                if builder.soft_break {
                    if let Some(depth) = numbered_depth(&text) {
                        builder.flush();
                        builder.number_indent = depth;
                    } else {
                        builder.text.push('\n');
                        builder.plain.push('\n');
                    }
                    builder.soft_break = false;
                } else if builder.text.is_empty() {
                    builder.number_indent = numbered_depth(&text).unwrap_or(0);
                }
                builder.push_text(&text);
            }
            Event::Code(text) => {
                builder.text.push('`');
                builder.push_text(&text);
                builder.text.push('`');
            }
            Event::SoftBreak => builder.soft_break = true,
            Event::HardBreak => {
                builder.text.push('\n');
                builder.plain.push('\n');
            }
            Event::TaskListMarker(checked) => {
                builder.push_text(if checked { "☑ " } else { "☐ " })
            }
            Event::Html(_)
            | Event::InlineHtml(_)
            | Event::Rule
            | Event::FootnoteReference(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => builder.push_text(&source[range]),
        }
    }
    builder.flush();
    builder.blocks
}

// 仅将以空白分隔的多段纯数字编号视为手写层级，避免将版本号当作层级。
fn numbered_depth(text: &str) -> Option<i32> {
    let marker = text
        .split_once(char::is_whitespace)?
        .0
        .trim_end_matches('.');
    let parts: Vec<_> = marker.split('.').collect();
    if parts.len() < 2
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    Some(parts.len().saturating_sub(1).min(12) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mixed_markdown_is_split_without_whole_document_fallback() {
        let blocks =
            preview_blocks("# 标题\n\n**粗体**\n\n> 引用\n\n```rs\nlet x = 1;\n```\n\n- [x] 完成");
        assert_eq!(blocks.len(), 5);
        assert_eq!(blocks[0].kind, 1);
        assert_eq!(blocks[2].indent, 1);
        assert_eq!(blocks[3].kind, 7);
    }

    #[test]
    fn test_unsupported_html_does_not_prevent_later_heading() {
        let blocks = preview_blocks("<script>alert(1)</script>\n\n# 标题");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].kind, 1);
    }

    #[test]
    fn test_inline_formats_links_and_lists_are_styled_separately() {
        let blocks = preview_blocks(
            "**粗体** *斜体* ~~删除~~ `代码` [链接](https://example.org)\n\n1. 第一\n2. 第二",
        );
        assert_eq!(blocks.len(), 3);
        assert_eq!(
            blocks[0].text,
            slint::StyledText::from_markdown(
                "**粗体** *斜体* ~~删除~~ `代码` [链接](https://example.org)"
            )
            .unwrap()
        );
        assert_eq!(
            blocks[1].text,
            slint::StyledText::from_markdown("1. 第一").unwrap()
        );
        assert_eq!(
            blocks[2].text,
            slint::StyledText::from_markdown("2. 第二").unwrap()
        );
    }

    #[test]
    fn test_manually_typed_hierarchical_numbers_indent_without_rewriting() {
        let blocks = preview_blocks("1. 遵纪守法\n1.1 遵纪\n1.1.1 zunji\n1.2 守法");
        assert_eq!(
            blocks.iter().map(|b| b.indent).collect::<Vec<_>>(),
            vec![0, 1, 2, 1]
        );
        assert_eq!(
            blocks[1].text,
            slint::StyledText::from_plain_text("1.1 遵纪")
        );
        assert_eq!(numbered_depth("v1.2 版本"), None);
    }

    #[test]
    fn test_mixed_chinese_and_arabic_items_follow_leading_spaces() {
        let input = "一、父级\n  1. 子级\n    二、再下一级\n  三、另一个子级\n\n这里是正文";
        let blocks = preview_blocks(input);
        assert_eq!(
            blocks.iter().map(|b| b.indent).collect::<Vec<_>>(),
            vec![0, 1, 2, 1, 0]
        );
        assert_eq!(
            blocks[0].text,
            slint::StyledText::from_plain_text("一、父级")
        );
        assert_eq!(
            blocks[1].text,
            slint::StyledText::from_plain_text("1. 子级")
        );
    }

    #[test]
    fn test_numbered_text_in_fenced_and_indented_code_stays_code() {
        for input in ["```\n    一、代码\n```", "    1. code"] {
            let blocks = preview_blocks(input);
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].kind, 7);
        }
    }
}
