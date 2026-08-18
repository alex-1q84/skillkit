//! scope 转移：global↔local，转移即同步物理落地 + 自动清理 profile/project 引用。
use crate::error::Result;
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};

/// rescope 报告：受影响的 profile/project（local→global 清理时填）。
#[derive(Debug, Clone, PartialEq)]
pub struct RescopeReport {
    pub affected_profiles: Vec<String>,
    pub affected_projects: Vec<String>,
}

/// 改 skill 的 scope 并同步物理落地。
/// local→global：先改 scope（内存）→ 建全局落地 → 落盘 → 清 profile/project 引用。
///   顺序要点：ensure_global_claude 有 scope 守卫（scope != Global → no-op），
///   必须先把 meta.scope 改成 Global 再调 ensure，否则建链被守卫跳过（留 scope=global 却无 symlink）。
/// global→local：撤全局落地（remove_global_claude 不加守卫，meta.scope 仍 Global 时安全删）→ 改 scope → 落盘。
/// 落地/落盘失败原子回滚（registry 不 save，scope 不变）；profile/project 多文件清理非原子（见 spec §6）。
/// 全程持 "registry" 锁：load→物理迁移→save 完整窗口与并发写方（import/upgrade）串行化，
/// 防旧快照 save 把本函数的写入覆盖回滚（rescope 效果静默丢失）。
pub fn set_scope(paths: &Paths, id: &str, target: Scope) -> Result<RescopeReport> {
    let lock = crate::lock::FileLock::acquire(paths, "registry")?;
    let mut reg = Registry::load(paths)?;
    let mut meta: SkillMeta = reg.get(id)?.clone();
    if meta.scope == target {
        return Ok(RescopeReport {
            affected_profiles: vec![],
            affected_projects: vec![],
        });
    }
    let prev = meta.scope;
    match (prev, target) {
        (Scope::Local, Scope::Global) => {
            // 先改 scope → ensure（守卫通过建链）→ save。ensure 失败则 registry 未落盘（scope 仍 local），原子。
            meta.scope = Scope::Global;
            crate::symlink::ensure_global_claude(paths, &meta)?;
            reg.upsert(meta.clone());
            reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
            drop(lock); // registry 写完即放，引用清理（profile/project 独立锁）不占 registry 锁
                        // 清 profile/project 引用（跨多文件，非原子——失败给可恢复文案，见 spec §6）
            let (ap, aproj) = remove_refs(paths, id)?;
            Ok(RescopeReport {
                affected_profiles: ap,
                affected_projects: aproj,
            })
        }
        (Scope::Global, Scope::Local) => {
            // 找 unmanaged 的真实 canonical（全局位置的真实目录，非 symlink）。
            // 不信任 registry canonical_path——它可能漂移（历史 import + 后续移动，如 docx）。
            // managed global 的全局位置是 symlink（→ 池子 canonical），real_canon 找不到，走 remove。
            let name = &meta.name;
            let agents_link = paths.agents_skills_dir().join(name);
            let claude_link = paths.claude_skills_dir().join(name);
            let real_canon = [agents_link.as_path(), claude_link.as_path()]
                .into_iter()
                .find(|p| {
                    std::fs::symlink_metadata(p)
                        .is_ok_and(|m| m.file_type().is_dir() && !m.file_type().is_symlink())
                });
            if let Some(src) = real_canon {
                // unmanaged：迁移真实 canonical 到池子（managed-local）
                let target = paths.skillkit_skills_dir().join(name);
                if target.exists() {
                    // 池子已有同名 canonical（旧 managed 残留/历史迁移）；src（全局位置）是重复，删它，canonical 用池子。
                    // 风险：若 src 与 target 内容不一致会丢 src 独有数据——rescope 语义是 canonical 进池子，
                    // 池子已有即视为权威 canonical，全局位置副本冗余。
                    std::fs::remove_dir_all(src)?;
                } else {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(src, &target)?;
                }
                meta.canonical_path = target.to_string_lossy().into_owned();
            }
            // 撤全局 symlink：managed 撤 agents+claude symlink；unmanaged 迁移后原位置已不存在（跳过）。
            // remove 不加守卫，meta.scope 仍 Global 时安全。
            crate::symlink::remove_global_claude(paths, &meta)?;
            meta.scope = Scope::Local;
            reg.upsert(meta.clone());
            reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
                                  // global 本不在 profile/project，无需清
            Ok(RescopeReport {
                affected_profiles: vec![],
                affected_projects: vec![],
            })
        }
        _ => Ok(RescopeReport {
            affected_profiles: vec![],
            affected_projects: vec![],
        }),
    }
}

