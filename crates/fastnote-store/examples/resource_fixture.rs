//! SHELL-03 P7：独立假数据生成/仓储计时工具；绝不打开默认用户数据目录。

use std::fs::{self, OpenOptions};
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use std::time::{Instant, SystemTime};

use fastnote_core::derive::TodayCollector;
use fastnote_core::{
    Clock, DateTime, DesktopSetting, NaiveDate, NaiveDateTime, Query, QueryResult,
    SettingsRepository, Task, TaskClock, TaskId, TaskRepository, Utc,
};
use fastnote_store::SqliteSettings;
use rusqlite::{params, Connection};
use serde_json::json;

#[path = "resource_fixture/measurement.rs"]
mod measurement;

const MARKER: &str = "fastnote-resource-fixture.json";

struct FixedClock {
    now: DateTime<Utc>,
    date: NaiveDate,
}
impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.date
    }
}
impl TaskClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
    fn local_datetime(&self, at: DateTime<Utc>) -> NaiveDateTime {
        self.date.and_time(at.time())
    }
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

fn create(root: &Path, date: NaiveDate, rows: usize) -> Result<(), Box<dyn std::error::Error>> {
    if !(1_000..=20_000).contains(&rows) {
        return Err(invalid("rows must be 1000..=20000").into());
    }
    // 原子拒绝任何已存在目录，不覆盖旧验收资料或真实库；不执行清理/删除。
    fs::create_dir(root)?;
    let database = root.join("FastNote/fastnote.db");
    let now: DateTime<Utc> = SystemTime::now().into();
    let mut store = SqliteSettings::open(&database, now)?;
    store.save_desktop(&[DesktopSetting::FirstRunDone(true)], now)?;
    drop(store);
    let mut connection = Connection::open(&database)?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    {
        let mut insert = transaction.prepare("INSERT INTO tasks(id,kind,title,description,status,priority,status_changed_at,sort_order,created_at,updated_at) VALUES(?1,'task',?2,?3,'todo',3,?4,?5,?4,?4)")?;
        for number in 1..=rows {
            let id: TaskId = format!("019a0fff-aaaa-7000-8000-{number:012x}").parse()?;
            let at =
                now - chrono::Duration::days(1) + chrono::Duration::milliseconds(number as i64);
            let mut task = Task::new(id, format!("资源验收假任务 {number:05}"), at)?;
            if number == rows {
                task.description = "假".repeat(10_000);
            }
            task.validate()?;
            insert.execute(params![
                task.id.to_string(),
                task.title,
                task.description,
                at.to_rfc3339(),
                number as i64 * 1024
            ])?;
        }
    }
    transaction.commit()?;
    connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    let marker = json!({"fake_only": true, "rows": rows, "date": date.to_string(), "body_chars": 10000, "schema": 3});
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(MARKER))?;
    serde_json::to_writer_pretty(&mut output, &marker)?;
    output.write_all(b"\n")?;
    println!("fake_fixture={} rows={rows} date={date}", root.display());
    Ok(())
}

fn fixture_database(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    // 只有本工具标识的假库才能计时，缺失/非法标识时不打开或修复任何数据库。
    let marker: serde_json::Value = serde_json::from_slice(&fs::read(root.join(MARKER))?)?;
    if marker.get("fake_only").and_then(|value| value.as_bool()) != Some(true) {
        return Err(invalid("not a resource fixture").into());
    }
    let database = root.join("FastNote/fastnote.db");
    if !database.is_file() {
        return Err(invalid("fixture database is missing").into());
    }
    Ok(database)
}

fn benchmark(root: &Path, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    let database = fixture_database(root)?;
    let now: DateTime<Utc> = SystemTime::now().into();
    let store = SqliteSettings::open(&database, now)?;
    for limit in [200, 1000] {
        for _ in 0..10 {
            store.query_tasks(
                Query::Today {
                    date,
                    limits: [limit; 4],
                },
                FixedClock { now, date },
            )?;
        }
        let mut elapsed = Vec::with_capacity(100);
        let mut loaded = 0;
        let mut total = 0;
        for _ in 0..100 {
            let started = Instant::now();
            let result = store.query_tasks(
                Query::Today {
                    date,
                    limits: [limit; 4],
                },
                FixedClock { now, date },
            )?;
            elapsed.push(started.elapsed().as_micros());
            if let QueryResult::Today(view) = result {
                loaded = view
                    .sections
                    .iter()
                    .map(|section| section.items.len())
                    .sum::<usize>();
                total = view.open_count;
            }
        }
        let (median, p95, max) =
            measurement::summarize(&mut elapsed).ok_or_else(|| invalid("no benchmark samples"))?;
        println!("mode={} rows={total} limit={limit} loaded={loaded} samples=100 median_us={median} p95_us={p95} max_us={max}", if cfg!(debug_assertions) { "debug" } else { "release" });
    }
    Ok(())
}

// 诊断而非验收：各阶段串行且物化完整候选，不是生产只读事务/流式路径。
fn diagnose(root: &Path, date: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    let database = fixture_database(root)?;
    let now: DateTime<Utc> = SystemTime::now().into();
    let store = SqliteSettings::open(&database, now)?;
    let preferences = store.today_preferences()?;
    let clock = FixedClock { now, date };
    for limit in [200, 1000] {
        let mut stages = [Vec::new(), Vec::new(), Vec::new()];
        let mut count = 0;
        for iteration in 0..40 {
            let mut tasks = Vec::new();
            let start = Instant::now();
            store.visit_today_tasks(date, &mut |task| tasks.push(task))?;
            let read = start.elapsed().as_micros();
            count = tasks.len();
            let start = Instant::now();
            let mut collector = TodayCollector::new(date, &clock, preferences, [limit; 4]);
            for task in tasks {
                collector.push(task);
            }
            let mut view = collector.finish();
            let group = start.elapsed().as_micros();
            let start = Instant::now();
            store.complete_today_projection(&mut view)?;
            let projection = start.elapsed().as_micros();
            if iteration >= 10 {
                for (samples, elapsed) in stages.iter_mut().zip([read, group, projection]) {
                    samples.push(elapsed);
                }
            }
        }
        for (name, samples) in ["read_decode", "group_sort", "projection"]
            .into_iter()
            .zip(&mut stages)
        {
            let (median, p95, max) =
                measurement::summarize(samples).ok_or_else(|| invalid("no diagnostic samples"))?;
            println!("diagnostic={name} limit={limit} candidates={count} samples=30 median_us={median} p95_us={p95} max_us={max}");
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 3 {
        return Err(invalid(
            "usage: resource_fixture create|benchmark|diagnose ROOT YYYY-MM-DD [ROWS]",
        )
        .into());
    }
    let mode = args[0].to_str().ok_or_else(|| invalid("invalid mode"))?;
    let root = Path::new(&args[1]);
    let date: NaiveDate = args[2]
        .to_str()
        .ok_or_else(|| invalid("invalid date"))?
        .parse()?;
    match mode {
        "create" if args.len() == 4 => create(
            root,
            date,
            args[3]
                .to_str()
                .ok_or_else(|| invalid("invalid rows"))?
                .parse()?,
        ),
        "benchmark" if args.len() == 3 => benchmark(root, date),
        "diagnose" if args.len() == 3 => diagnose(root, date),
        _ => Err(invalid(
            "usage: resource_fixture create|benchmark|diagnose ROOT YYYY-MM-DD [ROWS]",
        )
        .into()),
    }
}
