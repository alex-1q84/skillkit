//! upgrade：升级已安装 skill（npx skills update），更新 registry.computed_hash。
//! 冲突检测：升级会让「锁了当前 hash」的项目从同步变漂移，yes=false 时返回 UpgradeBlocked。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::Registry;

/// 单次升级结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpgradeReport {
    pub id: String,
    /// 升级前 computed_hash。
    pub old_hash: String,
    /// 升级后 computed_hash。
    pub new_hash: String,
    /// 升级后从「同步」变「漂移」的项目 id（需重新 project apply）。
    pub affected_projects: Vec<String>,
}

pub fn upgrade_skill(paths: &Paths, id: &str, yes: bool) -> Result<UpgradeReport> {
    let mut reg = Registry::load(paths)?;
    let mut meta = reg.get(id)?.clone();
    let old_hash = meta
        .computed_hash
        .clone()
        .ok_or_else(|| SkillkitError::Tool {
            message: format!("{id} 是 unmanaged skill，无版本锁，无法升级"),
        })?;
    let affected = find_affected_projects(paths, id, &old_hash)?;
    if !affected.is_empty() && !yes {
        return Err(SkillkitError::UpgradeBlocked {
            id: id.to_string(),
            affected,
        });
    }
    crate::npx::update(paths, &meta.name)?;
    let new_hash = crate::npx::read_computed_hash(paths, &meta.name)?;
    meta.computed_hash = Some(new_hash.clone());
    meta.installed_at = crate::install::now_iso();
    reg.upsert(meta);
    reg.save(paths)?;
    Ok(UpgradeReport {
        id: id.to_string(),
        old_hash,
        new_hash,
        affected_projects: affected,
    })
}

/// 升级全部 registry skill。unmanaged / 未安装 / 冲突 / 下载失败的跳过并 warn，不中断。
pub fn upgrade_all(paths: &Paths, yes: bool) -> Result<Vec<UpgradeReport>> {
    let reg = Registry::load(paths)?;
    let ids: Vec<String> = reg.skills.keys().cloned().collect();
    let mut reports = Vec::new();
    for id in ids {
        match upgrade_skill(paths, &id, yes) {
            Ok(r) => reports.push(r),
            Err(e) => tracing::warn!(error = ?e, "upgrade 跳过 {id}"),
        }
    }
    Ok(reports)
}

/// 找「升级后受影响」的项目：locked_shas[id] == old_hash（当前同步，升级后会漂移）。
fn find_affected_projects(paths: &Paths, id: &str, old_hash: &str) -> Result<Vec<String>> {
    let mut affected = Vec::new();
    for pid in crate::project::list_ids(paths)? {
        let p = crate::project::Project::load(paths, &pid)?;
        if p.locked_shas.get(id).map(String::as_str) == Some(old_hash) {
            affected.push(pid);
        }
    }
    affected.sort();
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use crate::registry::{Scope, SkillMeta};
    use tempfile::tempdir;

    fn install_managed(paths: &Paths, id: &str, hash: &str) {
        let skill = id.split('/').next_back().unwrap_or(id);
        let canon = paths.skillkit_skills_dir().join(skill);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(SkillMeta {
            id: id.into(),
            name: skill.into(),
            source: id.split('/').next().unwrap_or("").into(),
            scope: Scope::Local,
            version: None,
            computed_hash: Some(hash.into()),
            installed_at: "2026-07-31T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(paths).unwrap();
    }

    fn install_unmanaged(paths: &Paths, name: &str) {
        let canon = paths.agents_skills_dir().join(name);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(SkillMeta {
            id: format!("unmanaged/{name}"),
            name: name.into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "2026-07-31T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(paths).unwrap();
    }

    fn save_project(paths: &Paths, id: &str, locked: &[(&str, &str)]) {
        let proj = Project {
            id: id.into(),
            name: id.into(),
            path: "/tmp/p".into(),
            agents: vec![],
            applied_profiles: vec![],
            installed_skills: vec![],
            locked_shas: locked
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        proj.save(paths).unwrap();
    }

    #[test]
    fn upgrade_unknown_skill_errors() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let err = upgrade_skill(&paths, "nope/x", true).unwrap_err();
        assert!(matches!(err, SkillkitError::SkillNotInstalled { .. }));
    }

    #[test]
    fn upgrade_unmanaged_skips() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        install_unmanaged(&paths, "foo");
        let err = upgrade_skill(&paths, "unmanaged/foo", true).unwrap_err();
        assert!(matches!(err, SkillkitError::Tool { .. }));
    }

    #[test]
    fn upgrade_blocked_when_project_locked_and_no_yes() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        install_managed(&paths, "dc/foo", "oldhash");
        save_project(&paths, "P1", &[("dc/foo", "oldhash")]); // 锁当前 hash → 升级会漂移
        save_project(&paths, "P2", &[("dc/foo", "other")]); // 锁别的 → 不受影响

        let err = upgrade_skill(&paths, "dc/foo", false).unwrap_err();
        match err {
            SkillkitError::UpgradeBlocked { id, affected } => {
                assert_eq!(id, "dc/foo");
                assert_eq!(affected, vec!["P1".to_string()]);
            }
            other => panic!("expected UpgradeBlocked, got {other:?}"),
        }
    }

    #[test]
    fn find_affected_matches_only_locked_current_hash() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        save_project(&paths, "P1", &[("dc/foo", "oldhash")]);
        save_project(&paths, "P2", &[("dc/bar", "other")]);
        save_project(&paths, "P3", &[("dc/foo", "other")]);
        let affected = find_affected_projects(&paths, "dc/foo", "oldhash").unwrap();
        assert_eq!(affected, vec!["P1".to_string()]);
    }
}
