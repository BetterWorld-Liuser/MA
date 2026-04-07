use std::sync::{Mutex, MutexGuard};

use super::*;

lazy_static! {
    static ref TEST_ENV_LOCK: Mutex<()> = Mutex::new(());
}

struct TestEnvGuard {
    previous_settings_dir: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous_settings_dir.take() {
            unsafe { std::env::set_var("MA_SETTINGS_DIR", previous) };
        } else {
            unsafe { std::env::remove_var("MA_SETTINGS_DIR") };
        }
    }
}

fn with_test_manager(name: &str) -> (MemoryManager, TestEnvGuard, PathBuf) {
    let lock = TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ma-memory-{name}-{unique}"));
    let project_root = root.join("project");
    let settings_root = root.join("settings");
    fs::create_dir_all(&project_root).expect("failed to create temp project dir");
    fs::create_dir_all(&settings_root).expect("failed to create temp settings dir");

    let previous_settings_dir = std::env::var_os("MA_SETTINGS_DIR");
    unsafe { std::env::set_var("MA_SETTINGS_DIR", &settings_root) };
    let guard = TestEnvGuard {
        previous_settings_dir,
        _lock: lock,
    };
    let manager = MemoryManager::load(&project_root).expect("failed to load memory manager");
    (manager, guard, project_root)
}

#[test]
fn tokenization_keeps_cjk_and_path_words_searchable() {
    let tokens = search::tokenize_terms("src/auth/middleware.rs 刷新令牌");
    assert!(tokens.contains(&"src".to_string()));
    assert!(tokens.contains(&"auth".to_string()));
    assert!(tokens.contains(&"middleware".to_string()));
    assert!(tokens.iter().any(|token| token.contains("刷新")));
}

#[test]
fn memory_body_parsing_extracts_heading_as_title() {
    let (title, content) = storage::parse_memory_body("# JWT 规则\n\naccess token 15 分钟");
    assert_eq!(title, "JWT 规则");
    assert_eq!(content, "access token 15 分钟");
}

#[test]
fn global_memorize_reuses_string_stable_id() {
    let (mut manager, _guard, _project_root) = with_test_manager("global-stable-id");

    let first = manager
        .memorize(
            MemorizeRequest {
                id: "user-style".to_string(),
                memory_type: "preference".to_string(),
                topic: "style".to_string(),
                title: "Avoid noisy comments".to_string(),
                content: "Only comment non-obvious code paths.".to_string(),
                tags: vec!["style".to_string()],
                scope: None,
                level: Some("global".to_string()),
            },
            "march",
        )
        .expect("failed to persist first global memory");
    let second = manager
        .memorize(
            MemorizeRequest {
                id: "user-style".to_string(),
                memory_type: "preference".to_string(),
                topic: "style".to_string(),
                title: "Prefer sparse comments".to_string(),
                content: "Comments should stay rare and intentional.".to_string(),
                tags: vec!["style".to_string(), "comments".to_string()],
                scope: None,
                level: Some("global".to_string()),
            },
            "march",
        )
        .expect("failed to overwrite global memory");

    let visible = manager
        .list_visible("march")
        .expect("failed to list visible memories");
    let globals = visible
        .into_iter()
        .filter(|memory| memory.level == MemoryLevel::Global)
        .collect::<Vec<_>>();
    assert_eq!(globals.len(), 1, "global memory should overwrite in place");
    assert_eq!(
        first.id, second.id,
        "global memory id should stay stable across updates"
    );
    assert_eq!(globals[0].title, "Prefer sparse comments");
    assert_eq!(globals[0].stable_id, "user-style");
}

#[test]
fn recall_updates_usage_without_touching_updated_at() {
    let (mut manager, _guard, _project_root) = with_test_manager("recall-side-effects");

    let stored = manager
        .memorize(
            MemorizeRequest {
                id: "auth-policy".to_string(),
                memory_type: "fact".to_string(),
                topic: "auth".to_string(),
                title: "JWT refresh policy".to_string(),
                content: "Refresh token lives in httpOnly cookie.".to_string(),
                tags: vec!["auth".to_string()],
                scope: None,
                level: Some("project".to_string()),
            },
            "march",
        )
        .expect("failed to store project memory");

    let viewed = manager
        .peek("p:auth-policy", "march")
        .expect("peek should read without mutation");
    assert_eq!(viewed.access_count, 0);
    assert_eq!(viewed.updated_at, stored.updated_at);

    let recalled = manager
        .recall("p:auth-policy", "march")
        .expect("recall should succeed");
    assert_eq!(recalled.access_count, 1);
    assert_eq!(
        recalled.updated_at, stored.updated_at,
        "recall should not rewrite freshness timestamps"
    );
}

