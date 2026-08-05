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
pub fn set_scope(paths: &Paths, id: &str, target: Scope) -> Result<RescopeReport> {
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
            reg.save(paths)?;
            // 清 profile/project 引用（跨多文件，非原子——失败给可恢复文案，见 spec §6）
            let (ap, aproj) = remove_refs(paths, id)?;
            Ok(RescopeReport {
                affected_profiles: ap,
                affected_projects: aproj,
            })
        }
        (Scope::Global, Scope::Local) => {
            // remove 不加守卫（meta.scope 仍 Global，安全删链）→ 改 scope=Local → save
            crate::symlink::remove_global_claude(paths, &meta)?;
            meta.scope = Scope::Local;
            reg.upsert(meta.clone());
            reg.save(paths)?;
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
}
