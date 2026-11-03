use std::time::Duration;

use fastnote_core::{CloseBehavior, EntryMode, ThemeMode};

use super::*;

fn receive(worker: &DbWorker) -> Reply {
    worker
        .replies
        .as_ref()
        .expect("test worker receiver")
        .recv_timeout(Duration::from_secs(5))
        .expect("test worker reply")
}

#[test]
fn test_worker_owns_connection_saves_in_fifo_order_and_joins() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake.db");
    let worker = DbWorker::spawn(Some(path.clone()), Arc::new(|| {})).unwrap();
    assert!(matches!(receive(&worker), Reply::Loaded(Ok(_))));
    worker
        .save(1, vec![DesktopSetting::EntryMode(EntryMode::Tray)], false)
        .unwrap();
    worker
        .save(
            2,
            vec![
                DesktopSetting::CloseBehavior(CloseBehavior::Quit),
                DesktopSetting::Theme(ThemeMode::Dark),
            ],
            true,
        )
        .unwrap();
    assert!(matches!(
        receive(&worker),
        Reply::Saved {
            id: 1,
            result: Ok(())
        }
    ));
    assert!(matches!(
        receive(&worker),
        Reply::Saved {
            id: 2,
            result: Ok(())
        }
    ));
    drop(worker);
    let store = SqliteSettings::open(&path, timestamp()).unwrap();
    let settings = store.load_desktop().unwrap();
    assert_eq!(settings.entry_mode, EntryMode::Tray);
    assert_eq!(settings.close_behavior, CloseBehavior::Quit);
    assert_eq!(settings.theme, ThemeMode::Dark);
}

#[test]
fn test_failed_open_can_retry_without_rebuilding_corrupt_data() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let path = directory.path().join("fake.db");
    std::fs::write(&path, b"fake corrupt bytes").unwrap();
    let worker = DbWorker::spawn(Some(path.clone()), Arc::new(|| {})).unwrap();
    assert!(matches!(receive(&worker), Reply::Loaded(Err(_))));
    worker.reload().unwrap();
    assert!(matches!(receive(&worker), Reply::Loaded(Err(_))));
    assert_eq!(std::fs::read(path).unwrap(), b"fake corrupt bytes");
    drop(worker);
}

#[test]
fn test_save_reports_unavailable_store_without_panicking() {
    let error = save(
        &mut None,
        Some(Path::new("fake.db")),
        &[DesktopSetting::EntryMode(EntryMode::Tray)],
        true,
    )
    .unwrap_err();
    assert!(
        matches!(error, StoreError::File { source, .. } if source.kind() == std::io::ErrorKind::NotConnected)
    );
}

#[test]
fn test_bounded_command_backpressure_and_disconnection_are_explicit() {
    let (sender, receiver) = bounded(256);
    let worker = DbWorker {
        commands: Some(sender),
        replies: None,
        thread: None,
    };
    for _ in 0..256 {
        worker.reload().unwrap();
    }
    assert_eq!(worker.reload(), Err(SendError::Full));
    drop(receiver);
    assert_eq!(worker.reload(), Err(SendError::Stopped));
}

#[test]
fn test_settings_and_task_commands_share_one_fifo_connection_and_committed_query() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let worker = DbWorker::spawn(Some(directory.path().join("fake.db")), Arc::new(|| {})).unwrap();
    assert!(matches!(receive(&worker), Reply::Loaded(Ok(_))));
    let id = "019a0fff-aaaa-7000-8000-000000000001".parse().unwrap();
    worker
        .execute(
            1,
            TaskCommand::CreateTask {
                id,
                title: "假任务".to_owned(),
            },
            "test",
        )
        .unwrap();
    worker
        .save(2, vec![DesktopSetting::Theme(ThemeMode::Dark)], false)
        .unwrap();
    worker.query(3, Query::TaskDetail(id)).unwrap();
    assert!(matches!(
        receive(&worker),
        Reply::Executed {
            id: 1,
            result: Ok(_)
        }
    ));
    assert!(matches!(
        receive(&worker),
        Reply::Saved {
            id: 2,
            result: Ok(())
        }
    ));
    let Reply::Queried {
        id: 3,
        result: Ok(QueryResult::TaskDetail(detail)),
    } = receive(&worker)
    else {
        panic!("committed detail expected")
    };
    assert_eq!(detail.task.title, "假任务");
    drop(worker);
}
