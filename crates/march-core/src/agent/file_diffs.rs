use std::path::Path;

use similar::TextDiff;

const MAX_DIFF_LINES: usize = 200;

pub(super) struct FileDiff {
    pub rendered: String,
}

pub(super) fn format_file_diff(path: &Path, before: &str, after: &str) -> FileDiff {
    let unified = TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(
            &format!("a/{}", path.display()),
            &format!("b/{}", path.display()),
        )
        .to_string();

    let total_lines = unified.lines().count();
    if total_lines <= MAX_DIFF_LINES {
        return FileDiff { rendered: unified };
    }

    let mut truncated = unified
        .lines()
        .take(MAX_DIFF_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    truncated.push('\n');
    truncated.push_str(&format!(
        "[diff 共 {total_lines} 行，仅显示前 {MAX_DIFF_LINES} 行]"
    ));

    FileDiff {
        rendered: truncated,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::format_file_diff;

    #[test]
    fn formats_unified_diff_with_headers() {
        let diff = format_file_diff(
            Path::new("src/main.rs"),
            "fn main() {\n    println!(\"old\");\n}\n",
            "fn main() {\n    println!(\"new\");\n}\n",
        );

        assert!(diff.rendered.contains("--- a/src/main.rs"));
        assert!(diff.rendered.contains("+++ b/src/main.rs"));
        assert!(diff.rendered.contains("-    println!(\"old\");"));
        assert!(diff.rendered.contains("+    println!(\"new\");"));
    }

    #[test]
    fn truncates_long_diff_with_summary_line() {
        let before = (0..260)
            .map(|index| format!("before-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let after = (0..260)
            .map(|index| format!("after-{index}"))
            .collect::<Vec<_>>()
            .join("\n");

        let diff = format_file_diff(Path::new("src/huge.txt"), &before, &after);

        assert!(diff.rendered.contains("[diff 共 "));
        assert!(diff.rendered.contains("仅显示前 200 行"));
        assert_eq!(diff.rendered.lines().count(), 201);
    }
}
