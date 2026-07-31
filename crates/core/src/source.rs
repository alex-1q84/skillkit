//! 安装源注册表（sources.toml）。三种源：skills-sh / git / local。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceType {
    SkillsSh,
    Git,
    Local,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SkillsSh => write!(f, "skills-sh"),
            Self::Git => write!(f, "git"),
            Self::Local => write!(f, "local"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub source_type: SourceType,
    pub url: Option<String>,
    pub path: Option<String>,
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    /// skill 在 git/local 源仓库中的子目录（一仓库多 skill 场景）；None=skill 在仓库根。
    pub skills_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourcesStore {
    #[serde(default)]
    pub sources: Vec<Source>,
}

impl SourcesStore {
    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.sources_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let _lock = crate::lock::FileLock::acquire(paths, "sources")?;
        let path = paths.sources_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::error::atomic_write(&path, &toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn list(&self) -> &[Source] {
        &self.sources
    }

    pub fn add(&mut self, source: Source) -> Result<()> {
        if self.sources.iter().any(|s| s.name == source.name) {
            return Err(SkillkitError::SkillAlreadyInstalled {
                id: format!("source:{}", source.name),
            });
        }
        self.sources.push(source);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<&mut Self> {
        let before = self.sources.len();
        self.sources.retain(|s| s.name != name);
        if self.sources.len() == before {
            return Err(SkillkitError::SourceNotFound {
                name: name.to_string(),
            });
        }
        Ok(self)
    }

    pub fn get(&self, name: &str) -> Result<&Source> {
        self.sources
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| SkillkitError::SourceNotFound {
                name: name.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    #[test]
    fn add_then_list_then_remove() {
        let p = paths();
        let mut store = SourcesStore::load(&p).unwrap();
        assert!(store.list().is_empty());

        store
            .add(Source {
                name: "team-private".into(),
                source_type: SourceType::Git,
                url: Some("git@github.com:org/team.git".into()),
                path: None,
                ref_: Some("main".into()),
                skills_dir: None,
            })
            .unwrap();
        store.save(&p).unwrap();

        let mut reloaded = SourcesStore::load(&p).unwrap();
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].name, "team-private");

        reloaded.remove("team-private").unwrap().save(&p).unwrap();
        assert!(SourcesStore::load(&p).unwrap().list().is_empty());
    }

    #[test]
    fn add_duplicate_fails() {
        let mut store = SourcesStore::default();
        let s = Source {
            name: "x".into(),
            source_type: SourceType::Local,
            url: None,
            path: Some("~/x".into()),
            ref_: None,
            skills_dir: None,
        };
        store.add(s.clone()).unwrap();
        assert!(store.add(s).is_err());
    }

    #[test]
    fn remove_missing_fails() {
        let mut store = SourcesStore::default();
        assert!(matches!(
            store.remove("nope"),
            Err(SkillkitError::SourceNotFound { .. })
        ));
    }
}
