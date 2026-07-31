//! M3 端到端（#[ignore] 真跑 npx skills）：
//! import 溯源重装（local fixture 带 .git）+ upgrade 走 npx update 更新 hash。
use skillkit_core::{
    install,
    paths::Paths,
    registry::Registry,
    source::{Source, SourcesStore},
    upgrade_skill, Scope,
};
use std::process::Command;
use tempfile::tempdir;

/// 建带 .git 的本地 fixture 仓库（含 <skill>/SKILL.md），返回仓库根。
/// git 操作本地 bare repo 真跑（CLAUDE.md §8：不 mock）。
fn git_fixture(dir: &std::path::Path, skill: &str) -> std::path::PathBuf {
    let repo = dir.join("skill-repo");
    let skill_dir = repo.join(skill);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {skill}\ndescription: m3 fixture\n---\n# {skill}\n"),
    )
    .unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "m3@test"])
        .current_dir(&repo)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "m3"])
        .current_dir(&repo)
        .status()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&repo)
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(&repo)
        .status()
        .unwrap();
    repo
}

#[test]
#[ignore = "install/upgrade 委托 npx skills，需本地 Node；cargo test -- --ignored 手动跑"]
fn upgrade_updates_registry_hash() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let repo = git_fixture(tmp.path(), "m3-demo");
    let pkg = repo.to_string_lossy().into_owned();

    let mut store = SourcesStore::default();
    store
        .add(Source {
            name: "m3-src".into(),
            package: Some(pkg.clone()),
        })
        .unwrap();
    store.save(&paths).unwrap();

    let meta = install(&paths, "m3-src", "m3-demo", &pkg, Scope::Local).unwrap();
    let old = meta.computed_hash.unwrap();

    let report = upgrade_skill(&paths, "m3-src/m3-demo", true).unwrap();
    assert_eq!(report.old_hash, old);
    assert!(!report.new_hash.is_empty());
    // registry 已更新为新 hash
    let reg = Registry::load(&paths).unwrap();
    assert_eq!(
        reg.get("m3-src/m3-demo").unwrap().computed_hash.as_deref(),
        Some(report.new_hash.as_str())
    );
}
