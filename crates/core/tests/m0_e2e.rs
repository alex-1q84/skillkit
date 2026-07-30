//! M0 端到端验收：add source → install git 源 → registry 记录 + canonical 落地 +
//! Claude symlink → 幂等。含 skills_dir 子目录平铺场景。用本地 bare repo 真跑 git。
use skillkit_core::{
    ensure_global_claude, install,
    paths::Paths,
    registry::Registry,
    source::{Source, SourceType, SourcesStore},
    Scope,
};
use std::process::Command;
use tempfile::tempdir;

/// 把 work 目录做成 bare repo（带 -c user，不依赖全局 git 配置）。
fn git_bare(work: &std::path::Path, bare: &std::path::Path) {
    Command::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "init",
            "--quiet",
        ])
        .current_dir(work)
        .status()
        .unwrap();
    Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "add", "."])
        .current_dir(work)
        .status()
        .unwrap();
    Command::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .current_dir(work)
        .status()
        .unwrap();
    Command::new("git")
        .args(["clone", "--bare", "--quiet"])
        .arg(work)
        .arg(bare)
        .status()
        .unwrap();
}

/// skill 直接在仓库根。
fn bare_repo(dir: &std::path::Path) -> std::path::PathBuf {
    let work = dir.join("work");
    let bare = dir.join("src.git");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("SKILL.md"), "# e2e\n").unwrap();
    git_bare(&work, &bare);
    bare
}

/// skill 在仓库的 skills/shared-skill/ 子目录下。
fn bare_repo_with_skills_dir(dir: &std::path::Path) -> std::path::PathBuf {
    let work = dir.join("work-sd");
    let bare = dir.join("src-sd.git");
    std::fs::create_dir_all(work.join("skills").join("shared-skill")).unwrap();
    std::fs::write(
        work.join("skills").join("shared-skill").join("SKILL.md"),
        "# e2e skills-dir\n",
    )
    .unwrap();
    git_bare(&work, &bare);
    bare
}

#[test]
fn m0_full_flow_global_install_and_symlink() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let bare = bare_repo(tmp.path());

    let mut store = SourcesStore::default();
    store
        .add(Source {
            name: "team".into(),
            source_type: SourceType::Git,
            url: Some(bare.to_string_lossy().into()),
            path: None,
            ref_: None,
            skills_dir: None,
        })
        .unwrap();
    store.save(&paths).unwrap();

    let meta = install(&paths, "team", "shared-skill", Scope::Global).unwrap();
    assert_eq!(meta.scope, Scope::Global);

    let reg = Registry::load(&paths).unwrap();
    assert!(reg.get("team/shared-skill").is_ok());

    assert!(paths
        .agents_skills_dir()
        .join("shared-skill")
        .join("SKILL.md")
        .exists());

    let link = paths.claude_skills_dir().join("shared-skill");
    assert!(link.is_symlink(), "Claude symlink 应已建立");

    // 幂等：再 ensure 一次不报错
    ensure_global_claude(&paths, &meta).unwrap();
    assert!(link.is_symlink());
}

#[test]
fn m0_full_flow_with_skills_dir_flattens_and_symlinks() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let bare = bare_repo_with_skills_dir(tmp.path());

    let mut store = SourcesStore::default();
    store
        .add(Source {
            name: "dc".into(),
            source_type: SourceType::Git,
            url: Some(bare.to_string_lossy().into()),
            path: None,
            ref_: None,
            skills_dir: Some("skills".into()),
        })
        .unwrap();
    store.save(&paths).unwrap();

    install(&paths, "dc", "shared-skill", Scope::Global).unwrap();
    // canonical 平铺：SKILL.md 直接在 shared-skill 下，无 skills/ 中间层
    assert!(paths
        .agents_skills_dir()
        .join("shared-skill")
        .join("SKILL.md")
        .exists());
    assert!(!paths
        .agents_skills_dir()
        .join("shared-skill")
        .join("skills")
        .exists());
    // Claude symlink 桥接
    assert!(
        paths.claude_skills_dir().join("shared-skill").is_symlink(),
        "skills_dir 模式下 global install 也应建 Claude symlink"
    );
    // registry 记录
    assert!(Registry::load(&paths)
        .unwrap()
        .get("dc/shared-skill")
        .is_ok());
}

#[test]
fn reinstall_same_skill_fails_cleanly() {
    let tmp = tempdir().unwrap();
    let paths = Paths::new(tmp.path().to_path_buf());
    let bare = bare_repo(tmp.path());
    let mut store = SourcesStore::default();
    store
        .add(Source {
            name: "t".into(),
            source_type: SourceType::Git,
            url: Some(bare.to_string_lossy().into()),
            path: None,
            ref_: None,
            skills_dir: None,
        })
        .unwrap();
    store.save(&paths).unwrap();

    install(&paths, "t", "dup", Scope::Global).unwrap();
    let second = install(&paths, "t", "dup", Scope::Global);
    assert!(second.is_err(), "重复安装同名 skill 应报错而非覆盖");
}
