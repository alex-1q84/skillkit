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

/// --all 升级结果：upgraded 成功的 + blocked 冲突未升的。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpgradeAllReport {
    pub upgraded: Vec<UpgradeReport>,
    /// 冲突被拦截的 skill（id + 受影响项目），未升级。
    pub blocked: Vec<UpgradeBlockedInfo>,
}

/// 单个被冲突拦截的 skill。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpgradeBlockedInfo {
    pub id: String,
    /// 锁了当前 hash、升级后版本基线会漂移的项目。
    pub affected: Vec<String>,
}

/// 升级全部 registry skill。
///
/// - 成功的进 `upgraded`
/// - 冲突（UpgradeBlocked）的进 `blocked`，不升级也不中断
/// - unmanaged / 未安装 / 下载失败等其余错误 warn 后跳过（--all 的预期语义，不列出）
/// - 任一单点失败都不中断整个批量升级
pub fn upgrade_all(paths: &Paths, yes: bool) -> Result<UpgradeAllReport> {
    let reg = Registry::load(paths)?;
    let ids: Vec<String> = reg.skills.keys().cloned().collect();
    let mut upgraded = Vec::new();
    let mut blocked = Vec::new();
    for id in ids {
        match upgrade_skill(paths, &id, yes) {
            Ok(r) => upgraded.push(r),
            Err(SkillkitError::UpgradeBlocked { id, affected }) => {
                blocked.push(UpgradeBlockedInfo { id, affected });
            }
            Err(e) => tracing::warn!(error = ?e, "upgrade 跳过 {id}"),
        }
    }
    Ok(UpgradeAllReport { upgraded, blocked })
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

    #[test]
    fn upgrade_all_collects_blocked_and_skips_unmanaged() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 拦截真实 npx：`skills@latest update <skill> -y` 往 cwd 写 skills-lock.json，
        // 让 dc/ok 的升级路径在本机/无网环境下也能闭合（其余分支在 npx 之前就返回）。
        install_fake_npx(&paths);
        // managed 且无人锁定 → 正常升级
        install_managed(&paths, "dc/ok", "hashA");
        // managed 但 P1 锁当前 hash → 冲突拦截
        install_managed(&paths, "dc/conflict", "hashB");
        save_project(&paths, "P1", &[("dc/conflict", "hashB")]);
        save_project(&paths, "P2", &[("dc/conflict", "other")]);
        // unmanaged → 无版本锁，warn 跳过
        install_unmanaged(&paths, "unm");

        let all = upgrade_all(&paths, false).unwrap();
        assert_eq!(all.upgraded.len(), 1, "只有 dc/ok 成功升级");
        assert_eq!(all.upgraded[0].id, "dc/ok");
        assert_eq!(all.blocked.len(), 1, "dc/conflict 被拦截且列出");
        assert_eq!(all.blocked[0].id, "dc/conflict");
        assert_eq!(all.blocked[0].affected, vec!["P1".to_string()]);
        assert!(
            !all.upgraded.iter().any(|r| r.id == "unmanaged/unm"),
            "unmanaged 跳过，不进 upgraded"
        );
        assert!(
            !all.blocked.iter().any(|b| b.id == "unmanaged/unm"),
            "unmanaged 跳过，不进 blocked"
        );
        // 升级落库：registry 已换成 fake npx 写回的新 hash
        let reg = Registry::load(&paths).unwrap();
        assert_eq!(
            reg.get("dc/ok").unwrap().computed_hash.as_deref(),
            Some("hashnew")
        );
    }

    /// 往 PATH 前插一个假 npx：只响应 `skills@latest update <skill> -y`，
    /// 在 cwd（~/.skillkit/）写 skills-lock.json，返回 upgrade 后的新 hash。
    fn install_fake_npx(paths: &Paths) {
        let bin = paths.skillkit_dir().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let sh = bin.join("npx");
        std::fs::write(
            &sh,
            "#!/bin/sh\n\
             if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"update\" ]; then\n\
             \x20 printf '{\"skills\": {\"%s\": {\"computedHash\": \"hashnew\"}}}' \"$3\" > skills-lock.json\n\
             \x20 exit 0\n\
             fi\n\
             exit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", bin.display(), path));
    }
}
