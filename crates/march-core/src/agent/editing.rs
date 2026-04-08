use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub struct FileEditResult {
    pub before: String,
    pub after: String,
}

pub fn edit_lines<F>(path: &Path, mutate: F) -> Result<FileEditResult>
where
    F: FnOnce(&mut Vec<String>) -> Result<()>,
{
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let trailing_newline = content.ends_with('\n');
    let mut lines = if content.is_empty() {
        Vec::new()
    } else {
        content
            .split('\n')
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    mutate(&mut lines)?;
    let mut output = lines.join("\n");
    if trailing_newline {
        output.push('\n');
    }
    fs::write(path, &output).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(FileEditResult {
        before: content,
        after: output,
    })
}

pub fn replace_line_range(
    lines: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
    new_content: &str,
) -> Result<()> {
    validate_line_range(lines, start_line, end_line)?;
    let replacement = new_content
        .trim_end_matches('\n')
        .split('\n')
        .filter(|part| !part.is_empty() || !new_content.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    lines.splice((start_line - 1)..end_line, replacement);
    Ok(())
}

pub fn insert_line_block(
    lines: &mut Vec<String>,
    after_line: usize,
    new_content: &str,
) -> Result<()> {
    if after_line > lines.len() {
        bail!(
            "insert_lines after_line {} is out of range for {} lines",
            after_line,
            lines.len()
        );
    }
    let insertion = if new_content.is_empty() {
        Vec::new()
    } else {
        new_content
            .trim_end_matches('\n')
            .split('\n')
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    lines.splice(after_line..after_line, insertion);
    Ok(())
}

pub fn delete_line_range(
    lines: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
) -> Result<()> {
    validate_line_range(lines, start_line, end_line)?;
    lines.drain((start_line - 1)..end_line);
    Ok(())
}

fn validate_line_range(lines: &[String], start_line: usize, end_line: usize) -> Result<()> {
    if start_line == 0 || end_line == 0 || start_line > end_line || end_line > lines.len() {
        bail!(
            "invalid line range {}-{} for {} lines",
            start_line,
            end_line,
            lines.len()
        );
    }
    Ok(())
}
