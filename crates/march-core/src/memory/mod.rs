use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use indexmap::IndexMap;
use jieba_rs::Jieba;
use lazy_static::lazy_static;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::settings::march_settings_dir;

mod manager;
mod search;
mod storage;
#[cfg(test)]
mod tests;
mod types;

const PROJECT_MEMORY_DIR: &str = ".march/memories";
const LOW_CONTEXT_PRESSURE_SKIP_THRESHOLD: u8 = 95;

lazy_static! {
    static ref JIEBA: Jieba = Jieba::new();
}

pub use manager::MemoryManager;
pub use types::{
    MemorizeRequest, MemoryIndexEntry, MemoryIndexView, MemoryLevel, MemoryQuery, MemoryRecord,
    MemoryScope, UpdateMemoryRequest,
};
