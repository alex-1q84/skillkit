//! apply：让项目 <agent>/skills/ 下 skillkit 管的 local skill 与 installed_skills 一致。
//! 本模块含 diff 计算（纯逻辑，status 与 apply 共用）+ 落地执行（Task 7-8）。
use crate::config::Config;
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::project::Project;
use crate::registry::{Registry, Scope};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 一个 skill 在某 agent 下的落地目标（Task 7 落地用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTarget {
    pub skill_id: String,
    pub agent: String,
    pub canonical_path: String,
    pub computed_hash: String,
}

/// apply 内部 diff：expected（应落地的 local target）+ conflicts（sha 漂移的 skill）。
/// missing/extra 不在此（需结合现状扫描，由 build_status 算到 StatusView）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDiff {
    pub expected: Vec<LocalTarget>,
    pub conflicts: Vec<String>,
}

/// 计算 diff：expected = installed_skills 中 local scope 的 skill × agents；
/// conflicts = locked_shas 与 registry.computed_hash 不一致（sha 漂移）。
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
        let sha = meta.computed_hash.clone().unwrap_or_default();
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
                computed_hash: sha.clone(),
            });
        }
    }
    Ok(ApplyDiff {
        expected,
        conflicts,
    })
}

const EXCLUDE_BEGIN: &str = "# >>> skillkit managed >>>";
const EXCLUDE_END: &str = "# <<< skillkit managed <<<";

/// 落地动作记录（apply 输出用）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplyReport {
    pub created: Vec<String>,
    pub removed: Vec<String>,
    pub recopied: Vec<String>,
    pub warnings: Vec<String>,
}

/// agent name → 项目内 skills 目录名（Claude Code 的目录是 .claude，非 .claude-code）。
fn agent_dir_name(agent: &str) -> &str {
    match agent {
        "claude-code" => "claude",
        other => other,
    }
}

/// 对一个 local target 落地（按 agent 能力 symlink 或 copy）。返回 (created, recopied)。
fn land_one(
    project_root: &Path,
    target: &LocalTarget,
    supports_symlink: bool,
) -> Result<(bool, bool)> {
    let skill = target
        .skill_id
        .split('/')
        .next_back()
        .unwrap_or(&target.skill_id);
    let dir_name = agent_dir_name(&target.agent);
    let agent_skills = project_root.join(format!(".{dir_name}/skills"));
    std::fs::create_dir_all(&agent_skills)?;
    let dest = agent_skills.join(skill);
    let canonical = Path::new(&target.canonical_path);

    if dest.exists()
        && !dest.is_symlink()
        && std::fs::metadata(&dest)
            .map(|m| m.is_dir())
            .unwrap_or(false)
    {
        // 真实目录占位：copy 模式判断副本是否过期，symlink 模式疑似 shared 报错
        if !supports_symlink {
            let sha_file = dest.join(".skillkit-sha");
            let current = std::fs::read_to_string(&sha_file).unwrap_or_default();
            if current == target.computed_hash {
                return Ok((false, false)); // 副本未过期，幂等跳过
            }
            std::fs::remove_dir_all(&dest)?;
            copy_dir_all(canonical, &dest)?;
            std::fs::write(sha_file, &target.computed_hash)?;
            return Ok((false, true)); // 过期重 copy
        }
        return Err(SkillkitError::Tool {
            message: format!(
                "{} 已存在且非 symlink，疑似 shared，跳过 local 落地",
                dest.display()
            ),
        });
    }

    if supports_symlink {
        if let Ok(existing) = std::fs::read_link(&dest) {
            if existing.as_path() == canonical {
                return Ok((false, false)); // 幂等
            }
            std::fs::remove_file(&dest)?; // 指向错误，删旧重建
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(canonical, &dest).map_err(|e| SkillkitError::Tool {
            message: format!("symlink 失败：{e}"),
        })?;
        Ok((true, false))
    } else {
        copy_dir_all(canonical, &dest)?;
        std::fs::write(dest.join(".skillkit-sha"), &target.computed_hash)?;
        Ok((true, false))
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    let status = std::process::Command::new("cp")
        .arg("-R")
        .arg(src)
        .arg(dst)
        .status()
        .map_err(|e| SkillkitError::Tool {
            message: e.to_string(),
        })?;
    if !status.success() {
        return Err(SkillkitError::Tool {
            message: format!("复制失败：{}", src.display()),
        });
    }
    Ok(())
}

/// 重写 <project>/.git/info/exclude 的 skillkit 段，列入当前 local 落地清单。
pub fn write_exclude(project_root: &Path, targets: &[LocalTarget]) -> Result<()> {
    let exclude = project_root.join(".git/info/exclude");
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut lines: Vec<String> = Vec::new();
    if let Ok(content) = std::fs::read_to_string(&exclude) {
        let mut in_block = false;
        for line in content.lines() {
            if line == EXCLUDE_BEGIN {
                in_block = true;
                continue;
            }
            if line == EXCLUDE_END {
                in_block = false;
                continue;
            }
            if !in_block {
                lines.push(line.into());
            }
        }
    }
    lines.push(EXCLUDE_BEGIN.into());
    for t in targets {
        let skill = t.skill_id.split('/').next_back().unwrap_or(&t.skill_id);
        lines.push(format!(".{}/skills/{}", agent_dir_name(&t.agent), skill));
    }
    lines.push(EXCLUDE_END.into());
    crate::error::atomic_write(&exclude, &lines.join("\n"))?;
    Ok(())
}

/// 扫描 <project>/<agent>/skills/ 下 skillkit 管的 local 落地点。
fn scan_local_landed(project_root: &Path, agent: &str, skm_skills: &Path) -> Result<Vec<String>> {
    let dir = project_root.join(format!(".{}/skills", agent_dir_name(agent)));
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if p.is_symlink() {
            if let Ok(t) = std::fs::read_link(&p) {
                if t.starts_with(skm_skills) {
                    found.push(name);
                }
            }
        } else if p.is_dir() && p.join(".skillkit-sha").exists() {
            found.push(name);
        }
    }
    Ok(found)
}

/// 扫描项目 agents 的 skills 目录下 shared skill（真实目录，非 skillkit 管的 local）。
/// shared 由项目 git 管理，skillkit 只读展示，不安装/升级/卸载。
pub fn scan_shared(project_root: &Path, agents: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for agent in agents {
        collect_shared(project_root, agent_dir_name(agent), agent, &mut found);
    }
    // 项目级 .agents/skills：跨 agent 共享池（cursor/codex 直读），与 proj.agents 声明无关
    collect_shared(project_root, "agents", "agents", &mut found);
    found
}

/// 扫 `.<dir>/skills` 下真实目录（非 symlink、无 .skillkit-sha local 标记），按 `label/<name>` 推入 found。
fn collect_shared(project_root: &Path, dir: &str, label: &str, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(project_root.join(format!(".{dir}/skills"))) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && !p.is_symlink() && !p.join(".skillkit-sha").exists() {
            let name = entry.file_name().to_string_lossy().into_owned();
            found.push(format!("{label}/{name}"));
        }
    }
}

