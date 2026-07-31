//! import-existing：扫描存量 skill 目录（~/.agents/skills、~/.claude/skills、
//! ~/.codex/skills、~/.cursor/skills），识别 + 登记进 registry。
//! 无源 → unmanaged（虚拟源，computed_hash=None，不可升级）；有 .git 可溯源 → 重装入池。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};
use crate::source::{derive_source_name, Source, SourcesStore};
use std::path::Path;

/// import 结果汇总。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ImportReport {
    /// 成功登记（含 unmanaged）的 skill 名。
    pub imported: Vec<String>,
    /// 以 unmanaged 登记的 skill 名称。
    pub unmanaged: Vec<String>,
    /// 可溯源并重装入池的 skill 名称。
    pub reinstalled: Vec<String>,
    /// 跳过的 skill 名称（重复 / 无效 / symlink / 无 SKILL.md / 重装撞占位）。
    pub skipped: Vec<String>,
}

pub fn import_existing(paths: &Paths, dry_run: bool) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let reg = Registry::load(paths)?;
    let mut registered: std::collections::HashSet<String> =
        reg.skills.values().map(|m| m.name.clone()).collect();

    let mut plan: Vec<(String, String, Option<String>)> = Vec::new(); // (name, canonical, package?)

    // 1. ~/.agents/skills：全局落地点本体，一律 unmanaged（重装会自撞自身占位，无意义）
    scan_dir(
        &paths.agents_skills_dir(),
        false,
        false,
        &mut plan,
        &mut report.skipped,
    );
    // 2. ~/.claude/skills：跳过 symlink，真实目录 unmanaged
    scan_dir(
        &paths.claude_skills_dir(),
        true,
        false,
        &mut plan,
        &mut report.skipped,
    );
    // 3/4. codex/cursor：有 .git 尝试重装，否则 unmanaged
    scan_dir(
        &paths.codex_skills_dir(),
        true,
        true,
        &mut plan,
        &mut report.skipped,
    );
    scan_dir(
        &paths.cursor_skills_dir(),
        true,
        true,
        &mut plan,
        &mut report.skipped,
    );

    for (name, canonical, package) in plan {
        if registered.contains(&name) {
            tracing::warn!(
                "import 跳过同名 skill {name}（已从其他目录登记过，若实际不同需手工处理）"
            );
            report.skipped.push(name);
            continue;
        }
        if let Some(pkg) = package {
            if dry_run {
                report.reinstalled.push(name.clone());
                registered.insert(name.clone());
                report.imported.push(name);
                continue;
            }
            match try_reinstall(paths, &name, &pkg) {
                Ok(()) => {
                    report.reinstalled.push(name.clone());
                    registered.insert(name.clone());
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "重装 {name} 失败，保留原状");
                    report.skipped.push(name.clone());
                    continue;
                }
            }
        } else if !dry_run {
            let mut reg = Registry::load(paths)?;
            reg.upsert(SkillMeta {
                id: Registry::skill_id("unmanaged", &name),
                name: name.clone(),
                source: "unmanaged".into(),
                scope: Scope::Global,
                version: None,
                computed_hash: None,
                installed_at: crate::install::now_iso(),
                canonical_path: canonical.clone(),
            });
            reg.save(paths)?;
            registered.insert(name.clone());
            report.unmanaged.push(name.clone());
        } else {
            registered.insert(name.clone());
            report.unmanaged.push(name.clone());
        }
        report.imported.push(name);
    }
    Ok(report)
}

/// 扫描一个目录：收集 (name, canonical_path, package?)。SKILL.md 存在才算 skill。
fn scan_dir(
    dir: &Path,
    skip_symlink: bool,
    allow_reinstall: bool,
    plan: &mut Vec<(String, String, Option<String>)>,
    skipped: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // 目录不存在 / 无权限：不是错误，静默跳过
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        if skip_symlink && p.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !p.join("SKILL.md").exists() {
            skipped.push(format!("{name}（无 SKILL.md）"));
            continue;
        }
        let package = if allow_reinstall && p.join(".git").exists() {
            read_git_remote(&p)
        } else {
            None
        };
        plan.push((name, p.to_string_lossy().into_owned(), package));
    }
}

