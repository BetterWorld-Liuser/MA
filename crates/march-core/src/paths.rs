use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// 统一清理 Windows verbatim path（如 `\\?\C:\...`），避免它泄漏到 UI、prompt 和持久化层。
/// 内部仍然可以继续使用 canonical path 的“真实定位”能力，但对上层展示与序列化一律输出常规路径。
pub fn clean_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            return Path::new(stripped).to_path_buf();
        }
    }

    path
}

pub fn canonicalize_clean(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path)
        .map(clean_path)
        .with_context(|| format!("failed to canonicalize {}", path.display()))
}

/// Resolve the project root March should use for project-scoped metadata.
///
/// Tasks may run from a nested working directory such as `src/`, but project-level
/// `.march/` content lives at the workspace root. Walk upward so task-local cwd
/// changes do not hide shared agents, skills, config, or the database.
pub fn resolve_project_root(path: &Path) -> PathBuf {
    let cleaned = clean_path(path.to_path_buf());

    for ancestor in cleaned.ancestors() {
        if ancestor.join(".march").is_dir() {
            return clean_path(ancestor.to_path_buf());
        }
    }

    cleaned
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::resolve_project_root;

    use super::clean_path;

    #[test]
    fn clean_path_strips_windows_verbatim_prefix() {
        let raw = PathBuf::from(r"\\?\D:\playground\MA\AGENTS.md");
        let cleaned = clean_path(raw);

        #[cfg(windows)]
        assert_eq!(cleaned, PathBuf::from(r"D:\playground\MA\AGENTS.md"));

        #[cfg(not(windows))]
        assert_eq!(cleaned, PathBuf::from(r"\\?\D:\playground\MA\AGENTS.md"));
    }

    #[test]
    fn resolve_project_root_finds_nearest_march_ancestor() {
        let fixture = TestFixture::new("project-root-march");
        let nested = fixture.root.join("workspace").join("src").join("nested");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::create_dir_all(fixture.root.join("workspace").join(".march"))
            .expect("create workspace .march");

        assert_eq!(
            resolve_project_root(&nested),
            fixture.root.join("workspace"),
        );
    }

    #[test]
    fn resolve_project_root_falls_back_to_path_when_no_march() {
        let fixture = TestFixture::new("project-root-no-march");
        let nested = fixture.root.join("external").join("pkg").join("src");
        std::fs::create_dir_all(&nested).expect("create nested dir");

        assert_eq!(
            resolve_project_root(&nested),
            nested,
        );
    }

    struct TestFixture {
        root: PathBuf,
    }

    impl TestFixture {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("march-paths-{prefix}-{unique}"));
            std::fs::create_dir_all(&root).expect("create fixture root");
            Self { root }
        }
    }
}
