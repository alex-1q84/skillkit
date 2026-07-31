//! 安装源注册表（sources.toml）。Source 极简成 {name, package}：
//! package 是 npx skills 的 source format 串（github shorthand / git url / local path）；
//! None 表示 registry 搜索入口（skills.sh 默认源）。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    /// npx skills source format（github shorthand / git url / local path）；None=registry 搜索入口。
    pub package: Option<String>,
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
        // 旧 schema（含 source_type 字段）不兼容：备份后视为空（项目未发布，无线上数据）。
        if content.contains("source_type") {
            let _ = std::fs::rename(&path, path.with_extension("toml.bak"));
            return Ok(Self::default());
        }
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

    /// 入口层（CLI main / server 启动）调：sources.toml 不存在时种子写入 skills.sh
    /// 默认源（registry 搜索入口）。用户可删，删了不加回。
    pub fn ensure_default(paths: &Paths) -> Result<()> {
        if paths.sources_path().exists() {
            return Ok(());
        }
        Self {
            sources: vec![Source {
                name: "skills.sh".into(),
                package: None,
            }],
        }
        .save(paths)
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
                package: Some("git@github.com:org/team.git".into()),
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
            package: Some("~/x".into()),
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

    #[test]
    fn ensure_default_seeds_skills_sh_when_absent() {
        let p = paths();
        assert!(!p.sources_path().exists());
        SourcesStore::ensure_default(&p).unwrap();
        let store = SourcesStore::load(&p).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "skills.sh");
        assert!(store.list()[0].package.is_none());
        // 已存在不再覆盖
        SourcesStore::ensure_default(&p).unwrap();
        assert_eq!(SourcesStore::load(&p).unwrap().list().len(), 1);
    }

    #[test]
    fn legacy_schema_backed_up_and_reset() {
        let p = paths();
        std::fs::create_dir_all(p.skillkit_dir()).unwrap();
        std::fs::write(
            p.sources_path(),
            "[[sources]]\nname=\"old\"\nsource_type=\"git\"\nurl=\"x\"\n",
        )
        .unwrap();
        let store = SourcesStore::load(&p).unwrap();
        assert!(store.list().is_empty());
        assert!(p.sources_path().with_extension("toml.bak").exists());
    }
}
