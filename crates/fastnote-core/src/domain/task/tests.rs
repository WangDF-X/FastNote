use super::*;

fn task() -> Task {
    Task::new(
        "019a0fff-aaaa-7000-8000-000000000001"
            .parse()
            .expect("fake UUIDv7"),
        "  假任务  ".to_owned(),
        "2026-11-03T12:00:00Z".parse().expect("fixed timestamp"),
    )
    .expect("valid fake task")
}

#[test]
fn test_creation_only_requires_trimmed_title_and_keeps_defaults() {
    let task = task();
    assert_eq!(task.title, "假任务");
    assert_eq!(task.status, Some(TaskStatus::Todo));
    assert_eq!(task.priority, Priority::None);
    assert!(task.start_date.is_none() && task.due_date.is_none());
    assert!(task.plan_id.is_none() && task.parent_id.is_none() && task.completed_at.is_none());
}

#[test]
fn test_title_and_description_limits_count_unicode_characters() {
    let mut task = task();
    for title in ["", " \n\t", "\u{3000}"] {
        task.title = title.to_owned();
        assert_eq!(task.validate(), Err(TaskError::TitleRequired));
    }
    task.title = "中".repeat(200);
    task.description = "文".repeat(10_000);
    assert!(task.validate().is_ok());
    task.title.push('文');
    assert_eq!(task.validate(), Err(TaskError::TitleTooLong));
    task.title.pop();
    task.description.push('文');
    assert_eq!(task.validate(), Err(TaskError::DescriptionTooLong));
}

#[test]
fn test_dates_are_strict_optional_and_bounded() {
    assert_eq!(parse_task_date(""), Ok(None));
    for value in ["1900-01-01", "2024-02-29", "9999-12-31"] {
        assert_eq!(parse_task_date(value).unwrap().unwrap().to_string(), value);
    }
    for value in [
        "1899-12-31",
        "10000-01-01",
        "2026-02-29",
        "2026-1-01",
        "2026-01-1",
        " 2026-01-01",
        "2026-01-01 ",
        "2026-11-03T12:00:00Z",
        "not-a-date",
    ] {
        assert_eq!(parse_task_date(value), Err(TaskError::DateRange), "{value}");
    }
}

#[test]
fn test_patch_keep_set_clear_are_distinct_and_invalid_patch_is_atomic() {
    let task = task();
    let now = task.updated_at + chrono::Duration::seconds(1);
    let date = parse_task_date("2026-11-03").unwrap().unwrap();
    let dated = task
        .patched(
            &TaskPatch {
                start_date: Patch::Set(date),
                due_date: Patch::Set(date),
                description: Some("\n原样正文\n".to_owned()),
                ..Default::default()
            },
            now,
        )
        .unwrap();
    assert_eq!(dated.description, "\n原样正文\n");
    assert_eq!(dated.status, task.status);
    assert_eq!(dated.updated_at, now);
    assert_eq!(
        dated
            .patched(&TaskPatch::default(), now + chrono::Duration::seconds(1))
            .unwrap(),
        dated
    );
    let cleared = dated
        .patched(
            &TaskPatch {
                start_date: Patch::Clear,
                due_date: Patch::Clear,
                ..Default::default()
            },
            now,
        )
        .unwrap();
    assert!(cleared.start_date.is_none() && cleared.due_date.is_none());
    let original = dated.clone();
    assert_eq!(
        dated.patched(
            &TaskPatch {
                start_date: Patch::Set(date.succ_opt().unwrap()),
                ..Default::default()
            },
            now
        ),
        Err(TaskError::DateRange)
    );
    assert_eq!(dated, original);
}

#[test]
fn test_single_date_is_valid_but_out_of_range_date_is_rejected() {
    let mut task = task();
    task.start_date = parse_task_date("2026-11-04").unwrap();
    assert!(task.validate().is_ok());
    task.due_date = parse_task_date("2026-11-03").unwrap();
    assert_eq!(task.validate(), Err(TaskError::DateRange));
    task.start_date = None;
    assert!(task.validate().is_ok());
    task.due_date = Some("1899-12-31".parse().unwrap());
    assert_eq!(task.validate(), Err(TaskError::DateRange));
    task.due_date = Some("+10000-01-01".parse().unwrap());
    assert_eq!(task.validate(), Err(TaskError::DateRange));
}

#[test]
fn test_all_nine_state_transitions_preserve_dates_and_completion_invariant() {
    let original = task();
    let first = original.updated_at + chrono::Duration::seconds(1);
    let second = first + chrono::Duration::seconds(1);
    for from in [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] {
        let mut task = original.with_status(from, first).unwrap();
        task.start_date = parse_task_date("2026-11-03").unwrap();
        task.due_date = parse_task_date("2026-11-04").unwrap();
        task.start_time = parse_task_time("09:00").unwrap();
        task.due_time = parse_task_time("15:30").unwrap();
        for to in [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] {
            let next = task.with_status(to, second).unwrap();
            assert_eq!(next.status, Some(to));
            assert_eq!(next.completed_at.is_some(), to == TaskStatus::Done);
            assert_eq!(next.start_date, task.start_date);
            assert_eq!(next.due_date, task.due_date);
            assert_eq!(next.start_time, task.start_time);
            assert_eq!(next.due_time, task.due_time);
            if from == to {
                assert_eq!(next, task);
            } else {
                assert_eq!(next.status_changed_at, second);
                assert_eq!(next.updated_at, second);
            }
        }
    }
}

