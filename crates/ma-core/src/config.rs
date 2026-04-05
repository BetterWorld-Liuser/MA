use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::settings::march_settings_dir;

#[derive(Debug, Clone, Default)]
pub struct MarchConfig {
    pub skills: SkillsConfig,
}

#[derive(Debug, Clone)]
pub struct SkillsConfig {
    pub disable: Vec<String>,
    pub use_builtin_triggers: bool,
    pub trigger_rules: Vec<SkillTriggerRuleConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillTriggerRuleConfig {
    pub paths: Vec<String>,
    pub skills: Vec<String>,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            disable: Vec::new(),
            use_builtin_triggers: true,
            trigger_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct RawMarchConfig {
    skills: Option<RawSkillsConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct RawSkillsConfig {
    disable: Option<Vec<String>>,
    use_builtin_triggers: Option<bool>,
    #[serde(default, alias = "trigger_rules")]
    triggers: Vec<RawSkillTriggerRuleConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct RawSkillTriggerRuleConfig {
    paths: Option<Vec<String>>,
    skills: Option<Vec<String>>,
}

impl MarchConfig {
    /// 配置来源遵循设计文档：用户级 `~/.march/config.toml` 先加载，
    /// 项目级 `.march/config.toml` 再做字段级覆盖。
    pub fn load_for_workspace(work_dir: &Path) -> Result<Self> {
        Self::load_from_paths(
            march_settings_dir()?.join("config.toml"),
            work_dir.join(".march").join("config.toml"),
        )
    }

    fn load_from_paths(user_path: PathBuf, project_path: PathBuf) -> Result<Self> {
        let user = read_optional_config(user_path)?;
        let project = read_optional_config(project_path)?;
        let user_skills = user.and_then(|config| config.skills);
        let project_skills = project.and_then(|config| config.skills);
        let user_disable = user_skills
            .as_ref()
            .and_then(|skills| skills.disable.clone());
        let project_disable = project_skills
            .as_ref()
            .and_then(|skills| skills.disable.clone());
        let user_builtin = user_skills
            .as_ref()
            .and_then(|skills| skills.use_builtin_triggers);
        let project_builtin = project_skills
            .as_ref()
            .and_then(|skills| skills.use_builtin_triggers);
        let mut trigger_rules = user_skills
            .as_ref()
            .map(|skills| normalize_trigger_rules(&skills.triggers))
            .unwrap_or_default();
        trigger_rules.extend(
            project_skills
                .as_ref()
                .map(|skills| normalize_trigger_rules(&skills.triggers))
                .unwrap_or_default(),
        );

        Ok(Self {
            skills: SkillsConfig {
                disable: project_disable.unwrap_or_else(|| user_disable.unwrap_or_default()),
                use_builtin_triggers: project_builtin.or(user_builtin).unwrap_or(true),
                trigger_rules,
            },
        })
    }
}

fn normalize_trigger_rules(raw_rules: &[RawSkillTriggerRuleConfig]) -> Vec<SkillTriggerRuleConfig> {
    raw_rules
        .iter()
        .filter_map(|rule| {
            let paths = rule
                .paths
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|path| path.trim().replace('\\', "/"))
                .filter(|path| !path.is_empty())
                .collect::<Vec<_>>();
            let skills = rule
                .skills
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|skill| skill.trim().to_ascii_lowercase())
                .filter(|skill| !skill.is_empty())
                .collect::<Vec<_>>();

            (!paths.is_empty() && !skills.is_empty()).then_some(SkillTriggerRuleConfig {
                paths,
                skills,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn project_config_overrides_disable_and_builtin_flag_and_appends_trigger_rules() {
        let fixture = ConfigFixture::new("merge");
        fixture.write_file(
            fixture.home_dir.join(".march").join("config.toml"),
            r#"
[skills]
disable = ["git"]
use_builtin_triggers = true

[[skills.triggers]]
paths = ["pyproject.toml"]
skills = ["python"]
"#,
        );
        fixture.write_file(
            fixture.work_dir.join(".march").join("config.toml"),
            r#"
[skills]
disable = ["docker"]
use_builtin_triggers = false

[[skills.triggers]]
paths = ["package.json", "tsconfig.json"]
skills = ["node", "typescript"]
"#,
        );

        let config = MarchConfig::load_from_paths(
            fixture.home_dir.join(".march").join("config.toml"),
            fixture.work_dir.join(".march").join("config.toml"),
        )
        .expect("load config should succeed");

        assert_eq!(config.skills.disable, vec!["docker"]);
        assert!(!config.skills.use_builtin_triggers);
        assert_eq!(config.skills.trigger_rules.len(), 2);
        assert_eq!(config.skills.trigger_rules[0].skills, vec!["python"]);
        assert_eq!(
            config.skills.trigger_rules[1].paths,
            vec!["package.json", "tsconfig.json"]
        );
    }

    struct ConfigFixture {
        home_dir: PathBuf,
        work_dir: PathBuf,
    }

    impl ConfigFixture {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos();
            let root = std::env::temp_dir().join(format!("ma-config-{prefix}-{unique}"));
            let home_dir = root.join("home");
            let work_dir = root.join("workspace");
            fs::create_dir_all(home_dir.join(".march")).expect("create home config dir");
            fs::create_dir_all(work_dir.join(".march")).expect("create project config dir");
            Self { home_dir, work_dir }
        }

        fn write_file(&self, path: PathBuf, content: &str) {
            fs::write(path, content.trim_start()).expect("write config file");
        }
    }
}

fn read_optional_config(path: PathBuf) -> Result<Option<RawMarchConfig>> {
    if !path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str::<RawMarchConfig>(&content)
        .with_context(|| format!("failed to parse {}", path.display()))
        .map(Some)
}
