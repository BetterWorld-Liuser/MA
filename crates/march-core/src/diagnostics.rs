use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use crate::paths::clean_path;

const DIAGNOSTICS_DIR: &str = "diagnostics";
const BACKEND_LOG_FILENAME: &str = "backend.log";
const FRONTEND_LOG_FILENAME: &str = "frontend.log";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl DiagnosticLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticChannel {
    Backend,
    Frontend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    pub timestamp_ms: u64,
    pub level: DiagnosticLevel,
    pub channel: DiagnosticChannel,
    pub scope: String,
    pub event: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticPaths {
    pub root_dir: PathBuf,
    pub backend_path: PathBuf,
    pub frontend_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DiagnosticLogger {
    paths: DiagnosticPaths,
}

impl DiagnosticLogger {
    pub fn new(project_root: &Path) -> Result<Self> {
        let root_dir = clean_path(project_root.join(".march").join(DIAGNOSTICS_DIR));
        fs::create_dir_all(&root_dir)
            .with_context(|| format!("failed to create {}", root_dir.display()))?;

        Ok(Self {
            paths: DiagnosticPaths {
                backend_path: root_dir.join(BACKEND_LOG_FILENAME),
                frontend_path: root_dir.join(FRONTEND_LOG_FILENAME),
                root_dir,
            },
        })
    }

    pub fn paths(&self) -> &DiagnosticPaths {
        &self.paths
    }

    pub fn write_backend(&self, record: DiagnosticRecord) -> Result<()> {
        self.write(DiagnosticChannel::Backend, record)
    }

    pub fn write_frontend(&self, record: DiagnosticRecord) -> Result<()> {
        self.write(DiagnosticChannel::Frontend, record)
    }

    fn write(&self, expected_channel: DiagnosticChannel, record: DiagnosticRecord) -> Result<()> {
        debug_assert_eq!(
            record.channel, expected_channel,
            "diagnostic record channel should match write target"
        );

        fs::create_dir_all(&self.paths.root_dir)
            .with_context(|| format!("failed to create {}", self.paths.root_dir.display()))?;

        let path = match expected_channel {
            DiagnosticChannel::Backend => &self.paths.backend_path,
            DiagnosticChannel::Frontend => &self.paths.frontend_path,
        };
        let line = serialize_record(&record);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        file.write_all(line.as_bytes())
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

pub fn serialize_record(record: &DiagnosticRecord) -> String {
    let mut line = format!(
        "[{}] {} {} {}",
        format_timestamp_ms(record.timestamp_ms),
        record.level.as_str(),
        record.scope,
        record.event
    );

    if !record.message.trim().is_empty() {
        line.push(' ');
        line.push_str(&sanitize_segment(&record.message));
    }

    for (key, value) in &record.fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(&sanitize_segment(value));
    }

    line.push('\n');
    line
}

fn format_timestamp_ms(timestamp_ms: u64) -> String {
    let seconds = timestamp_ms / 1000;
    let millis = timestamp_ms % 1000;
    format!("{seconds}.{millis:03}")
}

fn sanitize_segment(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn now_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        DiagnosticChannel, DiagnosticLevel, DiagnosticLogger, DiagnosticRecord, serialize_record,
    };

    #[test]
    fn new_creates_project_scoped_diagnostics_directory() {
        let fixture = TestFixture::new("diagnostic-logger-new");
        let logger = DiagnosticLogger::new(&fixture.project_root).expect("create logger");

        assert!(logger.paths().root_dir.is_dir());
        assert_eq!(
            logger.paths().root_dir,
            fixture.project_root.join(".march").join("diagnostics")
        );
    }

    #[test]
    fn backend_and_frontend_records_are_written_to_separate_files() {
        let fixture = TestFixture::new("diagnostic-logger-write");
        let logger = DiagnosticLogger::new(&fixture.project_root).expect("create logger");

        logger
            .write_backend(DiagnosticRecord {
                timestamp_ms: 1,
                level: DiagnosticLevel::Info,
                channel: DiagnosticChannel::Backend,
                scope: "agent-loop".to_string(),
                event: "turn.started".to_string(),
                message: "starting".to_string(),
                fields: BTreeMap::from([(String::from("turn_id"), String::from("t1"))]),
            })
            .expect("write backend record");
        logger
            .write_frontend(DiagnosticRecord {
                timestamp_ms: 2,
                level: DiagnosticLevel::Debug,
                channel: DiagnosticChannel::Frontend,
                scope: "workspace-app".to_string(),
                event: "event.received".to_string(),
                message: "received payload".to_string(),
                fields: BTreeMap::from([(String::from("task_id"), String::from("7"))]),
            })
            .expect("write frontend record");

        let backend =
            std::fs::read_to_string(logger.paths().backend_path.clone()).expect("read backend log");
        let frontend = std::fs::read_to_string(logger.paths().frontend_path.clone())
            .expect("read frontend log");

        assert!(backend.contains("INFO agent-loop turn.started starting turn_id=t1"));
        assert!(!backend.contains("workspace-app"));
        assert!(frontend.contains("DEBUG workspace-app event.received received payload task_id=7"));
        assert!(!frontend.contains("agent-loop"));
        assert!(
            !fixture
                .project_root
                .join(".march")
                .join("debug")
                .join("backend.log")
                .exists()
        );
    }

    #[test]
    fn serialize_record_flattens_whitespace_in_message_and_fields() {
        let line = serialize_record(&DiagnosticRecord {
            timestamp_ms: 42,
            level: DiagnosticLevel::Warn,
            channel: DiagnosticChannel::Backend,
            scope: "command".to_string(),
            event: "timed_out".to_string(),
            message: "timed  out\nnow".to_string(),
            fields: BTreeMap::from([(String::from("reason"), String::from("took\n too long"))]),
        });

        assert_eq!(
            line,
            "[0.042] WARN command timed_out timed out now reason=took too long\n"
        );
    }

    struct TestFixture {
        project_root: PathBuf,
    }

    impl TestFixture {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos();
            let project_root =
                std::env::temp_dir().join(format!("march-diagnostics-{prefix}-{unique}"));
            std::fs::create_dir_all(project_root.join(".march")).expect("create .march");
            Self { project_root }
        }
    }
}
