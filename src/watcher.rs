use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::context::{FileSnapshot, ModifiedBy};

/// watcher 的内存态存储。
/// 这里不负责“如何执行命令”，只负责维护当前被 watch 文件的真实快照和最近的 agent 写入窗口。
#[derive(Debug, Clone)]
pub struct WatchedFileStore {
    state: Arc<RwLock<WatchState>>,
    agent_writes: Arc<Mutex<Vec<AgentWriteRecord>>>,
}

impl WatchedFileStore {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(WatchState::default())),
            agent_writes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 注册一个新文件并立即从磁盘读取首份快照。
    /// 这一步会把当前磁盘状态设成 source of truth，而不是信任调用方传入的内容。
    pub fn register_file(&self, path: impl Into<PathBuf>) -> Result<FileSnapshot> {
        let path = normalize_path(path.into())?;
        let snapshot = snapshot_from_disk(&path, ModifiedBy::Unknown)?;
        self.state
            .write()
            .expect("watch state poisoned")
            .snapshots
            .insert(path, snapshot.clone());
        Ok(snapshot)
    }

    pub fn refresh_file(&self, path: impl AsRef<Path>, modified_by: ModifiedBy) -> Result<()> {
        let path = normalize_path(path.as_ref().to_path_buf())?;
        let snapshot = snapshot_from_disk(&path, modified_by)?;
        self.state
            .write()
            .expect("watch state poisoned")
            .snapshots
            .insert(path, snapshot);
        Ok(())
    }

    pub fn remove_file(&self, path: impl AsRef<Path>, modified_by: ModifiedBy) -> Result<()> {
        let path = normalize_path(path.as_ref().to_path_buf())?;
        self.state
            .write()
            .expect("watch state poisoned")
            .mark_missing(path, modified_by);
        Ok(())
    }

    pub fn snapshots(&self) -> BTreeMap<PathBuf, FileSnapshot> {
        self.state
            .read()
            .expect("watch state poisoned")
            .snapshots
            .clone()
    }

    /// 在 agent 即将触碰某些文件时打开一个时间窗口。
    /// watcher 线程收到文件事件后，会用这个窗口判断改动更像是 Agent 还是 External。
    pub fn begin_agent_write<I>(&self, paths: I) -> Result<AgentWriteGuard>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let paths = paths
            .into_iter()
            .map(normalize_path)
            .collect::<Result<Vec<_>>>()?;
        let record = AgentWriteRecord {
            paths: paths.clone(),
            started_at: SystemTime::now(),
            finished_at: None,
        };

        self.agent_writes
            .lock()
            .expect("agent writes poisoned")
            .push(record);

        Ok(AgentWriteGuard {
            agent_writes: Arc::clone(&self.agent_writes),
            paths,
        })
    }
}

impl Default for WatchedFileStore {
    fn default() -> Self {
        Self::new()
    }
}

/// notify watcher 的薄封装。
/// 目前先支持逐文件 watch，后面如果要扩展到目录级别，也尽量把逻辑收敛在这里。
pub struct FileWatcherService {
    store: WatchedFileStore,
    watcher: RecommendedWatcher,
}

impl FileWatcherService {
    pub fn new() -> Result<Self> {
        let store = WatchedFileStore::new();
        let state = Arc::clone(&store.state);
        let agent_writes = Arc::clone(&store.agent_writes);

        // notify 在后台线程推送事件；这里一旦收到事件，就马上回到磁盘读取最新内容，
        // 保证内存里留下的是“文件现在长什么样”，而不是某次工具调用的旧文本。
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                for path in event.paths {
                    let Some(normalized) = path.canonicalize().ok() else {
                        continue;
                    };

                    let should_track = state
                        .read()
                        .expect("watch state poisoned")
                        .snapshots
                        .contains_key(&normalized);
                    if !should_track {
                        continue;
                    }

                    let modified_by = classify_change(&normalized, &agent_writes);
                    let snapshot =
                        snapshot_from_disk(&normalized, modified_by).unwrap_or_else(|_| {
                            FileSnapshot::new(
                                normalized.clone(),
                                String::new(),
                                SystemTime::now(),
                                modified_by,
                            )
                        });

                    state
                        .write()
                        .expect("watch state poisoned")
                        .snapshots
                        .insert(normalized, snapshot);
                }
            }
        })
        .context("failed to create file watcher")?;

        Ok(Self { store, watcher })
    }

    pub fn watch_file(&mut self, path: impl Into<PathBuf>) -> Result<FileSnapshot> {
        let path = normalize_path(path.into())?;
        let snapshot = self.store.register_file(path.clone())?;
        self.watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .with_context(|| format!("failed to watch {}", path.display()))?;
        Ok(snapshot)
    }

    pub fn store(&self) -> WatchedFileStore {
        self.store.clone()
    }
}

