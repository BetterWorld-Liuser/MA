use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{MarchConfig, SkillTriggerRuleConfig};
use crate::context::Injection;

const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
    pub auto_triggered: bool,
    pub trigger_reason: Option<String>,
}

pub struct SkillLoader {
    work_dir: PathBuf,
    home_dir: PathBuf,
}

impl SkillLoader {
    pub fn new(work_dir: impl Into<PathBuf>, home_dir: impl Into<PathBuf>) -> Self {
        Self {
            work_dir: work_dir.into(),
            home_dir: home_dir.into(),
        }
    }

    pub fn load(&self, config: &MarchConfig) -> Result<Vec<SkillEntry>> {
        let disabled = config
            .skills
            .disable
            .iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect::<HashSet<_>>();

        let mut merged = BTreeMap::<String, SkillEntry>::new();
        for base_dir in self.scan_roots() {
            for discovered in self.scan_root(&base_dir)? {
                if disabled.contains(&discovered.key) {
                    continue;
                }
                merged.insert(
                    discovered.key,
                    SkillEntry {
                        name: discovered.display_name,
                        path: discovered.path,
                        description: discovered.description,
                        auto_triggered: false,
                        trigger_reason: None,
                    },
                );
            }
        }

        let trigger_matches = collect_auto_trigger_matches(&self.work_dir, &config.skills);
        let mut entries = merged.into_values().collect::<Vec<_>>();
        for entry in &mut entries {
            if let Some(reasons) = trigger_matches.get(&entry.name.to_ascii_lowercase()) {
                entry.auto_triggered = true;
                entry.trigger_reason = Some(reasons.join(", "));
            }
        }

        Ok(entries)
    }

    pub fn to_injection(&self, entries: &[SkillEntry]) -> Injection {
        let mut content = String::from("可用 Skills：\n");

        if entries.is_empty() {
            content.push_str("- (none)\n");
        } else {
            for entry in entries {
                let display_path = normalize_display_path(&entry.path);
                let trigger_suffix = entry
                    .trigger_reason
                    .as_deref()
                    .map(|reason| format!(" [auto: {reason}]"))
                    .unwrap_or_default();
                if entry.description.is_empty() {
                    content.push_str(&format!(
                        "- {}  ({display_path}){trigger_suffix}\n",
                        entry.name
                    ));
                } else {
                    content.push_str(&format!(
                        "- {}  ({display_path})：{}{trigger_suffix}\n",
                        entry.name, entry.description
                    ));
                }
            }
        }

        content.push_str("\n需要某个 skill 的详细内容时，用 open_file 打开对应路径。");

        Injection {
            id: "skills".to_string(),
            content,
        }
    }

    fn scan_roots(&self) -> [PathBuf; 4] {
        [
            self.home_dir.join(".agent").join("skills"),
            self.home_dir.join(".agents").join("skills"),
            self.home_dir.join(".march").join("skills"),
            self.work_dir.join(".march").join("skills"),
        ]
    }

    fn scan_root(&self, root: &Path) -> Result<Vec<DiscoveredSkill>> {
        if !root.exists() {
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();
        let mut directories = fs::read_dir(root)
            .with_context(|| format!("failed to scan {}", root.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read entries under {}", root.display()))?;
        directories.sort_by_key(|entry| entry.file_name());

        for directory in directories {
            let file_type = directory
                .file_type()
                .with_context(|| format!("failed to inspect {}", directory.path().display()))?;
            if !file_type.is_dir() {
                continue;
            }

            let folder_name = directory.file_name().to_string_lossy().trim().to_string();
            if folder_name.is_empty() {
                continue;
            }

            let skill_path = directory.path().join(SKILL_FILE_NAME);
            if !skill_path.is_file() {
                continue;
            }

            let metadata = parse_skill_metadata(&skill_path)?;
            skills.push(DiscoveredSkill {
                key: folder_name.to_ascii_lowercase(),
                display_name: metadata.name.unwrap_or(folder_name),
                description: metadata.description.unwrap_or_default(),
                path: skill_path,
            });
        }

        Ok(skills)
    }
}

#[derive(Debug)]
struct DiscoveredSkill {
    key: String,
    display_name: String,
    description: String,
    path: PathBuf,
}

#[derive(Debug, Default)]
struct SkillMetadata {
    name: Option<String>,
    description: Option<String>,
}

fn parse_skill_metadata(path: &Path) -> Result<SkillMetadata> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse_frontmatter(&content))
}

fn parse_frontmatter(content: &str) -> SkillMetadata {
    let normalized = content.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return SkillMetadata::default();
    };
    let Some((frontmatter, _body)) = rest.split_once("\n---\n") else {
        return SkillMetadata::default();
    };

    let mut metadata = SkillMetadata::default();
    for line in frontmatter.lines() {
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            continue;
        };
        let key = raw_key.trim();
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        match key {
            "name" => metadata.name = Some(value.to_string()),
            "description" => metadata.description = Some(value.to_string()),
            _ => {}
        }
    }

    metadata
}

