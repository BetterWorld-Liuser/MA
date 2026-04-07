use super::*;

impl AgentSession {
    pub fn ui_system_status(&self, context_budget_tokens: usize) -> UiSystemStatusView {
        UiSystemStatusView {
            locked_files: self.locked_files_for_active_agent(),
            context_pressure: self.estimate_context_pressure(context_budget_tokens).map(
                |pressure| UiContextPressureView {
                    used_percent: pressure.used_percent,
                    message: pressure.message,
                },
            ),
        }
    }

    pub fn ui_context_usage(&self, context_budget_tokens: usize) -> UiContextUsageView {
        let sections = vec![
            UiContextUsageSectionView::new(
                "system",
                estimate_token_count(&self.system_core_for_active_agent()),
            ),
            UiContextUsageSectionView::new(
                "injections",
                self.injections
                    .iter()
                    .map(|injection| estimate_token_count(&injection.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "notes",
                self.notes_for_active_agent()
                    .values()
                    .map(|note| estimate_token_count(&note.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "chat",
                self.history
                    .turns
                    .iter()
                    .map(|turn| estimate_content_blocks_token_count(&turn.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "files",
                self.open_file_snapshots_for_active_agent()
                    .values()
                    .map(|snapshot| match snapshot {
                        FileSnapshot::Available { content, .. } => estimate_token_count(content),
                        FileSnapshot::Deleted { .. } | FileSnapshot::Moved { .. } => 8,
                    })
                    .sum(),
            ),
        ];

        let used_tokens = sections.iter().map(|section| section.tokens).sum();
        UiContextUsageView::new(used_tokens, context_budget_tokens, sections)
    }

    pub fn ui_runtime_snapshot(&self, context_budget_tokens: usize) -> UiRuntimeSnapshot {
        let open_file_snapshots = self.open_file_snapshots_for_active_agent();
        let available_shells = self
            .available_shells
            .iter()
            .map(|shell| UiShellView {
                kind: shell.kind.label().to_string(),
                program: shell.program.clone(),
            })
            .collect::<Vec<_>>();

        let open_files = open_file_snapshots
            .values()
            .cloned()
            .map(UiFileSnapshotView::from)
            .collect::<Vec<_>>();

        let skills = self
            .skills
            .iter()
            .map(|skill| UiSkillView {
                name: skill.name.clone(),
                path: clean_path(skill.path.clone()),
                description: skill.description.clone(),
                opened: open_file_snapshots.contains_key(&skill.path),
            })
            .collect::<Vec<_>>();

        UiRuntimeSnapshot::new(
            clean_path(self.working_directory.clone()),
            available_shells,
            open_files,
            skills,
            self.ui_system_status(context_budget_tokens),
            self.ui_context_usage(context_budget_tokens),
        )
    }

    pub(crate) fn estimate_context_pressure(
        &self,
        context_budget_tokens: usize,
    ) -> Option<ContextPressure> {
        let budget = context_budget_tokens.max(1);
        let size = estimate_token_count(&self.system_core_for_active_agent())
            + self
                .injections
                .iter()
                .map(|injection| estimate_token_count(&injection.content))
                .sum::<usize>()
            + self
                .notes_for_active_agent()
                .values()
                .map(|note| estimate_token_count(&note.content))
                .sum::<usize>()
            + self
                .history
                .turns
                .iter()
                .map(|turn| estimate_content_blocks_token_count(&turn.content))
                .sum::<usize>()
            + self
                .open_file_snapshots_for_active_agent()
                .values()
                .map(|snapshot| match snapshot {
                    FileSnapshot::Available { content, .. } => estimate_token_count(content),
                    FileSnapshot::Deleted { .. } | FileSnapshot::Moved { .. } => 8,
                })
                .sum::<usize>();
        let used_percent = ((size as f32 / budget as f32) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8;
        (used_percent >= 75).then_some(ContextPressure {
            used_percent,
            message:
                "Estimated token usage is getting dense; consider closing files or removing stale notes."
                    .to_string(),
        })
    }
}
