use fastnote_core::{
    Clock, DateTime, NaiveDate, NaiveDateTime, Task, TaskClock, TaskId, TaskRepository, Utc,
};
use fastnote_store::SqliteSettings;
use rusqlite::Connection;

pub const PLAN: &str = "019a0fff-aaaa-7000-8000-0000000000ff";

pub fn now() -> DateTime<Utc> {
    "2026-11-03T12:00:00Z"
        .parse()
        .expect("fixed test timestamp")
}
pub fn id(number: usize) -> TaskId {
    format!("019a0fff-aaaa-7000-8000-{number:012x}")
        .parse()
        .expect("fake UUIDv7")
}

#[derive(Clone)]
pub struct FixedClock(pub DateTime<Utc>, pub i64);
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.local_date(self.0)
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        (at + chrono::Duration::hours(self.1)).naive_utc()
    }
}

pub struct Fixture {
    pub store: SqliteSettings,
    pub path: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR"))
            .expect("temporary fake database directory");
        let path = directory.path().join("fake-tasks.db");
        let store = SqliteSettings::open(&path, now()).expect("initialize fake schema");
        Self {
            store,
            path,
            _directory: directory,
        }
    }

    pub fn external(&self) -> Connection {
        let connection = Connection::open(&self.path).expect("open fake test database");
        connection
            .execute_batch("PRAGMA foreign_keys=ON;")
            .expect("enable test foreign keys");
        connection
    }

    pub fn add(&mut self, number: usize) -> Task {
        let task =
            Task::new(id(number), format!("假任务 {number}"), now()).expect("valid fake task");
        self.store.insert_task(&task).expect("insert fake task");
        self.store
            .task_detail(task.id)
            .expect("read fake task")
            .expect("task exists")
            .task
    }

    pub fn plan(&self) {
        self.external()
            .execute(
                "INSERT INTO plans(id,name,created_at,updated_at) VALUES(?1,'假计划',?2,?2)",
                rusqlite::params![PLAN, now().to_rfc3339()],
            )
            .expect("insert fake plan");
    }
}
