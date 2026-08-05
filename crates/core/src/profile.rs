//! profile（粗分类候选集）—— ~/.skillkit/profiles/<name>.toml，只存 skill id 列表（DRY）。
//! source/scope/version 等信息只在 registry 存一份，profile 不重复。
use crate::error::{atomic_write, Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Registry, Scope};
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

    /// 加 skill：先查 registry 拒绝 global（core 硬约束），再查重。非幂等（重复返 SkillAlreadyInstalled）。
    /// registry 无记录的 id 按 Local 兜底（不拒绝），仅拦截明确的 global。
    pub fn add_skill(&mut self, id: &str, registry: &Registry) -> Result<()> {
        if registry.get(id).map(|m| m.scope).unwrap_or(Scope::Local) == Scope::Global {
            return Err(SkillkitError::SkillIsGlobal { id: id.to_string() });
        }
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

/// 反向索引：扫所有 profile，返回含 skill_id 的 profile name 列表。
/// global skill 永远空（不属任何 profile，语义保证）。现算不缓存（profile 数量小，YAGNI）。
pub fn skill_profiles(paths: &Paths, skill_id: &str) -> Vec<String> {
    let reg = Registry::load(paths).unwrap_or_default();
    // global 直接空（不依赖 profile 实存，registry 标 global 即不属任何 profile）
    if reg.get(skill_id).map(|m| m.scope).unwrap_or(Scope::Local) == Scope::Global {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Ok(names) = list_names(paths) {
        for name in names {
            if let Ok(p) = Profile::load(paths, &name) {
                if p.skills.iter().any(|s| s == skill_id) {
                    out.push(name);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    /// 增量建 registry：load 现有 + upsert 一条 + save（多次调用累积，供 skill_profiles 的 load 短路生效）。
    fn reg_with(paths: &Paths, id: &str, scope: Scope) -> Registry {
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(crate::registry::SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!(
                "~/.skillkit/.agents/skills/{}",
                id.rsplit('/').next().unwrap()
            ),
        });
        reg.save(paths).unwrap();
        reg
    }

    #[test]
    fn add_skill_local_persists_and_dedups() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/fe", Scope::Local);
        let mut profile = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec![],
        };
        profile.add_skill("skills.sh/fe", &reg).unwrap();
        // 重复 add 报 SkillAlreadyInstalled
        assert!(matches!(
            profile.add_skill("skills.sh/fe", &reg),
            Err(SkillkitError::SkillAlreadyInstalled { .. })
        ));
    }

    #[test]
    fn add_skill_global_rejected() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/g1", Scope::Global);
        let mut profile = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec![],
        };
        assert!(matches!(
            profile.add_skill("skills.sh/g1", &reg),
            Err(SkillkitError::SkillIsGlobal { .. })
        ));
        assert!(profile.skills.is_empty(), "拒绝时 skills 不变");
    }

    #[test]
    fn remove_skill_persists() {
        let p = paths();
        Profile {
            name: "frontend".into(),
            description: String::new(),
            skills: vec![
                "skills.sh/frontend-design".into(),
                "skills.sh/dataviz".into(),
            ],
        }
        .save(&p)
        .unwrap();
        let mut reloaded = Profile::load(&p, "frontend").unwrap();
        reloaded.remove_skill("skills.sh/dataviz").unwrap();
        reloaded.save(&p).unwrap();
        assert_eq!(
            Profile::load(&p, "frontend").unwrap().skills,
            vec!["skills.sh/frontend-design"]
        );
    }

    #[test]
    fn skill_profiles_reverses_and_global_empty() {
        let p = paths();
        reg_with(&p, "skills.sh/fe", Scope::Local);
        reg_with(&p, "skills.sh/g1", Scope::Global);
        // fe 含 fe（走 add_skill），base 手工含 fe
        let reg = Registry::load(&p).unwrap();
        let mut fe = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec![],
        };
        fe.add_skill("skills.sh/fe", &reg).unwrap();
        fe.save(&p).unwrap();
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["skills.sh/fe".into()],
        }
        .save(&p)
        .unwrap();
        // legacy：手工塞 global 进 profile（绕过校验模拟存量）
        Profile {
            name: "legacy".into(),
            description: String::new(),
            skills: vec!["skills.sh/g1".into()],
        }
        .save(&p)
        .unwrap();

        let mut got = skill_profiles(&p, "skills.sh/fe");
        got.sort();
        assert_eq!(got, vec!["base".to_string(), "fe".to_string()]);
        // global 永远空（即使 legacy profile 含它）
        assert!(skill_profiles(&p, "skills.sh/g1").is_empty());
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