/// 读 git remote url（溯源 package）。失败返回 None。
fn read_git_remote(dir: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// 重装入池：derive source name → 注册 source → install Global。撞占位/下载失败 → Err。
fn try_reinstall(paths: &Paths, name: &str, package: &str) -> Result<()> {
    let source_name = derive_source_name(package).ok_or_else(|| SkillkitError::Tool {
        message: format!("无法从 {package} 推导源名"),
    })?;
    let mut store = SourcesStore::load(paths)?;
    if store.get(&source_name).is_err() {
        store.add(Source {
            name: source_name.clone(),
            package: Some(package.into()),
        })?;
        store.save(paths)?;
    }
    crate::install::install(paths, &source_name, name, package, Scope::Global)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_skill(dir: &Path, name: &str) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("SKILL.md"),
            format!("---\nname: {name}\n---\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn import_registers_unmanaged_and_skips_invalid() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());

        // 有效的：agents/foo、codex/bar、claude/baz（真实目录）
        make_skill(&paths.agents_skills_dir(), "foo");
        make_skill(&paths.codex_skills_dir(), "bar");
        make_skill(&paths.claude_skills_dir(), "baz");

        // 无效的：claude 下的 symlink（指向 agents/foo）、无 SKILL.md 目录、空目录
        std::fs::create_dir_all(paths.claude_skills_dir()).unwrap();
        std::os::unix::fs::symlink(
            paths.agents_skills_dir().join("foo"),
            paths.claude_skills_dir().join("foo-link"),
        )
        .unwrap();
        let no_md = paths.codex_skills_dir().join("no-md");
        std::fs::create_dir_all(&no_md).unwrap();
        let empty = paths.cursor_skills_dir().join("empty");
        std::fs::create_dir_all(&empty).unwrap();

        let report = import_existing(&paths, false).unwrap();

        // foo/bar/baz 登记为 unmanaged
        assert!(report.imported.contains(&"foo".to_string()));
        assert!(report.imported.contains(&"bar".to_string()));
        assert!(report.imported.contains(&"baz".to_string()));
        assert_eq!(report.unmanaged.len(), 3);

        let reg = Registry::load(&paths).unwrap();
        let foo = reg.get("unmanaged/foo").unwrap();
        assert_eq!(foo.source, "unmanaged");
        assert!(foo.computed_hash.is_none());
        assert_eq!(foo.scope, Scope::Global);
        let baz = reg.get("unmanaged/baz").unwrap();
        assert_eq!(
            baz.canonical_path,
            paths.claude_skills_dir().join("baz").to_string_lossy()
        );

        // symlink 跳过（claude 里只有 baz 登记，foo-link 没有）
        assert!(reg.get("unmanaged/foo-link").is_err());
        // 无 SKILL.md / 空目录跳过
        assert!(reg.get("unmanaged/no-md").is_err());
        assert!(reg.get("unmanaged/empty").is_err());
    }

    #[test]
    fn import_dry_run_writes_nothing() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        make_skill(&paths.agents_skills_dir(), "foo");

        let report = import_existing(&paths, true).unwrap();
        assert!(report.unmanaged.contains(&"foo".to_string()));
        assert!(
            Registry::load(&paths).unwrap().skills.is_empty(),
            "dry-run 不写 registry"
        );
    }

    #[test]
    fn import_dry_run_dedups_same_name_across_dirs() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 迁移场景：同名 skill 同时存在于 agents 与 claude 两处
        make_skill(&paths.agents_skills_dir(), "foo");
        make_skill(&paths.claude_skills_dir(), "foo");

        let report = import_existing(&paths, true).unwrap();

        // dry-run 与真实运行一致：第一个登记为 unmanaged，第二个跳过
        assert_eq!(
            report.unmanaged.iter().filter(|n| *n == "foo").count(),
            1,
            "同名跨目录 dry-run 只报一次 unmanaged"
        );
        assert!(report.skipped.contains(&"foo".to_string()));
    }

    #[test]
    fn import_is_idempotent() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        make_skill(&paths.agents_skills_dir(), "foo");

        let r1 = import_existing(&paths, false).unwrap();
        assert_eq!(r1.imported.len(), 1);

        let r2 = import_existing(&paths, false).unwrap();
        assert!(r2.imported.is_empty(), "重复跑不重复登记");
        assert!(r2.skipped.contains(&"foo".to_string()));
        assert_eq!(Registry::load(&paths).unwrap().skills.len(), 1);
    }
}
