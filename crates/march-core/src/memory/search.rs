use super::*;

pub(super) fn query_tokens(query: &MemoryQuery) -> Vec<String> {
    let mut combined = String::new();
    if let Some(task_name) = &query.task_name {
        combined.push_str(task_name);
        combined.push(' ');
    }
    if let Some(message) = &query.latest_user_message {
        combined.push_str(message);
        combined.push(' ');
    }
    for path in &query.open_file_paths {
        combined.push_str(&path.to_string_lossy());
        combined.push(' ');
    }
    for message in &query.recent_assistant_messages {
        combined.push_str(message);
        combined.push(' ');
    }

    tokenize_terms(&combined)
}

pub(super) fn tokenize_text(raw: &str) -> String {
    tokenize_terms(raw).join(" ")
}

pub(super) fn tokenize_terms(raw: &str) -> Vec<String> {
    let prepared = raw
        .replace(
            ['/', '\\', '.', '_', '-', ':', ',', '(', ')', '[', ']'],
            " ",
        )
        .replace('\n', " ");
    let mut tokens = Vec::new();
    let mut seen = HashSet::new();

    for piece in JIEBA.cut(&prepared, false) {
        for token in piece.split_whitespace() {
            let lowered = token.trim().to_ascii_lowercase();
            if lowered.is_empty() {
                continue;
            }
            if lowered.chars().all(|ch| ch.is_ascii_punctuation()) {
                continue;
            }
            if seen.insert(lowered.clone()) {
                tokens.push(lowered);
            }
        }
    }

    tokens
}

pub(super) fn collect_open_path_segments(paths: &[PathBuf]) -> HashSet<String> {
    let mut segments = HashSet::new();
    for path in paths {
        for token in tokenize_terms(&path.to_string_lossy()) {
            segments.insert(token);
        }
    }
    segments
}

pub(super) fn calculate_path_match_score(
    memory: &MemoryRecord,
    path_segments: &HashSet<String>,
) -> f32 {
    if path_segments.is_empty() {
        return 0.0;
    }
    let tags = memory.normalized_tags();
    let hits = tags.intersection(path_segments).count();
    if hits == 0 {
        0.0
    } else {
        hits as f32 / path_segments.len().max(1) as f32
    }
}

pub(super) fn calculate_recency_score(updated_at: SystemTime) -> f32 {
    let age = SystemTime::now()
        .duration_since(updated_at)
        .unwrap_or(Duration::from_secs(0));
    let days = age.as_secs_f32() / 86_400.0;
    1.0 / (1.0 + days)
}

pub(super) fn calculate_frequency_score(access_count: u32, max_access_count: u32) -> f32 {
    if max_access_count == 0 {
        return 0.0;
    }
    let current = (1.0 + access_count as f32).ln();
    let max = (1.0 + max_access_count as f32).ln();
    (current / max).clamp(0.0, 1.0)
}

pub(super) fn topic_warnings_for_entries<'a>(
    ids: impl Iterator<Item = &'a str>,
    project_memories: &IndexMap<String, MemoryRecord>,
    global_memories: &IndexMap<String, MemoryRecord>,
) -> Vec<String> {
    let mut topic_counts = BTreeMap::<String, usize>::new();
    for id in ids {
        let memory = if let Some(raw_id) = id.strip_prefix("p:") {
            project_memories.get(raw_id)
        } else if let Some(raw_id) = id.strip_prefix("g:") {
            global_memories.get(raw_id)
        } else {
            None
        };
        let Some(memory) = memory else {
            continue;
        };
        *topic_counts.entry(memory.topic.clone()).or_default() += 1;
    }

    topic_counts
        .into_iter()
        .filter(|(_, count)| *count > 5)
        .map(|(topic, count)| {
            format!(
                "Topic \"{}\" currently has {} matched memories; consider merging them into a tighter summary.",
                topic, count
            )
        })
        .collect()
}
