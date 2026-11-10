use super::*;

fn now() -> DateTime<Utc> {
    "2026-11-09T08:00:00Z".parse().unwrap()
}
fn id() -> NoteId {
    "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap()
}

#[test]
fn test_note_validation_preserves_whitespace_and_unicode_character_limits() {
    let note = Note::new(id(), "\n  假正文 \n".to_owned(), now()).unwrap();
    assert_eq!(note.content, "\n  假正文 \n");
    assert!(validate_note_content(&"文".repeat(20_000)).is_ok());
    assert_eq!(
        validate_note_content(&"文".repeat(20_001)),
        Err(NoteError::TooLong)
    );
    for empty in ["", " \n\t\r", "\u{3000}\u{a0}"] {
        assert_eq!(validate_note_content(empty), Ok(()));
        assert!(Note::new(id(), empty.to_owned(), now()).is_ok());
    }
}

#[test]
fn test_note_updates_are_validated_and_monotonic_but_no_op_keeps_timestamp() {
    let note = Note::new(id(), "原假正文".to_owned(), now()).unwrap();
    assert_eq!(
        note.with_content(note.content.clone(), now()).unwrap(),
        note
    );
    assert_eq!(
        note.with_content(" ".to_owned(), now()).unwrap().content,
        " "
    );
    let next = note.with_content("新假正文".to_owned(), now()).unwrap();
    assert_eq!(next.created_at, note.created_at);
    assert!(next.updated_at > note.updated_at);
    assert_eq!(note.content, "原假正文");
}

#[test]
fn test_excerpt_skips_blank_lines_and_bounds_unicode_title_and_two_line_preview() {
    assert_eq!(
        note_excerpt("\n \t\n 首行 \r\n次行\n第三行"),
        ("首行".to_owned(), "首行 \n次行".to_owned())
    );
    let (title, preview) = note_excerpt(&format!("{}\n第二行", "字".repeat(200)));
    assert_eq!(title.chars().count(), 80);
    assert_eq!(preview.chars().count(), 120);
    assert_eq!(note_excerpt(" \n"), (String::new(), String::new()));
}
