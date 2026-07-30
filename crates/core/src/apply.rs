//! apply：让项目 <agent>/skills/ 下 skillkit 管的 local skill 与 installed_skills 一致。
//! 本模块含 diff 计算（纯逻辑，status 与 apply 共用）+ 落地执行（Task 7-8）。
use crate::error::Result;
use crate::project::Project;
use crate::registry::{Registry, Scope};
use serde::{Deserialize, Serialize};

/// 一个 skill 在某 agent 下的落地目标（Task 7 落地用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTarget {
    pub skill_id: String,
    pub agent: String,
    pub canonical_path: String,
    pub commit_sha: String,
}

/// apply 内部 diff：expected（应落地的 local target）+ conflicts（sha 漂移的 skill）。
/// missing/extra 不在此（需结合现状扫描，由 build_status 算到 StatusView）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDiff {
    pub expected: Vec<LocalTarget>,
    pub conflicts: Vec<String>,
}

/// 计算 diff：expected = installed_skills 中 local scope 的 skill × agents；
/// conflicts = locked_shas 与 registry.commit_sha 不一致（sha 漂移）。
pub fn compute_diff(project: &Project, registry: &Registry) -> Result<ApplyDiff> {
    let mut expected = Vec::new();
    let mut conflicts = Vec::new();
    for id in &project.installed_skills {
        let Ok(meta) = registry.get(id) else {
            continue; // 未安装：apply 时报错引导 install，diff 阶段跳过
        };
        if meta.scope != Scope::Local {
            continue; // global 不 per-project 落地
        }
        let sha = meta.commit_sha.clone().unwrap_or_default();
        if let Some(locked) = project.locked_shas.get(id) {
            if locked != &sha {
                conflicts.push(id.clone());
            }
        }
        let canonical = meta.canonical_path.clone();
        for agent in &project.agents {
            expected.push(LocalTarget {
                skill_id: id.clone(),
                agent: agent.clone(),
                canonical_path: canonical.clone(),
                commit_sha: sha.clone(),
            });
        }
    }
    Ok(ApplyDiff {
        expected,
        conflicts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use crate::registry::{Scope, SkillMeta};

    fn meta(id: &str, scope: Scope, sha: &str) -> SkillMeta {
        SkillMeta {
            id: id.into(),
            name: id.split('/').next_back().unwrap_or(id).into(),
            source: id.split('/').next().unwrap_or("").into(),
            scope,
            version: None,
            commit_sha: Some(sha.into()),
            installed_at: "2026-07-29T00:00:00Z".into(),
            canonical_path: format!("/canon/{}", id.split('/').next_back().unwrap_or(id)),
        }
    }

    fn proj(skills: &[&str], locked: &[(&str, &str)]) -> Project {
        Project {
            id: "TESTID".into(),
            name: "p".into(),
            path: "/tmp/p".into(),
            agents: vec!["claude-code".into()],
            applied_profiles: vec![],
            installed_skills: skills.iter().map(|s| (*s).to_owned()).collect(),
            locked_shas: locked
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn diff_expected_only_local_global_skipped() {
        let mut reg = Registry::default();
        reg.upsert(meta("dc/logseq", Scope::Local, "sha1"));
        reg.upsert(meta("dc/global", Scope::Global, "sha2"));
        let project = proj(&["dc/logseq", "dc/global"], &[]);
        let diff = compute_diff(&project, &reg).unwrap();
        assert_eq!(diff.expected.len(), 1, "global 不 per-project 落地");
        assert_eq!(diff.expected[0].skill_id, "dc/logseq");
        assert!(diff.conflicts.is_empty());
    }

    #[test]
    fn diff_conflicts_when_sha_drifted() {
        let mut reg = Registry::default();
        reg.upsert(meta("dc/logseq", Scope::Local, "new"));
        let project = proj(&["dc/logseq"], &[("dc/logseq", "old")]);
        let diff = compute_diff(&project, &reg).unwrap();
        assert_eq!(diff.conflicts, vec!["dc/logseq"]);
    }

    #[test]
    fn diff_skips_uninstalled() {
        let reg = Registry::default();
        let project = proj(&["dc/missing"], &[]);
        let diff = compute_diff(&project, &reg).unwrap();
        assert!(diff.expected.is_empty());
    }
}