#[derive(Debug, Default)]
struct WatchState {
    snapshots: BTreeMap<PathBuf, FileSnapshot>,
}

impl WatchState {
    fn mark_missing(&mut self, path: PathBuf, modified_by: ModifiedBy) {
        self.snapshots.insert(
            path.clone(),
            FileSnapshot::new(path, String::new(), SystemTime::now(), modified_by),
        );
    }
}

#[derive(Debug, Clone)]
struct AgentWriteRecord {
    paths: Vec<PathBuf>,
    started_at: SystemTime,
    finished_at: Option<SystemTime>,
}

pub struct AgentWriteGuard {
    agent_writes: Arc<Mutex<Vec<AgentWriteRecord>>>,
    paths: Vec<PathBuf>,
}

impl Drop for AgentWriteGuard {
    fn drop(&mut self) {
        // guard 被释放代表一轮 agent 写入动作结束。
        // 我们保留一个很短的尾部窗口，吸收编辑器/文件系统的延迟写事件。
        let now = SystemTime::now();
        let mut writes = self.agent_writes.lock().expect("agent writes poisoned");

        if let Some(record) = writes
            .iter_mut()
            .rev()
            .find(|record| record.paths == self.paths && record.finished_at.is_none())
        {
            record.finished_at = Some(now);
        }

        writes.retain(|record| {
            let Some(finished_at) = record.finished_at else {
                return true;
            };

            now.duration_since(finished_at).unwrap_or_default() <= Duration::from_secs(2)
        });
    }
}

/// 归因逻辑目前采用“路径匹配 + 时间窗口”的保守实现。
/// 它不追求绝对准确，但足够支撑当前阶段区分 agent 写入和外部改动。
fn classify_change(path: &Path, agent_writes: &Arc<Mutex<Vec<AgentWriteRecord>>>) -> ModifiedBy {
    let now = SystemTime::now();
    let writes = agent_writes.lock().expect("agent writes poisoned");

    let matched_agent_write = writes.iter().any(|record| {
        if !record.paths.iter().any(|tracked| tracked == path) {
            return false;
        }

        let end = record.finished_at.unwrap_or(now + Duration::from_secs(2));
        now >= record.started_at && now <= end + Duration::from_secs(2)
    });

    if matched_agent_write {
        ModifiedBy::Agent
    } else {
        ModifiedBy::External
    }
}

/// 每次都回到磁盘读完整文件，是当前阶段最符合设计目标的做法：
/// 简单、确定，而且不会把陈旧上下文长期留在内存里冒充真相。
fn snapshot_from_disk(path: &Path, modified_by: ModifiedBy) -> Result<FileSnapshot> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let last_modified = metadata.modified().unwrap_or_else(|_| SystemTime::now());

    Ok(FileSnapshot::new(
        path.to_path_buf(),
        content,
        last_modified,
        modified_by,
    ))
}

/// 统一做 canonicalize，避免同一文件因为相对/绝对路径差异在 store 中出现重复 key。
fn normalize_path(path: PathBuf) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    Ok(clean_path(canonical))
}

/// Windows 的 canonical path 可能带 `\\?\` 前缀，内部逻辑能处理，
/// 但打印到 prompt 和日志里会很刺眼，所以在展示层提前清理掉。
fn clean_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::Path;

        let raw = path.to_string_lossy();
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            return Path::new(stripped).to_path_buf();
        }
    }

    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_file_creates_initial_unknown_snapshot() {
        let store = WatchedFileStore::new();
        let path = std::env::current_dir()
            .expect("cwd")
            .join("src")
            .join("main.rs");

        let snapshot = store.register_file(path).expect("register file");

        assert_eq!(snapshot.last_modified_by, ModifiedBy::Unknown);
        assert!(!snapshot.content.is_empty());
        assert_eq!(store.snapshots().len(), 1);
    }

    #[test]
    fn classify_change_marks_agent_activity_when_write_window_is_open() {
        let path = std::env::current_dir()
            .expect("cwd")
            .join("src")
            .join("main.rs")
            .canonicalize()
            .expect("canonical path");
        let writes = Arc::new(Mutex::new(vec![AgentWriteRecord {
            paths: vec![path.clone()],
            started_at: SystemTime::now() - Duration::from_millis(50),
            finished_at: Some(SystemTime::now()),
        }]));

        let modified_by = classify_change(&path, &writes);

        assert_eq!(modified_by, ModifiedBy::Agent);
    }
}