fn normalize_display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn collect_auto_trigger_matches(
    work_dir: &Path,
    config: &crate::config::SkillsConfig,
) -> BTreeMap<String, Vec<String>> {
    let mut matches = BTreeMap::<String, BTreeSet<String>>::new();

    if config.use_builtin_triggers {
        for rule in BUILTIN_AUTO_TRIGGER_RULES {
            apply_trigger_rule(work_dir, &rule.as_config(), &mut matches);
        }
    }

    for rule in &config.trigger_rules {
        apply_trigger_rule(work_dir, rule, &mut matches);
    }

    matches
        .into_iter()
        .map(|(skill, reasons)| (skill, reasons.into_iter().collect()))
        .collect()
}

fn apply_trigger_rule(
    work_dir: &Path,
    rule: &SkillTriggerRuleConfig,
    matches: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let matched_paths = rule
        .paths
        .iter()
        .filter(|path| work_dir.join(path.as_str()).is_file())
        .cloned()
        .collect::<Vec<_>>();

    if matched_paths.is_empty() {
        return;
    }

    for skill in &rule.skills {
        let reasons = matches.entry(skill.clone()).or_default();
        for path in &matched_paths {
            reasons.insert(format!("detected {path}"));
        }
    }
}

const BUILTIN_AUTO_TRIGGER_RULES: &[AutoTriggerRule] = &[
    AutoTriggerRule {
        paths: &["Cargo.toml"],
        skills: &["rust"],
    },
    AutoTriggerRule {
        paths: &["package.json"],
        skills: &["node", "javascript", "typescript"],
    },
    AutoTriggerRule {
        paths: &["tsconfig.json"],
        skills: &["typescript"],
    },
    AutoTriggerRule {
        paths: &["pyproject.toml", "requirements.txt"],
        skills: &["python"],
    },
    AutoTriggerRule {
        paths: &["go.mod"],
        skills: &["go"],
    },
    AutoTriggerRule {
        paths: &["Gemfile"],
        skills: &["ruby"],
    },
    AutoTriggerRule {
        paths: &["Dockerfile", "docker-compose.yml", "compose.yaml"],
        skills: &["docker"],
    },
];

struct AutoTriggerRule {
    paths: &'static [&'static str],
    skills: &'static [&'static str],
}

