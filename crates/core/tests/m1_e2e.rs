//! M1 端到端：install local → profile → project → apply-profile → apply 落地 → status
//! → 幂等 → extra 清理。含 global 不 per-project + --json schema 锁定。
use skillkit_core::{
    apply::{build_status, run_apply},
    paths::Paths,
    profile::Profile,
    project::Project,
    registry::Registry,
    Scope,
};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn install_local_bare(paths: &Paths, id: &str) {
    let skill = id.split('/').next_back().unwrap_or(id);
    let canon = paths.skillkit_skills_dir().join(skill);
    std::fs::create_dir_all(&canon).unwrap();
    std::fs::write(canon.join("SKILL.md"), "# local\n").unwrap();
    let mut reg = Registry::load(paths).unwrap();
    reg.upsert(skillkit_core::registry::SkillMeta {
        id: id.into(),
        name: skill.into(),
        source: id.split('/').next().unwrap_or("").into(),
        scope: Scope::Local,
        version: None,
        computed_hash: Some("sha1".into()),
        installed_at: "2026-07-29T00:00:00Z".into(),
        canonical_path: canon.to_string_lossy().into_owned(),
    });
    reg.save(paths).unwrap();
}

#[test]
fn m1_full_flow_apply_and_status() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(project_root.join(".git/info")).unwrap();

    install_local_bare(&paths, "dc/logseq");
    install_local_bare(&paths, "dc/dataviz");

    let mut profile = Profile {
        name: "frontend".into(),
        description: String::new(),
        skills: vec![],
    };
    profile.add_skill("dc/logseq").unwrap();
    profile.add_skill("dc/dataviz").unwrap();
    profile.save(&paths).unwrap();

    let mut proj = Project {
        id: "E2E1".into(),
        name: "proj".into(),
        path: project_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: BTreeMap::new(),
    };
    proj.apply_profile("frontend", &profile.skills);
    proj.save(&paths).unwrap();

    // status（apply 前 missing=2）
    let reg = Registry::load(&paths).unwrap();
    let diff = skillkit_core::apply::compute_diff(&proj, &reg).unwrap();
    let st = build_status(&paths, &proj, &diff).unwrap();
    assert_eq!(st.missing.len(), 2);

    // apply 落地
    let report = run_apply(&paths, &mut proj, false).unwrap();
    assert_eq!(report.created.len(), 2);
    assert!(project_root.join(".claude/skills/logseq").is_symlink());
    assert!(project_root.join(".claude/skills/dataviz").is_symlink());

    // 幂等
    let r2 = run_apply(&paths, &mut proj, false).unwrap();
    assert!(r2.created.is_empty(), "重复 apply 零 created");

    // extra 清理
    proj.remove_skill("dc/dataviz").unwrap();
    let r3 = run_apply(&paths, &mut proj, false).unwrap();
    assert!(r3.removed.iter().any(|r| r.contains("dataviz")));
    assert!(!project_root.join(".claude/skills/dataviz").exists());

    // exclude 维护
    let excl = std::fs::read_to_string(project_root.join(".git/info/exclude")).unwrap();
    assert!(excl.contains(".claude/skills/logseq"));
}

#[test]
fn m1_global_skill_not_per_project() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let project_root = tmp.path().join("proj");
    std::fs::create_dir_all(project_root.join(".git/info")).unwrap();
    // global canonical（池子）
    let canon = paths.skillkit_skills_dir().join("gskill");
    std::fs::create_dir_all(&canon).unwrap();
    std::fs::write(canon.join("SKILL.md"), "g").unwrap();
    let mut reg = Registry::load(&paths).unwrap();
    reg.upsert(skillkit_core::registry::SkillMeta {
        id: "dc/g".into(),
        name: "gskill".into(),
        source: "dc".into(),
        scope: Scope::Global,
        version: None,
        computed_hash: Some("sha".into()),
        installed_at: "2026-07-29T00:00:00Z".into(),
        canonical_path: canon.to_string_lossy().into_owned(),
    });
    reg.save(&paths).unwrap();
    let mut proj = Project {
        id: "E2E2".into(),
        name: "proj".into(),
        path: project_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec!["dc/g".into()],
        locked_shas: BTreeMap::new(),
    };
    run_apply(&paths, &mut proj, false).unwrap();
    // global 不在项目落地，但 Claude 全局 symlink 在位
    assert!(
        !project_root.join(".claude/skills/gskill").exists(),
        "global 不 per-project 落地"
    );
    assert!(
        paths.claude_skills_dir().join("gskill").is_symlink(),
        "global 的 Claude symlink 在位"
    );
}

#[test]
fn json_schema_status_and_report_stable() {
    // ApplyReport schema：created/removed/recopied/warnings（Vec<String>），AI agent 依赖稳定
    let report = skillkit_core::apply::ApplyReport {
        created: vec!["a".into()],
        removed: vec![],
        recopied: vec![],
        warnings: vec!["w".into()],
    };
    let json = serde_json::to_value(&report).unwrap();
    let obj = json.as_object().unwrap();
    assert!(
        obj.contains_key("created")
            && obj.contains_key("removed")
            && obj.contains_key("recopied")
            && obj.contains_key("warnings"),
        "ApplyReport --json schema 必须稳定含这四个字段"
    );
}