/// apply 主流程：global ensure + local 落地 + extra 清理 + locked_shas 更新 + --frozen 冲突。
pub fn run_apply(paths: &Paths, project: &mut Project, frozen: bool) -> Result<ApplyReport> {
    let registry = Registry::load(paths)?;
    let config = Config::load(paths)?;
    let diff = compute_diff(project, &registry)?;
    let project_root = Path::new(&project.path);

    for id in &project.installed_skills {
        if registry.get(id).is_err() {
            return Err(SkillkitError::SkillNotInstalled { id: id.clone() });
        }
    }
    if frozen && !diff.conflicts.is_empty() {
        return Err(SkillkitError::Tool {
            message: format!("--frozen：版本冲突 {}", diff.conflicts.join(", ")),
        });
    }

    let mut report = ApplyReport::default();

    // global skill：ensure canonical + Claude symlink 在位（不 per-project 落地）
    for id in &project.installed_skills {
        let meta = registry.get(id)?;
        if meta.scope == Scope::Global {
            crate::symlink::ensure_global_claude(paths, meta)?;
        }
    }

    // local 落地
    let skm_skills = paths.skillkit_skills_dir();
    for target in &diff.expected {
        let supports_symlink = config
            .find_agent(&target.agent)
            .is_some_and(|a| a.supports_symlink);
        let skill = target
            .skill_id
            .split('/')
            .next_back()
            .unwrap_or(&target.skill_id);
        match land_one(project_root, target, supports_symlink) {
            Ok((created, recopied)) => {
                if created {
                    report.created.push(format!("{}/{}", target.agent, skill));
                }
                if recopied {
                    report.recopied.push(format!("{}/{}", target.agent, skill));
                }
            }
            Err(e) => report
                .warnings
                .push(format!("{}/{skill}：{e}", target.agent)),
        }
    }

    // extra：清理现状 skillkit-local 不在 expected 的
    let expected_names: std::collections::HashSet<String> = diff
        .expected
        .iter()
        .map(|t| {
            format!(
                "{}/{}",
                t.agent,
                t.skill_id.split('/').next_back().unwrap_or(&t.skill_id)
            )
        })
        .collect();
    for agent in &project.agents {
        for name in scan_local_landed(project_root, agent, &skm_skills)? {
            let key = format!("{agent}/{name}");
            if !expected_names.contains(&key) {
                let p = project_root.join(format!(".{}/skills/{}", agent_dir_name(agent), name));
                let _ = std::fs::remove_file(&p).or_else(|_| std::fs::remove_dir_all(&p));
                report.removed.push(key);
            }
        }
    }

    write_exclude(project_root, &diff.expected)?;

    // 更新 locked_shas 为当前 canonical sha
    for target in &diff.expected {
        project
            .locked_shas
            .insert(target.skill_id.clone(), target.computed_hash.clone());
    }
    Ok(report)
}

