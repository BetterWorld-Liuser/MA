use super::*;

impl AgentSession {
    pub fn active_agent_name(&self) -> &str {
        &self.active_agent
    }

    pub fn set_active_agent(&mut self, name: impl Into<String>) {
        let name = name.into();
        if self.agent_profiles.contains_key(&name) {
            self.active_agent = name;
        }
    }

    pub fn has_agent(&self, name: &str) -> bool {
        self.agent_profiles.contains_key(name)
    }

    pub fn agent_profiles(&self) -> impl Iterator<Item = &AgentProfile> {
        self.agent_profiles.values()
    }

    pub fn active_agent_profile(&self) -> Option<&AgentProfile> {
        self.agent_profiles.get(self.active_agent_name())
    }

    pub fn display_name_for_agent(&self, name: &str) -> String {
        self.agent_profiles
            .get(name)
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| {
                if name.eq_ignore_ascii_case(MARCH_AGENT_NAME) {
                    "March".to_string()
                } else {
                    name.to_string()
                }
            })
    }

    pub fn refresh_agent_profiles(&mut self) -> Result<()> {
        let active_agent = self.active_agent.clone();
        self.agent_profiles = load_agent_profiles(&self.working_directory)?
            .into_iter()
            .map(|profile| (profile.name.clone(), profile))
            .collect::<IndexMap<_, _>>();
        if !self.agent_profiles.contains_key(&active_agent) {
            self.active_agent = MARCH_AGENT_NAME.to_string();
        }
        Ok(())
    }

    pub fn open_file_in_scope(
        &mut self,
        scope: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<()> {
        let scope = scope.into();
        let path = self.resolve_path(path.into());
        self.watcher.watch_file(path.clone())?;
        if !self
            .open_files
            .iter()
            .any(|entry| entry.scope == scope && entry.path == path)
        {
            self.open_files.push(PersistedOpenFile {
                scope,
                path,
                locked: false,
            });
        }
        Ok(())
    }

    pub fn write_note_in_scope(
        &mut self,
        scope: impl Into<String>,
        id: impl Into<String>,
        content: impl Into<String>,
    ) {
        let scope = scope.into();
        self.notes
            .entry(scope)
            .or_default()
            .insert(id.into(), NoteEntry::new(content));
    }

    pub(crate) fn private_scope(&self) -> &str {
        &self.active_agent
    }

    pub(crate) fn notes_for_active_agent(&self) -> IndexMap<String, NoteEntry> {
        let mut merged = IndexMap::new();
        if let Some(shared) = self.notes.get(SHARED_SCOPE) {
            for (id, note) in shared {
                merged.insert(id.clone(), note.clone());
            }
        }
        if let Some(private) = self.notes.get(self.private_scope()) {
            for (id, note) in private {
                merged.insert(id.clone(), note.clone());
            }
        }
        merged
    }

    pub(crate) fn persisted_notes(&self) -> Vec<PersistedNote> {
        let mut persisted = Vec::new();
        for (scope, notes) in &self.notes {
            for (id, entry) in notes {
                persisted.push(PersistedNote {
                    scope: scope.clone(),
                    id: id.clone(),
                    entry: entry.clone(),
                });
            }
        }
        persisted.sort_by(|left, right| {
            (left.scope == SHARED_SCOPE)
                .cmp(&(right.scope == SHARED_SCOPE))
                .reverse()
                .then_with(|| left.scope.cmp(&right.scope))
                .then_with(|| left.id.cmp(&right.id))
        });
        persisted
    }

    pub(crate) fn open_file_snapshots_for_active_agent(&self) -> IndexMap<PathBuf, FileSnapshot> {
        let all = self.open_file_snapshots();
        let mut filtered = IndexMap::new();
        for scope in [SHARED_SCOPE, self.private_scope()] {
            for entry in self.open_files.iter().filter(|entry| entry.scope == scope) {
                if filtered.contains_key(&entry.path) {
                    continue;
                }
                if let Some(snapshot) = all.get(&entry.path) {
                    filtered.insert(entry.path.clone(), snapshot.clone());
                }
            }
        }
        filtered
    }

    pub(crate) fn locked_files_for_active_agent(&self) -> Vec<PathBuf> {
        let mut locked = Vec::new();
        for scope in [SHARED_SCOPE, self.private_scope()] {
            for entry in self
                .open_files
                .iter()
                .filter(|entry| entry.scope == scope && entry.locked)
            {
                if !locked.iter().any(|path| path == &entry.path) {
                    locked.push(entry.path.clone());
                }
            }
        }
        clean_unique_paths(&locked)
    }

    /// Assembles the system core for the current active agent following the
    /// design in agents-teams.md:
    ///   [base instructions]  — shared foundation (tool rules, completion, handoff)
    ///   [agents roster]      — who's available + active_agent marker
    ///   [agent system_prompt] — the active agent's persona/behavior
    pub(crate) fn system_core_for_active_agent(&self) -> String {
        let Some(profile) = self.agent_profiles.get(self.private_scope()) else {
            return self.config.system_core.clone();
        };

        let mut output = String::new();

        output.push_str(base_instructions());
        output.push_str("\n\n# Available Agents\n");
        output.push_str(&self.available_agents_for_prompt());

        let role_prompt = profile.system_prompt.trim();
        if !role_prompt.is_empty() {
            output.push_str("\n\n# Agent Role\n");
            output.push_str(role_prompt);
        }

        output
    }

    fn available_agents_for_prompt(&self) -> String {
        let active = self.active_agent_name();
        self.agent_profiles
            .values()
            .map(|profile| {
                if profile.name == active {
                    format!(
                        "- {} | {} | {} (you)",
                        profile.name, profile.display_name, profile.description
                    )
                } else {
                    format!(
                        "- {} | {} | {}",
                        profile.name, profile.display_name, profile.description
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) fn notes_by_scope(notes: Vec<PersistedNote>) -> IndexMap<String, IndexMap<String, NoteEntry>> {
    let mut by_scope = IndexMap::new();
    for note in notes {
        by_scope
            .entry(note.scope)
            .or_insert_with(IndexMap::new)
            .insert(note.id, note.entry);
    }
    by_scope
}
