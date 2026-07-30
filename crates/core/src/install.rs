//! install/uninstall：从源拉 skill 到 canonical，登记 registry。skills_dir 模式下
//! clone 到临时目录、取 `<skills_dir>/<skill>` 平铺到 canonical，保持单层可发现结构。
use crate::error::{Result, SkillkitError};
use crate::git;
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};
use crate::source::{Source, SourceType, SourcesStore};
use std::path::{Path, PathBuf};

/// 安装：拉取到 canonical（按 scope 决定位置），记 commit_sha，登记 registry。
pub fn install(
    paths: &Paths,
    source_name: &str,
    skill_name: &str,
    scope: Scope,
) -> Result<SkillMeta> {
    let store = SourcesStore::load(paths)?;
    let source = store.get(source_name)?.clone();

    let canonical_dir = match scope {
        Scope::Global => paths.agents_skills_dir(),
        Scope::Local => paths.skm_skills_dir(),
    };
    let target = canonical_dir.join(skill_name);
    if target.exists() {
        return Err(SkillkitError::SkillAlreadyInstalled {
            id: skill_name.to_string(),
        });
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let sha = match source.source_type {
        SourceType::Git | SourceType::SkillsSh => fetch_git(&source, skill_name, &target)?,
        SourceType::Local => fetch_local(&source, skill_name, &target)?,
    };

    let id = Registry::skill_id(source_name, skill_name);
    let meta = SkillMeta {
        id: id.clone(),
        name: skill_name.to_string(),
        source: source_name.to_string(),
        scope,
        version: None,
        commit_sha: Some(sha),
        installed_at: now_iso(),
        canonical_path: target.display().to_string(),
    };
    let mut reg = Registry::load(paths)?;
    reg.upsert(meta.clone());
    reg.save(paths)?;
    // global skill：install 即建 Claude symlink 桥接（local 留给 M1 的 project apply）
    crate::symlink::ensure_global_claude(paths, &meta)?;
    Ok(meta)
}

/// 卸载：删 canonical + registry 记录。
pub fn uninstall(paths: &Paths, id: &str) -> Result<()> {
    let mut reg = Registry::load(paths)?;
    let meta = reg.get(id)?.clone();
    let target = PathBuf::from(&meta.canonical_path);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|_| SkillkitError::CanonicalCreate(target.clone()))?;
    }
    reg.remove(id)?.save(paths)?;
    Ok(())
}