#[test]
fn test_task_and_stage_invariants_reject_inconsistent_entities() {
    let mut task = task();
    task.status = None;
    assert_eq!(task.validate(), Err(TaskError::Conflict));
    task.status = Some(TaskStatus::Done);
    assert_eq!(task.validate(), Err(TaskError::Conflict));
    task.kind = TaskKind::Stage;
    task.status = None;
    assert_eq!(task.validate(), Err(TaskError::StageParentInvalid));
    task.plan_id = Some("019a0fff-aaaa-7000-8000-000000000002".parse().unwrap());
    assert!(task.validate().is_ok());
    assert_eq!(
        task.with_status(TaskStatus::Done, task.updated_at),
        Err(TaskError::StageParentInvalid)
    );
    for case in 0..6 {
        let mut next = task.clone();
        match case {
            0 => next.parent_id = Some(task.id),
            1 => next.priority = Priority::High,
            2 => next.status = Some(TaskStatus::Todo),
            3 => next.completed_at = Some(task.updated_at),
            4 => {
                next.start_date = parse_task_date("2026-11-03").unwrap();
                next.start_time = parse_task_time("09:00").unwrap();
            }
            _ => {
                next.due_date = parse_task_date("2026-11-03").unwrap();
                next.due_time = parse_task_time("15:30").unwrap();
            }
        }
        assert_eq!(next.validate(), Err(TaskError::StageParentInvalid));
    }
    task.sort_order = 0;
    assert_eq!(task.validate(), Err(TaskError::Conflict));
}

#[test]
fn test_status_and_priority_boundaries_are_stable() {
    for status in [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] {
        assert_eq!(TaskStatus::parse(status.as_str()), Some(status));
    }
    assert!(TaskStatus::parse("unknown").is_none());
    assert!(TaskStatus::parse("waiting").is_none());
    for priority in [
        Priority::None,
        Priority::Low,
        Priority::Medium,
        Priority::High,
    ] {
        assert_eq!(Priority::parse(priority as i32), Some(priority));
    }
    assert!(Priority::parse(-1).is_none() && Priority::parse(4).is_none());
    let mut priority = Priority::High;
    for next in [
        Priority::Medium,
        Priority::Low,
        Priority::None,
        Priority::High,
    ] {
        priority = priority.next_priority();
        assert_eq!(priority, next);
    }
}

#[test]
fn test_time_is_strict_minute_precision_and_requires_its_date() {
    for value in ["00:00", "09:05", "23:59"] {
        assert_eq!(
            parse_task_time(value)
                .unwrap()
                .unwrap()
                .format("%H:%M")
                .to_string(),
            value
        );
    }
    assert_eq!(parse_task_time(""), Ok(None));
    for value in [
        "24:00", "12:60", "9:05", "09:5", "09:05:00", " 09:05", "09:05 ", "bad",
    ] {
        assert_eq!(parse_task_time(value), Err(TaskError::DateRange));
    }
    let mut task = task();
    task.start_time = parse_task_time("09:00").unwrap();
    assert_eq!(task.validate(), Err(TaskError::DateRange));
    task.start_date = parse_task_date("2026-11-03").unwrap();
    assert!(task.validate().is_ok());
    task.start_time = NaiveTime::from_hms_opt(9, 0, 1);
    assert_eq!(task.validate(), Err(TaskError::DateRange));
    task.start_time = NaiveTime::from_hms_nano_opt(9, 0, 0, 1);
    assert_eq!(task.validate(), Err(TaskError::DateRange));
}

#[test]
fn test_same_day_times_are_ordered_and_date_clear_also_clears_time() {
    let original = task();
    let patch = TaskPatch {
        start_date: Patch::Set("2026-11-03".parse().unwrap()),
        due_date: Patch::Set("2026-11-03".parse().unwrap()),
        start_time: Patch::Set(parse_task_time("09:00").unwrap().unwrap()),
        due_time: Patch::Set(parse_task_time("15:30").unwrap().unwrap()),
        ..Default::default()
    };
    let timed = original.patched(&patch, original.updated_at).unwrap();
    assert_eq!(
        timed.start_time.unwrap().format("%H:%M").to_string(),
        "09:00"
    );
    let invalid = TaskPatch {
        start_time: Patch::Set(parse_task_time("16:00").unwrap().unwrap()),
        ..Default::default()
    };
    assert_eq!(
        timed.patched(&invalid, original.updated_at),
        Err(TaskError::DateRange)
    );
    let equal = timed
        .patched(
            &TaskPatch {
                start_time: patch.due_time.clone(),
                ..Default::default()
            },
            original.updated_at,
        )
        .unwrap();
    assert_eq!(equal.start_time, equal.due_time);
    let clear = timed
        .patched(
            &TaskPatch {
                start_date: Patch::Clear,
                due_date: Patch::Clear,
                ..Default::default()
            },
            original.updated_at,
        )
        .unwrap();
    assert!(
        clear.start_date.is_none()
            && clear.start_time.is_none()
            && clear.due_date.is_none()
            && clear.due_time.is_none()
    );
    assert_eq!(clear.status, original.status);
}
