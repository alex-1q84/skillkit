//! install/uninstall：委托 npx skills 下载到 canonical 池子（~/.skillkit/.agents/skills/），
//! 读 skills-lock.json 记 computed_hash，登记 registry。scope=global 额外 symlink
//! 池子→~/.agents/skills/ + Claude 桥接。
use crate::error::{Result, SkillkitError};
use crate::npx;
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};
use crate::source::SourcesStore;
use std::path::PathBuf;

/// 安装：调 npx skills add 下载到池子，记 computed_hash，登记 registry。
/// `package` 由调用方解析（固定源用 source.package；registry 源由 CLI 层 find 选）。
/// scope=global 时额外 symlink 池子→~/.agents/skills/ + Claude 桥接，立即可用。
pub fn install(
    paths: &Paths,
    source_name: &str,
    skill_name: &str,
    package: &str,
    scope: Scope,
) -> Result<SkillMeta> {
    let store = SourcesStore::load(paths)?;
    let source = store.get(source_name)?.clone();

    let target = paths.skillkit_skills_dir().join(skill_name);
    if target.exists() {
        return Err(SkillkitError::SkillAlreadyInstalled {
            id: skill_name.to_string(),
        });
    }

    npx::add(paths, package, skill_name)?;
    let hash = npx::read_computed_hash(paths, skill_name)?;

    let id = Registry::skill_id(&source.name, skill_name);
    let meta = SkillMeta {
        id: id.clone(),
        name: skill_name.to_string(),
        source: source.name,
        scope,
        version: None,
        computed_hash: Some(hash),
        installed_at: now_iso(),
        canonical_path: target.display().to_string(),
    };
    // 登记 registry：持锁 load→upsert→save（npx 下载在锁外，网络操作不占锁），
    // 与并发写方（import/rescope）串行化，防旧快照 save 互相覆盖。
    let _lock = crate::lock::FileLock::acquire(paths, "registry")?;
    let mut reg = Registry::load(paths)?;
    reg.upsert(meta.clone());
    reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）

    // global：池子 → ~/.agents/skills/（agent 直读）+ ~/.claude/skills/（Claude 桥接）
    if scope == Scope::Global {
        crate::symlink::ensure_global_claude(paths, &meta)?;
    }
    Ok(meta)
}

/// 卸载：managed 删 canonical 池子 + 同步 npx skills lock；unmanaged（computed_hash=None）
/// 只摘 registry 记录，不删目录（不是 skillkit 装的，避免误删用户手工放置的 skill）。
pub fn uninstall(paths: &Paths, id: &str) -> Result<()> {
    let meta = Registry::load(paths)?.get(id)?.clone();
    if meta.computed_hash.is_some() {
        let target = PathBuf::from(&meta.canonical_path);
        if target.exists() {
            std::fs::remove_dir_all(&target)
                .map_err(|_| SkillkitError::RemoveFailed(target.clone()))?;
        }
        let _ = npx::remove(paths, &meta.name); // 同步 lock，失败不阻塞（registry 是事实源）
    }
    // 摘记录：物理删除/npx 在锁外（秒级），锁内重读再 remove，
    // 防基于删除前快照的 save 把并发写方（rescope/import）的写入覆盖回滚。
    let _lock = crate::lock::FileLock::acquire(paths, "registry")?;
    let mut reg = Registry::load(paths)?;
    reg.remove(id)?;
    reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
    Ok(())
}

/// 当前时间 ISO 字符串（UTC RFC3339）。
pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::registry::{Registry, Scope, SkillMeta};
    use tempfile::tempdir;

    /// unmanaged skill（computed_hash=None）uninstall 时：只摘 registry 记录，不删 canonical 目录。
    #[test]
    fn uninstall_unmanaged_keeps_directory() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());

        // 存量真实目录（模拟 ~/.agents/skills/foo，用户手工放置）
        let canon = tmp.path().join(".agents/skills/foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();

        let mut reg = Registry::load(&paths).unwrap();
        reg.upsert(SkillMeta {
            id: "unmanaged/foo".into(),
            name: "foo".into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "2026-07-31T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(&paths).unwrap();

        uninstall(&paths, "unmanaged/foo").unwrap();

        assert!(canon.exists(), "unmanaged 的目录不能被删");
        assert!(Registry::load(&paths)
            .unwrap()
            .get("unmanaged/foo")
            .is_err());
    }

    /// managed skill（computed_hash=Some）uninstall 仍删 canonical 目录（行为不变）。
    #[test]
    fn uninstall_managed_still_removes_directory() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let canon = paths.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();

        let mut reg = Registry::load(&paths).unwrap();
        reg.upsert(SkillMeta {
            id: "skills.sh/foo".into(),
            name: "foo".into(),
            source: "skills.sh".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: Some("abc123".into()),
            installed_at: "2026-07-31T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(&paths).unwrap();

        uninstall(&paths, "skills.sh/foo").unwrap();
        assert!(!canon.exists(), "managed 的 canonical 目录应被删");
    }
}
