use std::path::{Component, Path};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::{UiWorkspaceEntryKind, UiWorkspaceEntryView};

pub(super) fn search_workspace_entries(
    working_directory: &Path,
    query: &str,
    kind: Option<UiWorkspaceEntryKind>,
    limit: usize,
) -> Result<Vec<UiWorkspaceEntryView>> {
    let query = query.trim().to_lowercase();
    let mut files = visible_files_for_directory(working_directory)?;
    files.sort();
    files.dedup();

    let mut directories = files
        .iter()
        .flat_map(|path| collect_parent_directories(path))
        .collect::<Vec<_>>();
    directories.sort();
    directories.dedup();

    let entries = match kind {
        Some(UiWorkspaceEntryKind::File) => files
            .into_iter()
            .map(|path| UiWorkspaceEntryView {
                path,
                kind: UiWorkspaceEntryKind::File,
            })
            .collect::<Vec<_>>(),
        Some(UiWorkspaceEntryKind::Directory) => directories
            .into_iter()
            .map(|path| UiWorkspaceEntryView {
                path,
                kind: UiWorkspaceEntryKind::Directory,
            })
            .collect::<Vec<_>>(),
        None => {
            let mut combined = files
                .into_iter()
                .map(|path| UiWorkspaceEntryView {
                    path,
                    kind: UiWorkspaceEntryKind::File,
                })
                .collect::<Vec<_>>();
            combined.extend(directories.into_iter().map(|path| UiWorkspaceEntryView {
                path,
                kind: UiWorkspaceEntryKind::Directory,
            }));
            combined
        }
    };

    let mut ranked = entries
        .into_iter()
        .filter_map(|entry| rank_workspace_entry(&entry.path, &query).map(|score| (score, entry)))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.path.cmp(&right.1.path))
    });
    ranked.truncate(limit);
    Ok(ranked.into_iter().map(|(_, entry)| entry).collect())
}

fn visible_files_for_directory(working_directory: &Path) -> Result<Vec<String>> {
    if let Ok(files) = git_visible_files(working_directory) {
        return Ok(files);
    }
    fallback_visible_files(working_directory)
}

fn git_visible_files(working_directory: &Path) -> Result<Vec<String>> {
    let repo_root_output = Command::new("git")
        .arg("-C")
        .arg(working_directory)
        .args(["rev-parse", "--show-toplevel"])
        .output();
    let prefix_output = Command::new("git")
        .arg("-C")
        .arg(working_directory)
        .args(["rev-parse", "--show-prefix"])
        .output();
    let output = Command::new("git")
        .arg("-C")
        .arg(working_directory)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output();

    let repo_root_output = repo_root_output.context("failed to inspect git repo root")?;
    let prefix_output = prefix_output.context("failed to inspect git prefix")?;
    let output = output.context("failed to list git visible files")?;
    if !repo_root_output.status.success() || !prefix_output.status.success() || !output.status.success()
    {
        bail!("git metadata unavailable for {}", working_directory.display());
    }

    let repo_root = String::from_utf8_lossy(&repo_root_output.stdout)
        .trim()
        .to_string();
    let prefix = String::from_utf8_lossy(&prefix_output.stdout).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // git 在子目录中执行时仍然返回 repo-root 相对路径。
    // 这里把路径裁回当前 task 的工作目录，保证 @ 引用和 open_file 都基于 task cwd。
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            if prefix.is_empty() {
                return Some(line.to_string());
            }
            line.strip_prefix(&prefix)
                .map(|trimmed| trimmed.to_string())
                .or_else(|| {
                    let candidate = Path::new(&repo_root).join(line);
                    candidate
                        .strip_prefix(working_directory)
                        .ok()
                        .map(normalize_relative_path)
                })
        })
        .filter(|line| !line.is_empty())
        .collect())
}

fn fallback_visible_files(working_directory: &Path) -> Result<Vec<String>> {
    let mut pending = vec![working_directory.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name == ".git"
                || file_name == "node_modules"
                || file_name == "target"
                || file_name == "dist"
            {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(entry_path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if let Ok(relative) = entry_path.strip_prefix(working_directory) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    Ok(files)
}

fn collect_parent_directories(path: &str) -> Vec<String> {
    let mut current = Path::new(path).parent();
    let mut directories = Vec::new();
    while let Some(parent) = current {
        if parent.components().next().is_none() {
            break;
        }
        let normalized = normalize_relative_path(parent);
        if !normalized.is_empty() {
            directories.push(normalized);
        }
        current = parent.parent();
    }
    directories
}

fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn rank_workspace_entry(path: &str, query: &str) -> Option<(u8, usize)> {
    if query.is_empty() {
        return Some((3, path.len()));
    }

    let haystack = path.to_lowercase();
    if haystack == query {
        return Some((0, path.len()));
    }
    if haystack.starts_with(query) {
        return Some((1, path.len()));
    }
    if haystack.contains(query) {
        return Some((2, path.len()));
    }
    subsequence_score(&haystack, query).map(|score| (4, score))
}

fn subsequence_score(haystack: &str, needle: &str) -> Option<usize> {
    let mut score = 0usize;
    let mut cursor = 0usize;

    for ch in needle.chars() {
        let slice = &haystack[cursor..];
        let offset = slice.find(ch)?;
        score += offset;
        cursor += offset + ch.len_utf8();
    }

    Some(score + haystack.len())
}