/// status 输出：结合 diff.expected 与现状扫描，给具体 id 清单（供 agent 决策）。
#[derive(Debug, Clone, Serialize)]
pub struct StatusView {
    pub expected: Vec<String>,
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    pub conflicts: Vec<String>,
}

/// 计算 status：expected/missing（结合现状）/extra（现状多出）/conflicts。
pub fn build_status(paths: &Paths, project: &Project, diff: &ApplyDiff) -> Result<StatusView> {
    let project_root = Path::new(&project.path);
    let skm_skills = paths.skillkit_skills_dir();
    let mut expected = Vec::new();
    let mut missing = Vec::new();
    for t in &diff.expected {
        let skill = t.skill_id.split('/').next_back().unwrap_or(&t.skill_id);
        let key = format!("{}/{}", t.agent, skill);
        expected.push(key.clone());
        let dest = project_root.join(format!(".{}/skills/{}", agent_dir_name(&t.agent), skill));
        if !dest.exists() && !dest.is_symlink() {
            missing.push(key);
        }
    }
    let mut extra = Vec::new();
    for agent in &project.agents {
        for name in scan_local_landed(project_root, agent, &skm_skills)? {
            let key = format!("{agent}/{name}");
            if !expected.contains(&key) {
                extra.push(key);
            }
        }
    }
    Ok(StatusView {
        expected,
        missing,
        extra,
        conflicts: diff.conflicts.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use crate::registry::{Scope, SkillMeta};
    use std::collections::BTreeMap;

    fn meta(id: &str, scope: Scope, sha: &str) -> SkillMeta {
        SkillMeta {
            id: id.into(),
            name: id.split('/').next_back().unwrap_or(id).into(),
            source: id.split('/').next().unwrap_or("").into(),
            scope,
            version: None,
            computed_hash: Some(sha.into()),
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

    #[test]
    fn land_symlink_idempotent_and_exclude() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let project_root = home.join("proj");
        std::fs::create_dir_all(project_root.join(".git/info")).unwrap();
        let canon = home.join(".skillkit/.agents/skills/logseq");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let target = LocalTarget {
            skill_id: "dc/logseq".into(),
            agent: "claude-code".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
            computed_hash: "sha1".into(),
        };
        let (created, _) = land_one(&project_root, &target, true).unwrap();
        assert!(created);
        let link = project_root.join(".claude/skills/logseq");
        assert!(link.is_symlink());
        let (created2, _) = land_one(&project_root, &target, true).unwrap();
        assert!(!created2, "幂等：再 land 不重建");
        write_exclude(&project_root, &[target]).unwrap();
        let excl = std::fs::read_to_string(project_root.join(".git/info/exclude")).unwrap();
        assert!(excl.contains(".claude/skills/logseq"));
    }

    #[test]
    fn land_copy_with_sha_and_recopy_on_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let project_root = home.join("proj");
        std::fs::create_dir_all(&project_root).unwrap();
        let canon = home.join(".skillkit/.agents/skills/foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "v1").unwrap();
        let t1 = LocalTarget {
            skill_id: "dc/foo".into(),
            agent: "cursor".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
            computed_hash: "sha1".into(),
        };
        land_one(&project_root, &t1, false).unwrap();
        let dest = project_root.join(".cursor/skills/foo");
        assert!(dest.join("SKILL.md").exists());
        assert_eq!(
            std::fs::read_to_string(dest.join(".skillkit-sha")).unwrap(),
            "sha1"
        );
        std::fs::write(canon.join("SKILL.md"), "v2").unwrap();
        let t2 = LocalTarget {
            computed_hash: "sha2".into(),
            ..t1
        };
        let (_, recopied) = land_one(&project_root, &t2, false).unwrap();
        assert!(recopied, "sha 漂移应触发重 copy");
    }

    fn install_local_bare(paths: &Paths, id: &str, sha: &str) {
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
            computed_hash: Some(sha.into()),
            installed_at: "2026-07-29T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(paths).unwrap();
    }

    #[test]
    fn run_apply_lands_local_and_locks_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        install_local_bare(&paths, "dc/logseq", "sha1");
        let project_root = tmp.path().join("proj");
        std::fs::create_dir_all(project_root.join(".git/info")).unwrap();
        let mut proj = Project {
            id: "P1".into(),
            name: "proj".into(),
            path: project_root.to_string_lossy().into_owned(),
            agents: vec!["claude-code".into()],
            applied_profiles: vec![],
            installed_skills: vec!["dc/logseq".into()],
            locked_shas: BTreeMap::new(),
        };
        let report = run_apply(&paths, &mut proj, false).unwrap();
        assert_eq!(report.created, vec!["claude-code/logseq"]);
        assert!(project_root.join(".claude/skills/logseq").is_symlink());
        assert_eq!(proj.locked_shas.get("dc/logseq").unwrap(), "sha1");
        let report2 = run_apply(&paths, &mut proj, false).unwrap();
        assert!(report2.created.is_empty(), "幂等：再 apply 零 created");
    }

    #[test]
    fn run_apply_removes_extra() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        install_local_bare(&paths, "dc/keep", "sha1");
        install_local_bare(&paths, "dc/gone", "sha2");
        let project_root = tmp.path().join("proj");
        std::fs::create_dir_all(project_root.join(".git/info")).unwrap();
        let mut proj = Project {
            id: "P2".into(),
            name: "proj".into(),
            path: project_root.to_string_lossy().into_owned(),
            agents: vec!["claude-code".into()],
            applied_profiles: vec![],
            installed_skills: vec!["dc/keep".into(), "dc/gone".into()],
            locked_shas: BTreeMap::new(),
        };
        run_apply(&paths, &mut proj, false).unwrap();
        assert!(project_root.join(".claude/skills/gone").is_symlink());
        proj.remove_skill("dc/gone").unwrap();
        let report = run_apply(&paths, &mut proj, false).unwrap();
        assert!(report.removed.iter().any(|r| r.contains("gone")));
        assert!(!project_root.join(".claude/skills/gone").exists());
    }

    #[test]
    fn run_apply_frozen_conflict_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        install_local_bare(&paths, "dc/logseq", "new");
        let project_root = tmp.path().join("proj");
        std::fs::create_dir_all(project_root.join(".git/info")).unwrap();
        let mut proj = Project {
            id: "P3".into(),
            name: "proj".into(),
            path: project_root.to_string_lossy().into_owned(),
            agents: vec!["claude-code".into()],
            applied_profiles: vec![],
            installed_skills: vec!["dc/logseq".into()],
            locked_shas: [("dc/logseq".to_string(), "old".to_string())]
                .into_iter()
                .collect(),
        };
        let err = run_apply(&paths, &mut proj, true).unwrap_err();
        assert!(matches!(err, SkillkitError::Tool { .. }));
    }

    #[test]
    fn scan_shared_includes_project_agents_pool() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // 项目级 .agents/skills 是跨 agent 共享池（cursor/codex 直读），
        // 与 proj.agents 声明无关——即使项目只声明 claude-code 也应被发现
        std::fs::create_dir_all(root.join(".agents/skills/pool-skill")).unwrap();
        let found = scan_shared(root, &["claude-code".into()]);
        assert!(
            found.contains(&"agents/pool-skill".to_string()),
            "项目级 .agents/skills 共享池应被发现，got: {found:?}"
        );
    }

    #[test]
    fn scan_shared_finds_per_agent_dirs_and_skips_local() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // cursor shared：真实目录，应发现
        std::fs::create_dir_all(root.join(".cursor/skills/cursor-shared")).unwrap();
        // claude-code local：有 .skillkit-sha 标记（skillkit 管的 local），不应算 shared
        let local = root.join(".claude/skills/local-managed");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join(".skillkit-sha"), "sha").unwrap();
        let found = scan_shared(root, &["claude-code".into(), "cursor".into()]);
        assert!(
            found.contains(&"cursor/cursor-shared".to_string()),
            "cursor shared 应被发现，got: {found:?}"
        );
        assert!(
            !found.iter().any(|s| s.contains("local-managed")),
            "有 .skillkit-sha 标记的 local 不应算 shared，got: {found:?}"
        );
    }
}
