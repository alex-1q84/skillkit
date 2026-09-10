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
/// force=true 时覆盖同名占用：registry 有记录走 uninstall 清场，unmanaged 目录 /
/// 孤儿目录补删（覆盖即用户明确要求替换），然后正常安装。
pub fn install(
    paths: &Paths,
    source_name: &str,
    skill_name: &str,
    package: &str,
    scope: Scope,
    force: bool,
) -> Result<SkillMeta> {
    let store = SourcesStore::load(paths)?;
    let source = store.get(source_name)?.clone();

    let target = paths.skillkit_skills_dir().join(skill_name);
    if target.exists() {
        if !force {
            return Err(SkillkitError::SkillAlreadyInstalled {
                id: skill_name.to_string(),
            });
        }
        clear_occupied(paths, skill_name)?;
    }

    npx::add(paths, package, skill_name)?;
    let hash = npx::read_computed_hash(paths, skill_name)?;

    let id = Registry::skill_id(&source.name, skill_name);
    // registry 源（sources.toml package=None）的 package 参数即 spec，记原始安装名；固定源无此概念。
    let spec = if source.package.is_none() {
        Some(package.to_string())
    } else {
        None
    };
    let meta = SkillMeta {
        id: id.clone(),
        name: skill_name.to_string(),
        source: source.name,
        spec,
        scope,
        version: None,
        computed_hash: Some(hash),
        installed_at: now_iso(),
        canonical_path: target.display().to_string(),
    };
    // 登记 registry：持锁写事务（npx 下载在锁外，网络操作不占锁），
    // 与并发写方（import/rescope）串行化，防旧快照 save 互相覆盖。
    crate::registry::with_registry(paths, |reg| {
        reg.upsert(meta.clone());
        Ok(())
    })?;

    // global：池子 → ~/.agents/skills/（agent 直读）+ ~/.claude/skills/（Claude 桥接）
    if scope == Scope::Global {
        crate::symlink::ensure_global_claude(paths, &meta)?;
    }
    Ok(meta)
}

/// 覆盖安装的清场：按短名摘掉 registry 里全部同名记录（复用 uninstall：managed
/// 撤桥接/删目录/同步 lock，unmanaged 只摘记录），unmanaged 目录与孤儿目录在此补删
/// （uninstall 对 unmanaged 不删目录是防误删；覆盖是用户明确要求替换，语义不同）。
fn clear_occupied(paths: &Paths, skill_name: &str) -> Result<()> {
    let reg = Registry::load(paths)?;
    let ids: Vec<String> = reg
        .skills
        .values()
        .filter(|m| m.name == skill_name)
        .map(|m| m.id.clone())
        .collect();
    for id in ids {
        uninstall(paths, &id)?;
    }
    let target = paths.skillkit_skills_dir().join(skill_name);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|_| SkillkitError::RemoveFailed(target.clone()))?;
    }
    Ok(())
}

