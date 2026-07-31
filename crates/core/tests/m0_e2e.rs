//! M0 端到端：source 注册（含默认源种子）+ install（委托 npx skills，local fixture 真跑）
//! → registry + canonical 池子 + global 双层 symlink + 幂等。
//! install 走外部 npx skills（需本地 Node），标 #[ignore]：日常 make check 跳过，
//! 本地 `cargo test -p skillkit-core m0 -- --ignored` 手动跑。
use skillkit_core::{
    ensure_global_claude, install,
    paths::Paths,
    registry::Registry,
    source::{Source, SourcesStore},
    Scope,
};
use tempfile::tempdir;

/// 建本地 skill fixture 目录（含 <skill>/SKILL.md），返回其根。npx skills add <root> -s <skill>。
fn local_fixture(dir: &std::path::Path, skill: &str) -> std::path::PathBuf {
    let skill_dir = dir.join(skill);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {skill}\ndescription: e2e fixture\n---\n# {skill}\n"),
    )
    .unwrap();
    dir.to_path_buf()
}

#[test]
fn source_default_seed_and_add() {
    // 纯逻辑（不调 npx）：默认源种子 + 固定 package 源注册
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());

    SourcesStore::ensure_default(&paths).unwrap();
    let store = SourcesStore::load(&paths).unwrap();
    assert_eq!(store.list()[0].name, "skills.sh");
    assert!(
        store.list()[0].package.is_none(),
        "skills.sh 是 registry 搜索入口"
    );

    let mut store = SourcesStore::load(&paths).unwrap();
    store
        .add(Source {
            name: "team".into(),
            package: Some("git@github.com:org/team.git".into()),
        })
        .unwrap();
    store.save(&paths).unwrap();
    assert_eq!(SourcesStore::load(&paths).unwrap().list().len(), 2);
}

#[test]
#[ignore = "install 委托 npx skills，需本地 Node；cargo test -- --ignored 手动跑"]
fn m0_install_local_to_pool_and_global_symlink() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let fixture = local_fixture(tmp.path(), "demo-skill");
    let pkg = fixture.to_string_lossy().into_owned();

    let mut store = SourcesStore::default();
    store
        .add(Source {
            name: "local-src".into(),
            package: Some(pkg.clone()),
        })
        .unwrap();
    store.save(&paths).unwrap();

    let meta = install(&paths, "local-src", "demo-skill", &pkg, Scope::Global).unwrap();
    assert_eq!(meta.scope, Scope::Global);

    // canonical 池子落地（~/.skillkit/.agents/skills/demo-skill）
    assert!(paths
        .skillkit_skills_dir()
        .join("demo-skill")
        .join("SKILL.md")
        .exists());

    // registry 登记
    assert!(Registry::load(&paths)
        .unwrap()
        .get("local-src/demo-skill")
        .is_ok());

    // global 双层 symlink：agents 落地 + Claude 桥接
    assert!(paths.agents_skills_dir().join("demo-skill").is_symlink());
    assert!(paths.claude_skills_dir().join("demo-skill").is_symlink());

    // 幂等：再 ensure 不报错
    ensure_global_claude(&paths, &meta).unwrap();
    assert!(paths.claude_skills_dir().join("demo-skill").is_symlink());
}

#[test]
#[ignore = "install 委托 npx skills，需本地 Node"]
fn reinstall_same_skill_fails() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let fixture = local_fixture(tmp.path(), "dup");
    let pkg = fixture.to_string_lossy().into_owned();
    let mut store = SourcesStore::default();
    store
        .add(Source {
            name: "t".into(),
            package: Some(pkg.clone()),
        })
        .unwrap();
    store.save(&paths).unwrap();

    install(&paths, "t", "dup", &pkg, Scope::Global).unwrap();
    assert!(install(&paths, "t", "dup", &pkg, Scope::Global).is_err());
}
