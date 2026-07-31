//! project（项目实例）—— ~/.skillkit/projects/<id>.toml。id 注册时随机生成、冻结；
//! path/name 可变（rebind 重绑定）。installed_skills 是 apply 唯一依据，
//! locked_shas 是上次 apply 的基线快照（非版本锁）。
use crate::error::{atomic_write, Result, SkillkitError};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// 注册时生成的 8 hex 短码，冻结（rebind 不变）。
    pub id: String,
    /// 从 path basename，rebind 随新 path 调整。
    pub name: String,
    /// 项目实际位置，rebind 更新。
    pub path: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub applied_profiles: Vec<String>,
    #[serde(default)]
    pub installed_skills: Vec<String>,
    #[serde(default)]
    pub locked_shas: BTreeMap<String, String>,
}

/// 生成新 project-id：uuid v4 前 8 hex 大写（独立于 path，支持 rebind）。
pub fn new_id() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_uppercase()
}

impl Project {
    /// 注册新项目：生成随机 id，name/path 取自传入路径。
    pub fn register(abs_path: PathBuf, agents: Vec<String>) -> Self {
        let name = abs_path
            .file_name()
            .map_or_else(|| "project".into(), |s| s.to_string_lossy().into_owned());
        Self {
            id: new_id(),
            name,
            path: abs_path.to_string_lossy().into_owned(),
            agents,
            applied_profiles: vec![],
            installed_skills: vec![],
            locked_shas: BTreeMap::new(),
        }
    }

    pub fn load(paths: &Paths, id: &str) -> Result<Self> {
        let path = paths.projects_dir().join(format!("{id}.toml"));
        if !path.exists() {
            return Err(SkillkitError::ProjectNotFound { id: id.to_string() });
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let dir = paths.projects_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.toml", self.id));
        atomic_write(&path, &toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// 重绑定：项目移动/改名后更新 path/name，id 不变。
    pub fn rebind(&mut self, new_path: &Path) {
        let abs = new_path
            .canonicalize()
            .unwrap_or_else(|_| new_path.to_path_buf());
        self.path = abs.to_string_lossy().into_owned();
        if let Some(name) = abs.file_name() {
            self.name = name.to_string_lossy().into_owned();
        }
    }

    pub fn add_skill(&mut self, id: &str) -> Result<()> {
        if self.installed_skills.iter().any(|s| s == id) {
            return Err(SkillkitError::SkillAlreadyInstalled { id: id.to_string() });
        }
        self.installed_skills.push(id.to_string());
        Ok(())
    }

    pub fn remove_skill(&mut self, id: &str) -> Result<()> {
        let before = self.installed_skills.len();
        self.installed_skills.retain(|s| s != id);
        if self.installed_skills.len() == before {
            return Err(SkillkitError::SkillNotInstalled { id: id.to_string() });
        }
        Ok(())
    }

    pub fn apply_profile(&mut self, profile_name: &str, skill_ids: &[String]) {
        if !self.applied_profiles.iter().any(|p| p == profile_name) {
            self.applied_profiles.push(profile_name.to_string());
        }
        for id in skill_ids {
            if !self.installed_skills.iter().any(|s| s == id) {
                self.installed_skills.push(id.clone());
            }
        }
    }
}

pub fn list_ids(paths: &Paths) -> Result<Vec<String>> {
    let dir = paths.projects_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    #[test]
    fn new_id_is_short_hex_and_unique() {
        let id1 = new_id();
        let id2 = new_id();
        assert_eq!(id1.len(), 8, "8 hex 短码");
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(id1, id2, "每次随机生成应不同");
    }

    #[test]
    fn rebind_updates_path_name_keeps_id() {
        let mut proj = Project {
            id: "ABC12345".into(),
            name: "old".into(),
            path: "/tmp/old".into(),
            agents: vec![],
            applied_profiles: vec![],
            installed_skills: vec![],
            locked_shas: BTreeMap::new(),
        };
        proj.rebind(Path::new("/tmp/new-name"));
        assert_eq!(proj.id, "ABC12345", "重绑定 id 不变");
        assert_eq!(proj.name, "new-name");
        assert!(proj.path.contains("new-name"));
    }

    #[test]
    fn register_and_apply_profile_persists() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let mut proj = Project::register(PathBuf::from("/tmp/demo"), vec!["claude-code".into()]);
        let id = proj.id.clone();
        proj.add_skill("dc/logseq").unwrap();
        proj.apply_profile("frontend", &["dc/dataviz".into(), "dc/logseq".into()]);
        assert_eq!(proj.installed_skills, vec!["dc/logseq", "dc/dataviz"]);
        proj.save(&paths).unwrap();

        let reloaded = Project::load(&paths, &id).unwrap();
        assert_eq!(reloaded.id, id);
        assert_eq!(reloaded.installed_skills.len(), 2);
    }

    #[test]
    fn load_missing_fails() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        assert!(matches!(
            Project::load(&paths, "nope"),
            Err(SkillkitError::ProjectNotFound { .. })
        ));
    }
}
