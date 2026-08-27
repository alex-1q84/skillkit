//! 已安装 skill 元数据（registry.json），以 id 为 key。单版本模型：canonical 物理
//! 只有一份，版本信息记在 computed_hash（源自 skills-lock.json），registry 不存多版本。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Local,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Global => write!(f, "global"),
            Self::Local => write!(f, "local"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    pub source: String,
    pub scope: Scope,
    pub version: Option<String>,
    pub computed_hash: Option<String>,
    pub installed_at: String,
    pub canonical_path: String,
}

impl SkillMeta {
    /// 模板用：判断 scope（避免 Askama 表达式里写 Scope::Local 变体路径不可靠）。
    pub fn is_local(&self) -> bool {
        self.scope == Scope::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    pub skills: BTreeMap<String, SkillMeta>,
}

impl Registry {
    /// 生成 skill id：`<source-name>/<skill-name>`。跨实体引用都用它（DRY）。
    pub fn skill_id(source: &str, name: &str) -> String {
        format!("{source}/{name}")
    }

    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.registry_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let _lock = crate::lock::FileLock::acquire(paths, "registry")?;
        let path = paths.registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::error::atomic_write(&path, &serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// 写 registry，不获取锁（调用方须已持 "registry" 锁）。供持锁全流程的调用方用，
    /// 避免 install_local 已持锁时 Registry::save 再取同 key 致同进程 flock 自死锁。
    pub(crate) fn save_raw(&self, paths: &Paths) -> Result<()> {
        let path = paths.registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::error::atomic_write(&path, &serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn upsert(&mut self, meta: SkillMeta) {
        self.skills.insert(meta.id.clone(), meta);
    }

    pub fn get(&self, id: &str) -> Result<&SkillMeta> {
        self.skills
            .get(id)
            .ok_or_else(|| SkillkitError::SkillNotInstalled { id: id.to_string() })
    }

    pub fn remove(&mut self, id: &str) -> Result<&mut Self> {
        if self.skills.remove(id).is_none() {
            return Err(SkillkitError::SkillNotInstalled { id: id.to_string() });
        }
        Ok(self)
    }
}

/// 持 "registry" 锁的写事务：acquire → load → f → save_raw。
/// 闭包内做 registry 变更（可含必要的物理迁移，锁窗口内与并发写方串行化）；
/// 网络等长耗时操作应在调用前完成（锁外）。闭包内勿再 Registry::load / FileLock::acquire
/// （同进程 flock 重取自死锁），一律操作传入的 &mut Registry。
pub(crate) fn with_registry<R>(
    paths: &Paths,
    f: impl FnOnce(&mut Registry) -> Result<R>,
) -> Result<R> {
    let _lock = crate::lock::FileLock::acquire(paths, "registry")?;
    let mut reg = Registry::load(paths)?;
    let out = f(&mut reg)?;
    reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    fn meta(id: &str, scope: Scope) -> SkillMeta {
        SkillMeta {
            id: id.into(),
            name: id.split('/').nth(1).unwrap_or(id).into(),
            source: id.split('/').next().unwrap_or("").into(),
            scope,
            version: Some("1.0.0".into()),
            computed_hash: Some("abc123".into()),
            installed_at: "2026-07-29T00:00:00Z".into(),
            canonical_path: format!("~/.agents/skills/{}", id.split('/').nth(1).unwrap_or(id)),
        }
    }

    #[test]
    fn skill_id_format() {
        assert_eq!(
            Registry::skill_id("skills.sh", "frontend-design"),
            "skills.sh/frontend-design"
        );
    }

    #[test]
    fn upsert_get_remove_roundtrip() {
        let p = paths();
        let mut reg = Registry::load(&p).unwrap();
        let m = meta("skills.sh/foo", Scope::Global);
        reg.upsert(m.clone());
        reg.save(&p).unwrap();

        let mut reloaded = Registry::load(&p).unwrap();
        assert_eq!(reloaded.get("skills.sh/foo").unwrap().name, "foo");
        assert_eq!(reloaded.skills.len(), 1);

        reloaded.remove("skills.sh/foo").unwrap().save(&p).unwrap();
        assert!(Registry::load(&p).unwrap().get("skills.sh/foo").is_err());
    }

    #[test]
    fn remove_missing_fails() {
        let mut reg = Registry::default();
        assert!(matches!(
            reg.remove("nope"),
            Err(SkillkitError::SkillNotInstalled { .. })
        ));
    }

    #[test]
    fn with_registry_persists_and_releases_lock() {
        let p = paths();
        with_registry(&p, |reg| {
            reg.upsert(meta("local/x", Scope::Local));
            Ok(())
        })
        .unwrap();
        // 事务落盘可见
        assert!(Registry::load(&p).unwrap().get("local/x").is_ok());
        // 锁已释放：下一笔事务能正常进行（若锁泄漏此处会 LockTimeout）
        with_registry(&p, |reg| reg.remove("local/x").map(|_| ())).unwrap();
        assert!(Registry::load(&p).unwrap().get("local/x").is_err());
    }

    #[test]
    fn with_registry_error_skips_save() {
        let p = paths();
        let err = with_registry(&p, |reg| -> Result<()> {
            reg.upsert(meta("local/y", Scope::Local));
            Err(SkillkitError::SkillNotInstalled { id: "boom".into() })
        })
        .unwrap_err();
        assert!(matches!(err, SkillkitError::SkillNotInstalled { .. }));
        // 闭包报错则不落盘：中途变更不残留
        assert!(Registry::load(&p).unwrap().get("local/y").is_err());
    }
}
