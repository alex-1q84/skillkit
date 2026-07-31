//! profile（粗分类候选集）—— ~/.skillkit/profiles/<name>.toml，只存 skill id 列表（DRY）。
//! source/scope/version 等信息只在 registry 存一份，profile 不重复。
use crate::error::{atomic_write, Result, SkillkitError};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
}

impl Profile {
    pub fn load(paths: &Paths, name: &str) -> Result<Self> {
        let path = paths.profiles_dir().join(format!("{name}.toml"));
        if !path.exists() {
            return Err(SkillkitError::ProfileNotFound {
                name: name.to_string(),
            });
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let _lock = crate::lock::FileLock::acquire(paths, &format!("profile-{}", self.name))?;
        let dir = paths.profiles_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.toml", self.name));
        atomic_write(&path, &toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add_skill(&mut self, id: &str) -> Result<()> {
        if self.skills.iter().any(|s| s == id) {
            return Err(SkillkitError::SkillAlreadyInstalled { id: id.to_string() });
        }
        self.skills.push(id.to_string());
        Ok(())
    }

    pub fn remove_skill(&mut self, id: &str) -> Result<()> {
        let before = self.skills.len();
        self.skills.retain(|s| s != id);
        if self.skills.len() == before {
            return Err(SkillkitError::SkillNotInstalled { id: id.to_string() });
        }
        Ok(())
    }
}

/// profile 目录扫描（list 用）。
pub fn list_names(paths: &Paths) -> Result<Vec<String>> {
    let dir = paths.profiles_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
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
    fn add_remove_skill_persists() {
        let p = paths();
        let mut profile = Profile {
            name: "frontend".into(),
            description: String::new(),
            skills: vec![],
        };
        profile.add_skill("skills.sh/frontend-design").unwrap();
        profile.add_skill("skills.sh/dataviz").unwrap();
        assert!(
            profile.add_skill("skills.sh/frontend-design").is_err(),
            "重复 add 报错"
        );
        profile.save(&p).unwrap();

        let reloaded = Profile::load(&p, "frontend").unwrap();
        assert_eq!(
            reloaded.skills,
            vec!["skills.sh/frontend-design", "skills.sh/dataviz"]
        );

        let mut reloaded = Profile::load(&p, "frontend").unwrap();
        reloaded.remove_skill("skills.sh/dataviz").unwrap();
        reloaded.save(&p).unwrap();
        assert_eq!(
            Profile::load(&p, "frontend").unwrap().skills,
            vec!["skills.sh/frontend-design"]
        );
    }

    #[test]
    fn load_missing_fails() {
        let p = paths();
        assert!(matches!(
            Profile::load(&p, "nope"),
            Err(SkillkitError::ProfileNotFound { .. })
        ));
    }

    #[test]
    fn list_names_sorted() {
        let p = paths();
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec![],
        }
        .save(&p)
        .unwrap();
        Profile {
            name: "frontend".into(),
            description: String::new(),
            skills: vec![],
        }
        .save(&p)
        .unwrap();
        assert_eq!(list_names(&p).unwrap(), vec!["base", "frontend"]);
    }
}
