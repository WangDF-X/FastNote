use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use fastnote_core::{DateTime, NaiveDate, Utc};

use super::diagnostic::LogError;

pub(super) struct LogWriter {
    root: PathBuf,
    date: Option<NaiveDate>,
    file: Option<File>,
    bytes: u64,
    part: u32,
    max_bytes: u64,
}

impl LogWriter {
    pub(super) fn new(root: PathBuf, max_bytes: u64) -> Result<Self, LogError> {
        fs::create_dir_all(&root)
            .map_err(|error| LogError::new("create_log_directory", Some(&root), error))?;
        Ok(Self {
            root,
            date: None,
            file: None,
            bytes: 0,
            part: 0,
            max_bytes,
        })
    }

    /// 启动时验证日志文件确实能打开，而不是等待首个事件才发现拒写。
    pub(super) fn prepare(&mut self, now: DateTime<Utc>) -> Result<(), LogError> {
        self.prepare_date(now.date_naive())?;
        self.open_part(0)
    }

    pub(super) fn write(&mut self, now: DateTime<Utc>, line: &str) -> Result<(), LogError> {
        let line = format!("{} {line}\n", now.to_rfc3339());
        if line.len() as u64 > self.max_bytes {
            return Err(LogError::new(
                "validate_log_line",
                Some(&self.root),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "log line exceeds file limit",
                ),
            ));
        }
        self.prepare_date(now.date_naive())?;
        if self.file.is_none() || self.bytes + line.len() as u64 > self.max_bytes {
            self.open_part(line.len() as u64)?;
        }
        let path = self.part_path()?;
        let file = self.file.as_mut().ok_or_else(|| {
            LogError::new(
                "write_log_file",
                Some(&path),
                std::io::Error::other("log file unavailable"),
            )
        })?;
        file.write_all(line.as_bytes())
            .map_err(|error| LogError::new("write_log_file", Some(&path), error))?;
        self.bytes += line.len() as u64;
        Ok(())
    }

    fn prepare_date(&mut self, date: NaiveDate) -> Result<(), LogError> {
        if self.date != Some(date) {
            self.file.take();
            self.retain_seven_days(date)?;
            self.date = Some(date);
            self.part = 0;
            self.bytes = 0;
        }
        Ok(())
    }

    fn part_path(&self) -> Result<PathBuf, LogError> {
        let date = self.date.ok_or_else(|| {
            LogError::new(
                "resolve_log_file",
                Some(&self.root),
                std::io::Error::other("log date unavailable"),
            )
        })?;
        let suffix = if self.part == 0 {
            String::new()
        } else {
            format!(".{}", self.part)
        };
        Ok(self.root.join(format!("fastnote.log.{date}{suffix}")))
    }

    fn open_part(&mut self, next_bytes: u64) -> Result<(), LogError> {
        self.file.take();
        loop {
            let path = self.part_path()?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|error| LogError::new("open_log_file", Some(&path), error))?;
            let bytes = file
                .metadata()
                .map_err(|error| LogError::new("inspect_log_file", Some(&path), error))?
                .len();
            if bytes == 0 || bytes + next_bytes <= self.max_bytes {
                self.bytes = bytes;
                self.file = Some(file);
                return Ok(());
            }
            self.part = self.part.checked_add(1).ok_or_else(|| {
                LogError::new(
                    "rotate_log_file",
                    Some(&self.root),
                    std::io::Error::other("log part exhausted"),
                )
            })?;
        }
    }

    fn retain_seven_days(&self, today: NaiveDate) -> Result<(), LogError> {
        let entries = fs::read_dir(&self.root)
            .map_err(|error| LogError::new("list_log_directory", Some(&self.root), error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| LogError::new("list_log_directory", Some(&self.root), error))?;
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|error| LogError::new("inspect_log_file", Some(&path), error))?
                .is_file()
            {
                continue;
            }
            let name = entry.file_name();
            let Some(suffix) = name
                .to_str()
                .and_then(|name| name.strip_prefix("fastnote.log."))
            else {
                continue;
            };
            let (date, part) = suffix.split_once('.').unwrap_or((suffix, ""));
            if !part.is_empty() && part.parse::<u32>().is_err() {
                continue;
            }
            let Ok(date) = date.parse::<NaiveDate>() else {
                continue;
            };
            if today.signed_duration_since(date).num_days() >= 7 {
                // 只删除本应用明确命名的旧诊断日志，不碰其它文件或用户内容。
                fs::remove_file(&path)
                    .map_err(|error| LogError::new("remove_expired_log", Some(&path), error))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-11-02T08:00:00Z".parse().unwrap()
    }

    #[test]
    fn test_log_directory_failure_reports_its_effective_path_and_phase() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let path = directory.path().join("fake-file");
        fs::write(&path, b"fake").unwrap();
        let error = LogWriter::new(path.clone(), 100).err().unwrap();
        let LogError::Operation {
            phase,
            path: reported_path,
            ..
        } = error;
        assert_eq!(phase, "create_log_directory");
        assert_eq!(reported_path, Some(path));
    }

    #[test]
    fn test_prepare_detects_unwritable_log_file_before_first_event() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let path = directory.path().join("fastnote.log.2026-11-02");
        fs::create_dir(&path).unwrap();
        let mut writer = LogWriter::new(directory.path().to_owned(), 100).unwrap();
        let LogError::Operation {
            phase,
            path: reported_path,
            ..
        } = writer.prepare(now()).unwrap_err();
        assert_eq!(phase, "open_log_file");
        assert_eq!(reported_path, Some(path));
    }

    #[test]
    fn test_prepare_opens_log_without_writing_a_diagnostic_event() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let mut writer = LogWriter::new(directory.path().to_owned(), 100).unwrap();
        writer.prepare(now()).unwrap();
        let path = directory.path().join("fastnote.log.2026-11-02");
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        writer.write(now(), "INFO fake").unwrap();
        assert!(fs::read_to_string(path).unwrap().contains("INFO fake"));
    }

    #[test]
    fn test_log_size_rotation_appends_without_truncating_existing_files() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let mut writer = LogWriter::new(directory.path().to_owned(), 100).unwrap();
        writer.write(now(), "INFO fake-1").unwrap();
        writer.write(now(), "INFO fake-2").unwrap();
        writer.write(now(), "INFO fake-3").unwrap();
        drop(writer);
        let files: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(files.len(), 2);
        assert!(files
            .iter()
            .all(|path| fs::metadata(path).unwrap().len() <= 100));
        let mut writer = LogWriter::new(directory.path().to_owned(), 100).unwrap();
        writer.write(now(), "INFO fake-4").unwrap();
        drop(writer);
        let content = fs::read_to_string(directory.path().join("fastnote.log.2026-11-02")).unwrap();
        assert!(content.contains("fake-1") && content.contains("fake-2"));
    }

    #[test]
    fn test_retention_only_removes_old_owned_log_names_and_keeps_unknown_files() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        for name in [
            "fastnote.log.2026-10-26",
            "fastnote.log.2026-10-26.1",
            "fastnote.log.2026-10-27",
            "fake-user.txt",
            "fastnote.log.2026-10-26.user",
        ] {
            fs::write(directory.path().join(name), "fake").unwrap();
        }
        let mut writer = LogWriter::new(directory.path().to_owned(), 100).unwrap();
        writer.write(now(), "INFO fake").unwrap();
        assert!(!directory.path().join("fastnote.log.2026-10-26").exists());
        assert!(!directory.path().join("fastnote.log.2026-10-26.1").exists());
        assert!(directory.path().join("fastnote.log.2026-10-27").exists());
        assert!(directory.path().join("fake-user.txt").exists());
        assert!(directory
            .path()
            .join("fastnote.log.2026-10-26.user")
            .exists());
    }
}