impl AutoTriggerRule {
    fn as_config(&self) -> SkillTriggerRuleConfig {
        SkillTriggerRuleConfig {
            paths: self.paths.iter().map(|path| (*path).to_string()).collect(),
            skills: self
                .skills
                .iter()
                .map(|skill| (*skill).to_ascii_lowercase())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::config::{MarchConfig, SkillsConfig};

    #[test]
    fn higher_priority_skill_overrides_lower_priority_with_same_folder_name() {
        let fixture = SkillFixture::new("override");
        fixture.write_skill(
            &fixture.home_dir.join(".agent").join("skills").join("rust"),
            "Shared Rust",
            "共享 Rust skill",
        );
        fixture.write_skill(
            &fixture.work_dir.join(".march").join("skills").join("rust"),
            "Project Rust",
            "项目 Rust skill",
        );

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let skills = loader
            .load(&MarchConfig::default())
            .expect("load skills should succeed");

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Project Rust");
        assert!(!skills[0].auto_triggered);
        assert!(
            skills[0]
                .path
                .starts_with(fixture.work_dir.join(".march").join("skills"))
        );
    }

    #[test]
    fn agents_directory_is_scanned_as_shared_skill_root() {
        let fixture = SkillFixture::new("agents-root");
        fixture.write_skill(
            &fixture
                .home_dir
                .join(".agents")
                .join("skills")
                .join("find-skills"),
            "find-skills",
            "Scan local skill inventory",
        );

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let skills = loader
            .load(&MarchConfig::default())
            .expect("load skills should succeed");

        assert!(skills.iter().any(|skill| {
            skill.name == "find-skills"
                && skill
                    .path
                    .starts_with(fixture.home_dir.join(".agents").join("skills"))
        }));
    }

    #[test]
    fn disabled_skill_is_filtered_by_folder_name() {
        let fixture = SkillFixture::new("disable");
        fixture.write_skill(
            &fixture.home_dir.join(".march").join("skills").join("git"),
            "Git skill",
            "Git conventions",
        );

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let skills = loader
            .load(&MarchConfig {
                skills: SkillsConfig {
                    disable: vec!["git".to_string()],
                    use_builtin_triggers: true,
                    trigger_rules: Vec::new(),
                },
            })
            .expect("load skills should succeed");

        assert!(skills.is_empty());
    }

    #[test]
    fn injection_uses_absolute_paths() {
        let fixture = SkillFixture::new("injection");
        let shared_path = fixture.home_dir.join(".agent").join("skills").join("rust");
        let project_path = fixture
            .work_dir
            .join(".march")
            .join("skills")
            .join("deploy");
        fixture.write_skill(&shared_path, "Rust", "共享 skill");
        fixture.write_skill(&project_path, "Deploy", "项目 skill");

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let entries = loader
            .load(&MarchConfig::default())
            .expect("load skills should succeed");
        let injection = loader.to_injection(&entries);

        assert!(
            injection
                .content
                .contains(&normalize_display_path(&shared_path.join(SKILL_FILE_NAME)))
        );
        assert!(
            injection
                .content
                .contains(&normalize_display_path(&project_path.join(SKILL_FILE_NAME)))
        );
    }

    #[test]
    fn cargo_toml_auto_triggers_rust_skill() {
        let fixture = SkillFixture::new("auto-trigger");
        fixture.write_skill(
            &fixture.home_dir.join(".agent").join("skills").join("rust"),
            "rust",
            "Rust 项目工作流",
        );
        fs::write(
            fixture.work_dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .expect("write cargo");

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let entries = loader
            .load(&MarchConfig::default())
            .expect("load skills should succeed");
        let rust = entries
            .into_iter()
            .find(|entry| entry.name == "rust")
            .expect("rust skill should exist");

        assert!(rust.auto_triggered);
        assert_eq!(rust.trigger_reason.as_deref(), Some("detected Cargo.toml"));
    }

    #[test]
    fn custom_trigger_rule_supports_many_to_many_mappings() {
        let fixture = SkillFixture::new("many-to-many");
        fixture.write_skill(
            &fixture.home_dir.join(".agent").join("skills").join("node"),
            "node",
            "Node 工作流",
        );
        fixture.write_skill(
            &fixture
                .home_dir
                .join(".agent")
                .join("skills")
                .join("typescript"),
            "typescript",
            "TypeScript 工作流",
        );
        fixture.write_skill(
            &fixture
                .home_dir
                .join(".agent")
                .join("skills")
                .join("frontend"),
            "frontend",
            "前端工作流",
        );
        fs::write(fixture.work_dir.join("package.json"), "{}").expect("write package");
        fs::write(fixture.work_dir.join("tsconfig.json"), "{}").expect("write tsconfig");

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let entries = loader
            .load(&MarchConfig {
                skills: crate::config::SkillsConfig {
                    use_builtin_triggers: false,
                    trigger_rules: vec![SkillTriggerRuleConfig {
                        paths: vec!["package.json".to_string(), "tsconfig.json".to_string()],
                        skills: vec![
                            "node".to_string(),
                            "typescript".to_string(),
                            "frontend".to_string(),
                        ],
                    }],
                    ..Default::default()
                },
            })
            .expect("load skills should succeed");

        let triggered = entries
            .into_iter()
            .filter(|entry| entry.auto_triggered)
            .map(|entry| (entry.name, entry.trigger_reason.unwrap_or_default()))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(triggered.len(), 3);
        assert_eq!(
            triggered.get("node").map(String::as_str),
            Some("detected package.json, detected tsconfig.json")
        );
        assert_eq!(
            triggered.get("typescript").map(String::as_str),
            Some("detected package.json, detected tsconfig.json")
        );
        assert_eq!(
            triggered.get("frontend").map(String::as_str),
            Some("detected package.json, detected tsconfig.json")
        );
    }

    #[test]
    fn multiple_rules_can_activate_same_skill() {
        let fixture = SkillFixture::new("many-files-one-skill");
        fixture.write_skill(
            &fixture
                .home_dir
                .join(".agent")
                .join("skills")
                .join("python"),
            "python",
            "Python 工作流",
        );
        fs::write(
            fixture.work_dir.join("pyproject.toml"),
            "[project]\nname='demo'\n",
        )
        .expect("write pyproject");
        fs::write(fixture.work_dir.join("requirements.txt"), "pytest\n").expect("write reqs");

        let loader = SkillLoader::new(&fixture.work_dir, &fixture.home_dir);
        let entries = loader
            .load(&MarchConfig::default())
            .expect("load skills should succeed");
        let python = entries
            .into_iter()
            .find(|entry| entry.name == "python")
            .expect("python skill should exist");

        assert!(python.auto_triggered);
        assert_eq!(
            python.trigger_reason.as_deref(),
            Some("detected pyproject.toml, detected requirements.txt")
        );
    }

    struct SkillFixture {
        home_dir: PathBuf,
        work_dir: PathBuf,
    }

    impl SkillFixture {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("ma-skills-{prefix}-{unique}"));
            let home_dir = root.join("home");
            let work_dir = root.join("workspace");
            fs::create_dir_all(&home_dir).expect("create home");
            fs::create_dir_all(&work_dir).expect("create workspace");
            Self { home_dir, work_dir }
        }

        fn write_skill(&self, dir: &Path, name: &str, description: &str) {
            fs::create_dir_all(dir).expect("create skill dir");
            fs::write(
                dir.join(SKILL_FILE_NAME),
                format!("---\nname: {name}\ndescription: {description}\n---\n\n{description}\n"),
            )
            .expect("write skill");
        }
    }
}