#[test]
fn cold_start_search_returns_all_memories_up_to_fifty() {
    let (mut manager, _guard, _project_root) = with_test_manager("cold-start-all");

    for index in 0..20 {
        manager
            .memorize(
                MemorizeRequest {
                    id: format!("memory-{index}"),
                    memory_type: "fact".to_string(),
                    topic: "auth".to_string(),
                    title: format!("Memory {index}"),
                    content: format!("auth detail {index}"),
                    tags: vec!["auth".to_string()],
                    scope: None,
                    level: Some("project".to_string()),
                },
                "march",
            )
            .expect("failed to create project memory");
    }

    let view = manager
        .search(
            &MemoryQuery {
                task_name: None,
                latest_user_message: Some("auth".to_string()),
                open_file_paths: Vec::new(),
                recent_assistant_messages: Vec::new(),
                active_agent: "march".to_string(),
                context_pressure_percent: None,
            },
            12,
        )
        .expect("search should succeed");

    assert_eq!(view.entries.len(), 20);
}

#[test]
fn task_name_participates_in_memory_matching() {
    let (mut manager, _guard, _project_root) = with_test_manager("task-name-signal");

    manager
        .memorize(
            MemorizeRequest {
                id: "task-target".to_string(),
                memory_type: "pattern".to_string(),
                topic: "build".to_string(),
                title: "Need special preflight".to_string(),
                content: "When touching nebula build, run the preflight checklist first."
                    .to_string(),
                tags: vec!["nebula-preflight".to_string()],
                scope: None,
                level: Some("project".to_string()),
            },
            "march",
        )
        .expect("failed to create target memory");

    for index in 0..50 {
        manager
            .memorize(
                MemorizeRequest {
                    id: format!("filler-{index}"),
                    memory_type: "fact".to_string(),
                    topic: "misc".to_string(),
                    title: format!("Filler {index}"),
                    content: format!("Unrelated filler content {index}"),
                    tags: vec![format!("filler-{index}")],
                    scope: None,
                    level: Some("project".to_string()),
                },
                "march",
            )
            .expect("failed to create filler memory");
    }

    let view = manager
        .search(
            &MemoryQuery {
                task_name: Some("nebula preflight".to_string()),
                latest_user_message: None,
                open_file_paths: Vec::new(),
                recent_assistant_messages: Vec::new(),
                active_agent: "march".to_string(),
                context_pressure_percent: None,
            },
            5,
        )
        .expect("search should succeed");

    assert!(
        view.entries.iter().any(|entry| entry.id == "p:task-target"),
        "task title signal should make the target memory retrievable"
    );
}

#[test]
fn usage_counters_stay_in_memory_until_flush() {
    let (mut manager, _guard, project_root) = with_test_manager("usage-flush");

    manager
        .memorize(
            MemorizeRequest {
                id: "auth-policy".to_string(),
                memory_type: "fact".to_string(),
                topic: "auth".to_string(),
                title: "JWT refresh policy".to_string(),
                content: "Refresh token lives in httpOnly cookie.".to_string(),
                tags: vec!["auth".to_string()],
                scope: None,
                level: Some("project".to_string()),
            },
            "march",
        )
        .expect("failed to store project memory");

    manager
        .search(
            &MemoryQuery {
                task_name: None,
                latest_user_message: Some("auth".to_string()),
                open_file_paths: Vec::new(),
                recent_assistant_messages: Vec::new(),
                active_agent: "march".to_string(),
                context_pressure_percent: None,
            },
            12,
        )
        .expect("search should succeed");
    manager
        .recall("p:auth-policy", "march")
        .expect("recall should succeed");

    let memory_path = project_root
        .join(".march")
        .join("memories")
        .join("auth-policy.md");
    let before_flush = fs::read_to_string(&memory_path).expect("read memory before flush");
    assert!(before_flush.contains("access_count: 0"));
    assert!(before_flush.contains("skip_count: 0"));

    manager
        .flush_pending_usage_updates()
        .expect("flush should succeed");

    let after_flush = fs::read_to_string(&memory_path).expect("read memory after flush");
    assert!(after_flush.contains("access_count: 1"));
    assert!(after_flush.contains("skip_count: 0"));
}