/// git 源：skills_dir=None 直接 clone 到 target；skills_dir=Some 先 clone 临时再取
/// `<skills_dir>/<skill>` 平铺到 target（删临时），避免 canonical 残留中间层。
fn fetch_git(source: &Source, skill_name: &str, target: &Path) -> Result<String> {
    let url = source.url.as_deref().ok_or_else(|| SkillkitError::Git {
        message: format!("源 {} 缺少 url", source.name),
    })?;
    match &source.skills_dir {
        None => git::clone(url, target, source.ref_.as_deref()),
        Some(dir) => {
            let tmp = std::env::temp_dir().join(format!(
                "skillkit-{}-{}",
                skill_name,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&tmp); // 清理上次异常残留
            let sha = git::clone(url, &tmp, source.ref_.as_deref())?;
            let src = tmp.join(dir).join(skill_name);
            let result = if src.exists() {
                copy_dir_all(&src, target)
            } else {
                Err(SkillkitError::Git {
                    message: format!("仓库 {url} 的 {dir}/{skill_name} 不存在"),
                })
            };
            let _ = std::fs::remove_dir_all(&tmp);
            result?;
            Ok(sha)
        }
    }
}

/// local 源：skills_dir=None 直接 copy path；skills_dir=Some 取 `path/<dir>/<skill>`。
fn fetch_local(source: &Source, skill_name: &str, target: &Path) -> Result<String> {
    let raw = source.path.as_deref().ok_or_else(|| SkillkitError::Git {
        message: format!("源 {} 缺少 path", source.name),
    })?;
    let root = PathBuf::from(expand_tilde(raw));
    let src = match &source.skills_dir {
        None => root,
        Some(dir) => root.join(dir).join(skill_name),
    };
    if !src.exists() {
        return Err(SkillkitError::Git {
            message: format!("local 源路径不存在：{}", src.display()),
        });
    }
    copy_dir_all(&src, target)?;
    // local 源若是 git repo 取 sha，否则 unmanaged
    Ok(git::rev_parse(&src).unwrap_or_else(|_| "unmanaged".to_string()))
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    let status = std::process::Command::new("cp")
        .arg("-R")
        .arg(src)
        .arg(dst)
        .status()
        .map_err(|e| SkillkitError::Git {
            message: e.to_string(),
        })?;
    if !status.success() {
        return Err(SkillkitError::Git {
            message: format!("复制失败：{} -> {}", src.display(), dst.display()),
        });
    }
    Ok(())
}

fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

/// 当前时间 ISO 字符串（UTC RFC3339）。
fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::registry::{Registry, Scope};
    use crate::source::{Source, SourceType, SourcesStore};
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

    /// skill 直接在仓库根（skills_dir=None 场景）。
    fn bare_repo(dir: &std::path::Path) -> std::path::PathBuf {
        let work = dir.join("work");
        let bare = dir.join("src.git");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("SKILL.md"), "# test skill\n").unwrap();
        git_bare(&work, &bare);
        bare
    }

    /// skill 在仓库的 skills/demo-skill/ 子目录下（skills_dir=Some("skills") 场景）。
    /// 仓库根放 README 证明 skills/ 是子目录、不是仓库根。
    fn bare_repo_with_skills_dir(dir: &std::path::Path) -> std::path::PathBuf {
        let work = dir.join("work-sd");
        let bare = dir.join("src-sd.git");
        std::fs::create_dir_all(work.join("skills").join("demo-skill")).unwrap();
        std::fs::write(
            work.join("skills").join("demo-skill").join("SKILL.md"),
            "# skills-dir skill\n",
        )
        .unwrap();
        std::fs::write(work.join("README.md"), "repo root\n").unwrap();
        git_bare(&work, &bare);
        bare
    }

    #[test]
    fn install_git_source_records_sha_and_canonical() {
        // skills_dir=None：skill 在仓库根，clone 到 canonical
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let bare = bare_repo(tmp.path());

        let mut store = SourcesStore::default();
        store
            .add(Source {
                name: "test-src".into(),
                source_type: SourceType::Git,
                url: Some(bare.to_string_lossy().into_owned()),
                path: None,
                ref_: None,
                skills_dir: None,
            })
            .unwrap();
        store.save(&paths).unwrap();

        let meta = install(&paths, "test-src", "demo-skill", Scope::Global).unwrap();
        assert_eq!(meta.name, "demo-skill");
        assert!(meta.commit_sha.as_deref().unwrap().len() >= 7);
        assert!(paths
            .agents_skills_dir()
            .join("demo-skill")
            .join("SKILL.md")
            .exists());

        let reg = Registry::load(&paths).unwrap();
        assert!(reg.get("test-src/demo-skill").is_ok());
    }

    #[test]
    fn install_git_source_with_skills_dir_flattens() {
        // skills_dir=Some("skills")：skill 在 skills/demo-skill，install 后 canonical 平铺
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let bare = bare_repo_with_skills_dir(tmp.path());

        let mut store = SourcesStore::default();
        store
            .add(Source {
                name: "dc".into(),
                source_type: SourceType::Git,
                url: Some(bare.to_string_lossy().into_owned()),
                path: None,
                ref_: None,
                skills_dir: Some("skills".into()),
            })
            .unwrap();
        store.save(&paths).unwrap();

        let meta = install(&paths, "dc", "demo-skill", Scope::Global).unwrap();
        assert_eq!(meta.name, "demo-skill");
        // canonical 平铺：SKILL.md 直接在 demo-skill 下
        let skill_md = paths
            .agents_skills_dir()
            .join("demo-skill")
            .join("SKILL.md");
        assert!(skill_md.exists(), "skills_dir 子目录应平铺到 canonical");
        // 不残留 skills/ 中间层
        assert!(
            !paths
                .agents_skills_dir()
                .join("demo-skill")
                .join("skills")
                .exists(),
            "canonical 不应残留 skills/ 中间层"
        );
        // 仓库根的非 skill 文件不应进 canonical
        assert!(
            !paths
                .agents_skills_dir()
                .join("demo-skill")
                .join("README.md")
                .exists(),
            "skills_dir 模式下仓库根的非 skill 文件不应进 canonical"
        );

        let reg = Registry::load(&paths).unwrap();
        assert!(reg.get("dc/demo-skill").is_ok());
    }

    #[test]
    fn uninstall_removes_canonical_and_registry() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let bare = bare_repo(tmp.path());
        let mut store = SourcesStore::default();
        store
            .add(Source {
                name: "s".into(),
                source_type: SourceType::Git,
                url: Some(bare.to_string_lossy().into_owned()),
                path: None,
                ref_: None,
                skills_dir: None,
            })
            .unwrap();
        store.save(&paths).unwrap();
        install(&paths, "s", "sk", Scope::Global).unwrap();

        uninstall(&paths, "s/sk").unwrap();
        assert!(!paths.agents_skills_dir().join("sk").exists());
        assert!(Registry::load(&paths).unwrap().get("s/sk").is_err());
    }
}