/// 从所有 profile.skills 和 project.installed_skills 移除 id。返回受影响名单。
/// 跨多文件独立 FileLock，非原子：逐个改逐个存，失败向上抛（已改的保留，调用方给可恢复文案）。
fn remove_refs(paths: &Paths, id: &str) -> Result<(Vec<String>, Vec<String>)> {
    let mut affected_profiles = Vec::new();
    for name in crate::profile::list_names(paths)? {
        if let Ok(mut p) = crate::profile::Profile::load(paths, &name) {
            if p.skills.iter().any(|s| s == id) {
                p.skills.retain(|s| s != id);
                p.save(paths)?;
                affected_profiles.push(name);
            }
        }
    }
    let mut affected_projects = Vec::new();
    for pid in crate::project::list_ids(paths)? {
        if let Ok(mut proj) = crate::project::Project::load(paths, &pid) {
            if proj.installed_skills.iter().any(|s| s == id) {
                proj.installed_skills.retain(|s| s != id);
                proj.save(paths)?;
                affected_projects.push(pid);
            }
        }
    }
    Ok((affected_profiles, affected_projects))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SkillkitError;
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    fn seed_skill(paths: &Paths, id: &str, scope: Scope) -> SkillMeta {
        let name = id.rsplit('/').next().unwrap();
        let canon = paths.skillkit_skills_dir().join(name);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let meta = SkillMeta {
            id: id.into(),
            name: name.into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        };
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(meta.clone());
        reg.save(paths).unwrap();
        meta
    }

    #[test]
    fn local_to_global_builds_links_and_clears_refs() {
        let p = paths();
        seed_skill(&p, "dc/fe", Scope::Local);
        let fe = crate::profile::Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/fe".into()],
        };
        fe.save(&p).unwrap();
        let proj = crate::project::Project {
            id: "P1".into(),
            name: "p".into(),
            path: "/tmp/p".into(),
            agents: vec![],
            applied_profiles: vec![],
            installed_skills: vec!["dc/fe".into()],
            locked_shas: std::collections::BTreeMap::new(),
        };
        proj.save(&p).unwrap();

        let report = set_scope(&p, "dc/fe", Scope::Global).unwrap();
        assert_eq!(report.affected_profiles, vec!["fe".to_string()]);
        assert_eq!(report.affected_projects, vec!["P1".to_string()]);
        assert!(p.agents_skills_dir().join("fe").is_symlink());
        assert!(p.claude_skills_dir().join("fe").is_symlink());
        assert!(crate::profile::Profile::load(&p, "fe")
            .unwrap()
            .skills
            .is_empty());
        assert!(crate::project::Project::load(&p, "P1")
            .unwrap()
            .installed_skills
            .is_empty());
        assert_eq!(
            Registry::load(&p).unwrap().get("dc/fe").unwrap().scope,
            Scope::Global
        );
    }

    #[test]
    fn global_to_local_removes_links_keeps_canonical() {
        let p = paths();
        let meta = seed_skill(&p, "dc/g", Scope::Global);
        crate::symlink::ensure_global_claude(&p, &meta).unwrap();
        assert!(p.agents_skills_dir().join("g").is_symlink());

        let report = set_scope(&p, "dc/g", Scope::Local).unwrap();
        assert!(report.affected_profiles.is_empty() && report.affected_projects.is_empty());
        assert!(!p.agents_skills_dir().join("g").exists());
        assert!(!p.claude_skills_dir().join("g").exists());
        // canonical 保留
        let canon = std::path::Path::new(&meta.canonical_path);
        assert!(canon.exists(), "canonical 池子保留");
        assert_eq!(
            Registry::load(&p).unwrap().get("dc/g").unwrap().scope,
            Scope::Local
        );
    }

    #[test]
    fn same_scope_noop() {
        let p = paths();
        seed_skill(&p, "dc/x", Scope::Local);
        let report = set_scope(&p, "dc/x", Scope::Local).unwrap();
        assert!(report.affected_profiles.is_empty());
    }

    #[test]
    fn missing_skill_errors() {
        let p = paths();
        assert!(matches!(
            set_scope(&p, "nope/x", Scope::Global),
            Err(SkillkitError::SkillNotInstalled { .. })
        ));
    }

    #[test]
    fn unmanaged_global_to_local_migrates_canonical() {
        // unmanaged：canonical 在 ~/.agents/skills/（真实目录，import 登记的 global skill 模型）
        let p = paths();
        let name = "gskill";
        let agents_canon = p.agents_skills_dir().join(name);
        std::fs::create_dir_all(&agents_canon).unwrap();
        std::fs::write(agents_canon.join("SKILL.md"), "x").unwrap();
        let meta = SkillMeta {
            id: "unmanaged/gskill".into(),
            name: name.into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "t".into(),
            canonical_path: agents_canon.to_string_lossy().into_owned(),
        };
        let mut reg = Registry::load(&p).unwrap();
        reg.upsert(meta);
        reg.save(&p).unwrap();

        let report = set_scope(&p, "unmanaged/gskill", Scope::Local).unwrap();
        assert!(report.affected_profiles.is_empty());
        // canonical 迁移到池子，原全局位置清空
        let pool_canon = p.skillkit_skills_dir().join(name);
        assert!(pool_canon.exists(), "canonical 迁到池子");
        assert!(pool_canon.join("SKILL.md").exists(), "内容随迁移");
        assert!(!agents_canon.exists(), "原 ~/.agents/skills/<name> 已迁走");
        // registry canonical_path 更新 + scope=local
        let m2 = Registry::load(&p)
            .unwrap()
            .get("unmanaged/gskill")
            .unwrap()
            .clone();
        assert_eq!(m2.scope, Scope::Local);
        assert_eq!(m2.canonical_path, pool_canon.to_string_lossy());
    }

    #[test]
    fn global_to_local_finds_real_canonical_even_if_registry_drifts() {
        // docx 类场景：registry canonical_path 漂移到 ~/.agents/skills/<name>（不存在），
        // 实际物理在 ~/.claude/skills/<name>（真实目录）。set_scope 扫物理位置找真实 canonical 迁移。
        let p = paths();
        let name = "drift";
        let claude_canon = p.claude_skills_dir().join(name);
        std::fs::create_dir_all(&claude_canon).unwrap();
        std::fs::write(claude_canon.join("SKILL.md"), "x").unwrap();
        let meta = SkillMeta {
            id: "unmanaged/drift".into(),
            name: name.into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "t".into(),
            canonical_path: p
                .agents_skills_dir()
                .join(name)
                .to_string_lossy()
                .into_owned(),
        };
        let mut reg = Registry::load(&p).unwrap();
        reg.upsert(meta);
        reg.save(&p).unwrap();

        let report = set_scope(&p, "unmanaged/drift", Scope::Local).unwrap();
        assert!(report.affected_profiles.is_empty());
        let pool_canon = p.skillkit_skills_dir().join(name);
        assert!(
            pool_canon.exists(),
            "物理迁到池子（即使 registry canonical 漂移）"
        );
        assert!(pool_canon.join("SKILL.md").exists());
        assert!(!claude_canon.exists(), "原 claude 位置已迁走");
        let m2 = Registry::load(&p)
            .unwrap()
            .get("unmanaged/drift")
            .unwrap()
            .clone();
        assert_eq!(m2.scope, Scope::Local);
        assert_eq!(m2.canonical_path, pool_canon.to_string_lossy());
    }

    #[test]
    fn global_to_local_dedupes_when_pool_already_has_canonical() {
        // 池子已有 <name>（旧 managed 残留）+ 全局位置也有 <name>（重复）→ 删全局重复，canonical 用池子
        let p = paths();
        let name = "dup";
        let pool = p.skillkit_skills_dir().join(name);
        std::fs::create_dir_all(&pool).unwrap();
        std::fs::write(pool.join("SKILL.md"), "x").unwrap();
        let claude = p.claude_skills_dir().join(name);
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("SKILL.md"), "x").unwrap();
        let meta = SkillMeta {
            id: "unmanaged/dup".into(),
            name: name.into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "t".into(),
            canonical_path: claude.to_string_lossy().into_owned(),
        };
        let mut reg = Registry::load(&p).unwrap();
        reg.upsert(meta);
        reg.save(&p).unwrap();

        set_scope(&p, "unmanaged/dup", Scope::Local).unwrap();
        assert!(pool.exists(), "池子 canonical 保留");
        assert!(!claude.exists(), "全局重复已删");
        let m2 = Registry::load(&p)
            .unwrap()
            .get("unmanaged/dup")
            .unwrap()
            .clone();
        assert_eq!(m2.scope, Scope::Local);
        assert_eq!(m2.canonical_path, pool.to_string_lossy());
    }

    /// 回归：rescope 的 registry 写入曾被并发写方（import/upgrade 的长 load→save 窗口）
    /// 用旧快照覆盖回滚——GUI 表现为「点 →local 无声失效」。锁化后两方串行，写入并存。
    #[test]
    fn rescope_survives_concurrent_registry_writer() {
        let p = paths();
        seed_skill(&p, "dc/g", Scope::Global);
        seed_skill(&p, "dc/other", Scope::Local);

        // writer 模拟锁化的并发写方（import 每 adopt 一对持锁 load→save_raw）
        let wpaths = p.clone();
        let writer = std::thread::spawn(move || {
            for i in 0..50 {
                let _lock = crate::lock::FileLock::acquire(&wpaths, "registry").unwrap();
                let mut reg = Registry::load(&wpaths).unwrap();
                if let Some(m) = reg.skills.get_mut("dc/other") {
                    m.version = Some(format!("v{i}"));
                }
                reg.save_raw(&wpaths).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });
        // 等 writer 跑进循环中途再 rescope，制造真实的窗口交叠
        std::thread::sleep(std::time::Duration::from_millis(30));
        set_scope(&p, "dc/g", Scope::Local).unwrap();
        writer.join().unwrap();

        let reg = Registry::load(&p).unwrap();
        assert_eq!(
            reg.get("dc/g").unwrap().scope,
            Scope::Local,
            "rescope 写入不被并发写方覆盖回滚"
        );
        assert_eq!(
            reg.get("dc/other").unwrap().version.as_deref(),
            Some("v49"),
            "并发写方的写入不丢"
        );
    }
}