/// 卸载：managed 撤 global 桥接 + 删 canonical 池子 + 同步 npx skills lock；unmanaged
/// （computed_hash=None）只摘 registry 记录，不删目录（不是 skillkit 装的，避免误删用户
/// 手工放置的 skill）。
pub fn uninstall(paths: &Paths, id: &str) -> Result<()> {
    let meta = Registry::load(paths)?.get(id)?.clone();
    if meta.computed_hash.is_some() {
        // 先撤 global 桥接（~/.agents/skills/ + ~/.claude/skills/）再删 canonical，
        // 否则池子删掉后桥接残留成 dangling。与 ensure_global_claude 对称（install 时建）；
        // unmanaged 无桥接语义（目录本就属用户），不走此分支。
        crate::symlink::remove_global_claude(paths, &meta)?;
        let target = PathBuf::from(&meta.canonical_path);
        if target.exists() {
            std::fs::remove_dir_all(&target)
                .map_err(|_| SkillkitError::RemoveFailed(target.clone()))?;
        }
        let _ = npx::remove(paths, &meta.name); // 同步 lock，失败不阻塞（registry 是事实源）
    }
    // 摘记录：物理删除/npx 在锁外（秒级），锁内重读再 remove，
    // 防基于删除前快照的 save 把并发写方（rescope/import）的写入覆盖回滚。
    crate::registry::with_registry(paths, |reg| reg.remove(id).map(|_| ()))?;
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
            spec: None,
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

    /// managed + global：uninstall 时撤两层 global 桥接（~/.agents/skills/ + ~/.claude/skills/），
    /// 不留 dangling symlink。
    #[test]
    fn uninstall_global_managed_removes_bridge_links() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let canon = paths.skillkit_skills_dir().join("pdf");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();

        let meta = SkillMeta {
            id: "skills.sh/pdf".into(),
            name: "pdf".into(),
            source: "skills.sh".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: Some("abc".into()),
            spec: None,
            installed_at: "2026-08-21T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        };
        let mut reg = Registry::load(&paths).unwrap();
        reg.upsert(meta.clone());
        reg.save(&paths).unwrap();
        crate::symlink::ensure_global_claude(&paths, &meta).unwrap();
        let agents_link = paths.agents_skills_dir().join("pdf");
        let claude_link = paths.claude_skills_dir().join("pdf");
        assert!(agents_link.is_symlink() && claude_link.is_symlink());

        uninstall(&paths, "skills.sh/pdf").unwrap();
        assert!(!canon.exists(), "canonical 应被删");
        assert!(
            !agents_link.exists() && !agents_link.is_symlink(),
            "~/.agents/skills/ 落地不应残留 dangling"
        );
        assert!(
            !claude_link.exists() && !claude_link.is_symlink(),
            "~/.claude/skills/ 桥接不应残留 dangling"
        );
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
            spec: None,
            installed_at: "2026-07-31T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(&paths).unwrap();

        uninstall(&paths, "skills.sh/foo").unwrap();
        assert!(!canon.exists(), "managed 的 canonical 目录应被删");
    }

    /// force 覆盖：unmanaged 同名记录被摘、目录被替换，skillkit 正常登记。
    /// WHY：unmanaged 的 uninstall 不删目录（防误删），只有覆盖语义才允许删——
    /// 若 clear_occupied 漏删，install 会因目录仍存在而失败或漏装。
    #[test]
    fn install_force_replaces_unmanaged_occupant() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 种 skills.sh 源 + 假 npx（add 写 lock，免真实网络）
        SourcesStore::ensure_default(&paths).unwrap();
        let _guard = fake_npx_add(&paths);

        let canon = paths.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "old").unwrap();
        let mut reg = Registry::load(&paths).unwrap();
        reg.upsert(SkillMeta {
            id: "unmanaged/foo".into(),
            name: "foo".into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            spec: None,
            installed_at: "2026-07-31T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(&paths).unwrap();

        // 非 force 仍拒绝
        assert!(install(&paths, "skills.sh", "foo", "o/r@foo", Scope::Local, false).is_err());
        let meta = install(&paths, "skills.sh", "foo", "o/r@foo", Scope::Local, true).unwrap();
        assert_eq!(meta.computed_hash.as_deref(), Some("hashnew"));
        assert!(canon.join("SKILL.md").exists(), "目录应被新安装内容替换");
        assert!(Registry::load(&paths)
            .unwrap()
            .get("unmanaged/foo")
            .is_err());
        assert!(Registry::load(&paths).unwrap().get("skills.sh/foo").is_ok());
    }

    /// force 覆盖孤儿目录（registry 无记录）：目录被清掉后正常安装登记。
    #[test]
    fn install_force_clears_orphan_directory() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        SourcesStore::ensure_default(&paths).unwrap();
        let _guard = fake_npx_add(&paths);

        let canon = paths.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "orphan").unwrap();

        let meta = install(&paths, "skills.sh", "foo", "o/r@foo", Scope::Local, true).unwrap();
        assert_eq!(meta.computed_hash.as_deref(), Some("hashnew"));
        assert!(Registry::load(&paths).unwrap().get("skills.sh/foo").is_ok());
    }

    /// 测试用假 npx：响应 `skills@latest add <pkg> -s <skill> ...`，建 skill 目录 +
    /// 写 lock（computedHash=hashnew），模拟 npx 成功安装；其余调用退出码 1。
    /// RAII 守卫包 PATH 变更，drop 时还原，避免污染并行测试。
    fn fake_npx_add(paths: &Paths) -> PathGuard {
        let bin = paths.skillkit_dir().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let sh = bin.join("npx");
        std::fs::write(
            &sh,
            "#!/bin/sh\n\
             if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"add\" ]; then\n\
             \x20 for i in 3 4 5 6 7 8; do\n\
             \x20   if [ \"$(eval echo \\$$i)\" = \"-s\" ]; then\n\
             \x20     skill=$(eval echo \\$$((i+1)))\n\
             \x20     mkdir -p \".agents/skills/$skill\"\n\
             \x20     printf 'new' > \".agents/skills/$skill/SKILL.md\"\n\
             \x20     printf '{\"skills\": {\"%s\": {\"computedHash\": \"hashnew\"}}}' \"$skill\" > skills-lock.json\n\
             \x20     exit 0\n\
             \x20   fi\n\
             \x20 done\n\
             \x20 exit 1\n\
             fi\n\
             exit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let old = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", bin.display(), old));
        PathGuard { old }
    }

    /// RAII 守卫：构造时备份 PATH，drop 时还原，避免污染并行测试。
    struct PathGuard {
        old: String,
    }
    impl Drop for PathGuard {
        fn drop(&mut self) {
            if self.old.is_empty() {
                std::env::remove_var("PATH");
            } else {
                std::env::set_var("PATH", &self.old);
            }
        }
    }
}
