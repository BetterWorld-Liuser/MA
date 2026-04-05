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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

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
}
