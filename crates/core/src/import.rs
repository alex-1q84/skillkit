//! import-existing：扫描存量 skill 目录（~/.agents/skills、~/.claude/skills、
//! ~/.codex/skills、~/.cursor/skills），识别 + 登记进 registry。
//! 无源 → unmanaged（虚拟源，computed_hash=None，不可升级）；有 .git 可溯源 → 重装入池。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};
use crate::source::{derive_source_name, Source, SourcesStore};
use std::path::{Path, PathBuf};

/// import 结果汇总。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ImportReport {
    /// 成功登记（含 unmanaged）的 skill 名。
    pub imported: Vec<String>,
    /// 以 unmanaged 登记的 skill 名称。
    pub unmanaged: Vec<String>,
    /// 可溯源并重装入池的 skill 名称。
    pub reinstalled: Vec<String>,
    /// 跳过的 skill 名称（重复 / 无效 / symlink / 无 SKILL.md / 重装失败 / 桥接撞占位）。
    pub skipped: Vec<String>,
    /// 新发现并迁入池子的 skill（主循环 unmanaged 分支 adopt）。
    pub relocated: Vec<String>,
    /// 存量补迁入池的 skill（relink_unmanaged）。
    pub relinked: Vec<String>,
}

pub fn import_existing(paths: &Paths, dry_run: bool) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    relink_unmanaged(paths, &mut report, dry_run)?;
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
        } else {
            // unmanaged 分支（无 package）
            let canon_path = Path::new(&canonical);
            adopt_unmanaged(
                paths,
                &name,
                canon_path,
                dry_run,
                &mut registered,
                &mut report,
            )?;
            continue; // adopt_unmanaged 自管 imported.push，防落末尾双计
        }
        report.imported.push(name);
    }
    Ok(report)
}

/// 主循环 unmanaged 分支：symlink src 跳过；真实目录 adopt 入池 → registry save → 桥接。
/// imported/unmanaged/relocated/skipped 计数由本函数负责。
fn adopt_unmanaged(
    paths: &Paths,
    name: &str,
    canon_path: &Path,
    dry_run: bool,
    registered: &mut std::collections::HashSet<String>,
    report: &mut ImportReport,
) -> Result<()> {
    // 步骤 0：symlink src → skipped（dry_run 分叉前生效，防 rename symlink 产悬空 canonical）
    if std::fs::symlink_metadata(canon_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        report
            .skipped
            .push(format!("{name}（symlink，只迁真实目录）"));
        return Ok(());
    }
    if dry_run {
        registered.insert(name.to_string());
        report.unmanaged.push(name.to_string());
        report.relocated.push(name.to_string());
        report.imported.push(name.to_string());
        return Ok(());
    }
    // adopt → registry save → 桥接（对齐 install.rs:45-52 顺序）。
    // 迁移+登记全程持 "registry" 锁：防并发写方（rescope/upgrade）的旧快照 save 覆盖回滚
    let lock = crate::lock::FileLock::acquire(paths, "registry")?;
    let target = adopt_into_pool(paths, name, canon_path)?;
    let meta = SkillMeta {
        id: Registry::skill_id("unmanaged", name),
        name: name.into(),
        source: "unmanaged".into(),
        scope: Scope::Global,
        version: None,
        computed_hash: None,
        installed_at: crate::install::now_iso(),
        canonical_path: target.to_string_lossy().into_owned(),
    };
    let mut reg = Registry::load(paths)?;
    reg.upsert(meta.clone());
    reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
    drop(lock); // 桥接是纯 fs 操作，不占 registry 锁
    registered.insert(name.to_string());
    report.unmanaged.push(name.to_string());
    report.relocated.push(name.to_string());
    report.imported.push(name.to_string());
    // 桥接失败降级 skipped 点名，不中止其余 skill：registry 已一致（canonical 指池），
    // 手动清理占位目录后重跑 import，relink 幂等补建桥接即收敛（spec §3.3）
    if let Err(e) = crate::symlink::ensure_global_claude(paths, &meta) {
        tracing::warn!(error = ?e, "import 桥接失败 {name}：{e}");
        report.skipped.push(format!(
            "{name}（桥接失败：{e}，清理占位目录后重跑 import）"
        ));
    }
    Ok(())
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

/// 把真实目录 src 迁入池子 ~/.skillkit/.agents/skills/<name>。
/// 池子已有同名 → 删 src 冗余副本（池子权威，对齐 scope.rs:60-64）；
/// 池子空、src 在 → rename（同 FS 原子）；src 空 target 在 → 幂等返回 target（中间态收敛）。
/// src 必须是真实目录——调用方负责过滤 symlink（对齐 import.rs:129「只迁真实目录」）。
fn adopt_into_pool(paths: &Paths, name: &str, src: &Path) -> Result<PathBuf> {
    let target = paths.skillkit_skills_dir().join(name);
    if target.exists() {
        if src.exists() {
            std::fs::remove_dir_all(src)?;
        }
    } else if src.exists() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(src, &target)?;
    }
    Ok(target)
}

/// 遍历 registry 的 unmanaged global skill：
/// - canonical 不在池且是真实目录 → adopt 入池 + 更新 canonical_path + 立即 save（对齐 §3.2 顺序）
/// - canonical 不在池但 dangling/symlink → warn 跳过，**不**补桥接（防自指环，spec §3.3）
/// - canonical 已在池（含刚归槽）→ 补建缺失桥接（ensure_global_claude 幂等）
///
/// dry_run 只统计 report.relinked，不迁移/不桥接。
fn relink_unmanaged(paths: &Paths, report: &mut ImportReport, dry_run: bool) -> Result<()> {
    let pool = paths.skillkit_skills_dir();
    let reg = Registry::load(paths)?;
    let unmanaged: Vec<SkillMeta> = reg
        .skills
        .values()
        .filter(|m| m.source == "unmanaged" && m.scope == Scope::Global)
        .cloned()
        .collect();
    for mut meta in unmanaged {
        let canon = Path::new(&meta.canonical_path);
        if !canon.starts_with(&pool) {
            // canonical 不在池：尝试归槽
            let is_real_dir = std::fs::symlink_metadata(canon)
                .map(|m| m.file_type().is_dir() && !m.file_type().is_symlink())
                .unwrap_or(false);
            if !is_real_dir {
                tracing::warn!(
                    "relink 跳过 unmanaged {}：canonical {} 非真实目录（dangling/symlink）",
                    meta.name,
                    meta.canonical_path
                );
                continue; // 不补桥接
            }
            if dry_run {
                report.relinked.push(meta.name.clone());
                continue;
            }
            // 迁移+登记全程持 "registry" 锁（防并发写方旧快照覆盖回滚，对齐 adopt_unmanaged）
            let lock = crate::lock::FileLock::acquire(paths, "registry")?;
            let target = adopt_into_pool(paths, &meta.name, canon)?;
            meta.canonical_path = target.to_string_lossy().into_owned();
            // 立即落盘（每 skill adopt 后 save，失败面可推导）
            let mut reg = Registry::load(paths)?;
            reg.upsert(meta.clone());
            reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
            drop(lock); // 桥接是纯 fs 操作，不占 registry 锁
            report.relinked.push(meta.name.clone());
        }
        // canonical 已在池（刚归槽或本就在）：补建缺失桥接（幂等，在位跳过）。
        // 桥接撞真实目录占位降级 skipped 点名，不中止其余 skill——承重墙在 ensure_link
        //（占位不静默删），此处只放弃传播：registry 已一致，清占位后重跑即收敛
        if !dry_run {
            if let Err(e) = crate::symlink::ensure_global_claude(paths, &meta) {
                tracing::warn!(error = ?e, "relink 桥接失败 {}：{e}", meta.name);
                report.skipped.push(format!(
                    "{}（桥接失败：{e}，清理占位目录后重跑 import）",
                    meta.name
                ));
            }
        }
    }
    Ok(())
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
    fn adopt_into_pool_migrates_real_dir() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let src = tmp.path().join("src/foo");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "x").unwrap();
        let target = adopt_into_pool(&paths, "foo", &src).unwrap();
        assert_eq!(target, paths.skillkit_skills_dir().join("foo"));
        assert!(target.join("SKILL.md").exists(), "内容随迁移");
        assert!(!src.exists(), "原位置已迁走");
    }

    #[test]
    fn adopt_into_pool_dedup_when_pool_has_canonical() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 池子已有 foo（旧残留）
        let pool = paths.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&pool).unwrap();
        std::fs::write(pool.join("SKILL.md"), "pool").unwrap();
        // 原位置也有 foo（冗余副本）
        let src = tmp.path().join("src/foo");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "src").unwrap();
        let target = adopt_into_pool(&paths, "foo", &src).unwrap();
        assert_eq!(
            std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
            "pool",
            "池子权威，src 副本删除"
        );
        assert!(!src.exists(), "冗余副本已删");
    }

    #[test]
    fn adopt_into_pool_idempotent_when_src_gone_target_present() {
        // 中间态：上次 adopt 入池、registry 未落盘，重跑 src 空 target 在
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let pool = paths.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&pool).unwrap();
        std::fs::write(pool.join("SKILL.md"), "x").unwrap();
        let src = tmp.path().join("src/foo"); // 不存在
        let target = adopt_into_pool(&paths, "foo", &src).unwrap();
        assert_eq!(target, pool);
        assert!(pool.join("SKILL.md").exists(), "池子保留不报错");
    }

    fn seed_unmanaged_global(paths: &Paths, name: &str, canonical: &Path) {
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(SkillMeta {
            id: Registry::skill_id("unmanaged", name),
            name: name.into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "t".into(),
            canonical_path: canonical.to_string_lossy().into_owned(),
        });
        reg.save(paths).unwrap();
    }

    #[test]
    fn relink_migrates_existing_unmanaged_to_pool() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 存量 unmanaged，canonical 在 ~/.agents/skills/foo（真实目录，无桥接）
        let canon = paths.agents_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        seed_unmanaged_global(&paths, "foo", &canon);

        let mut report = ImportReport::default();
        relink_unmanaged(&paths, &mut report, false).unwrap();
        assert_eq!(report.relinked, vec!["foo".to_string()]);
        let pool = paths.skillkit_skills_dir().join("foo");
        assert!(pool.join("SKILL.md").exists(), "迁入池");
        assert!(
            !std::fs::symlink_metadata(&canon).unwrap().is_dir(),
            "原位置真实目录已迁空（后建桥接 symlink）"
        );
        // 桥接建（agents 位置=原 canon，迁空后建 symlink）
        assert!(
            paths.agents_skills_dir().join("foo").is_symlink(),
            "agents 桥接"
        );
        assert!(
            paths.claude_skills_dir().join("foo").is_symlink(),
            "claude 桥接"
        );
        let reg_after = Registry::load(&paths).unwrap();
        let m = reg_after.get("unmanaged/foo").unwrap();
        assert_eq!(
            m.canonical_path,
            pool.to_string_lossy(),
            "registry canonical 更新"
        );
    }

    #[test]
    fn relink_rebuilds_missing_bridge_for_pooled_canonical() {
        // 中间态：canonical 已在池但桥接缺
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let pool = paths.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&pool).unwrap();
        std::fs::write(pool.join("SKILL.md"), "x").unwrap();
        seed_unmanaged_global(&paths, "foo", &pool);

        let mut report = ImportReport::default();
        relink_unmanaged(&paths, &mut report, false).unwrap();
        assert!(report.relinked.is_empty(), "已在池不重复 relinked");
        assert!(
            paths.agents_skills_dir().join("foo").is_symlink(),
            "补建 agents 桥接"
        );
        assert!(
            paths.claude_skills_dir().join("foo").is_symlink(),
            "补建 claude 桥接"
        );
    }

    #[test]
    fn relink_skips_dangling_without_bridge() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // canonical 指不存在的 agents 路径（dangling）
        let dangling = paths.agents_skills_dir().join("foo");
        seed_unmanaged_global(&paths, "foo", &dangling);

        let mut report = ImportReport::default();
        relink_unmanaged(&paths, &mut report, false).unwrap();
        assert!(report.relinked.is_empty());
        // 不建自指/悬空桥接（P2-1 关键）
        assert!(
            !paths.agents_skills_dir().join("foo").exists(),
            "无 agents symlink"
        );
        assert!(
            !paths.claude_skills_dir().join("foo").exists(),
            "无 claude symlink"
        );
    }

    #[test]
    fn relink_skips_symlink_canonical_without_bridge() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let real = tmp.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(paths.agents_skills_dir()).unwrap();
        let link = paths.agents_skills_dir().join("foo");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        seed_unmanaged_global(&paths, "foo", &link);

        let mut report = ImportReport::default();
        relink_unmanaged(&paths, &mut report, false).unwrap();
        assert!(report.relinked.is_empty());
        assert!(link.is_symlink(), "原 symlink 保留未动");
        assert!(!paths.claude_skills_dir().join("foo").exists(), "不建桥接");
    }

    #[test]
    fn relink_dry_run_counts_only() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let canon = paths.agents_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        seed_unmanaged_global(&paths, "foo", &canon);

        let mut report = ImportReport::default();
        relink_unmanaged(&paths, &mut report, true).unwrap();
        assert_eq!(report.relinked, vec!["foo".to_string()]);
        assert!(canon.exists(), "dry_run 不迁文件");
        assert!(
            !paths.agents_skills_dir().join("foo").is_symlink(),
            "dry_run 不建桥接"
        );
    }

    #[test]
    fn import_skips_symlink_src_in_agents() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // ~/.agents/skills/foo 是 symlink 指向外部真实目录
        let real = tmp.path().join("real-foo");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("SKILL.md"), "x").unwrap();
        std::fs::create_dir_all(paths.agents_skills_dir()).unwrap();
        std::os::unix::fs::symlink(&real, paths.agents_skills_dir().join("foo")).unwrap();

        let report = import_existing(&paths, false).unwrap();
        assert!(
            report.skipped.iter().any(|s| s.contains("foo")),
            "symlink src 进 skipped"
        );
        assert!(
            !paths.skillkit_skills_dir().join("foo").exists(),
            "池子不出现 symlink-canonical"
        );
        assert!(
            Registry::load(&paths)
                .unwrap()
                .get("unmanaged/foo")
                .is_err(),
            "registry 不登记 symlink src"
        );
    }

    #[test]
    fn import_cross_dir_same_name_agents_claude_degrades() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // agents/foo + claude/foo 均真实目录
        make_skill(&paths.agents_skills_dir(), "foo");
        make_skill(&paths.claude_skills_dir(), "foo");
        // agents/foo adopt 入池后建 claude 桥接撞 claude/foo 真实目录 → 降级 skipped 点名，不中止
        let report = import_existing(&paths, false).unwrap();
        assert!(
            report.skipped.iter().any(|s| s.contains("foo（桥接失败")),
            "撞占位进 skipped 点名：{:?}",
            report.skipped
        );
        let reg = Registry::load(&paths).unwrap();
        let foo = reg.get("unmanaged/foo").unwrap();
        assert_eq!(
            foo.canonical_path,
            paths.skillkit_skills_dir().join("foo").to_string_lossy(),
            "agents/foo 已入池并落盘 registry"
        );
        assert!(
            paths
                .claude_skills_dir()
                .join("foo")
                .join("SKILL.md")
                .exists(),
            "claude/foo 占位真实目录不静默删"
        );
    }

    #[test]
    fn relink_bridge_placeholder_degrades_not_aborts() {
        // 真实事故场景：registry 已登记 unmanaged（canonical 在池），
        // ~/.claude/skills/<name> 被 Claude CLI 自装的真实目录占位 → 不再整批中断
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        make_skill(&paths.skillkit_skills_dir(), "dws");
        seed_unmanaged_global(&paths, "dws", &paths.skillkit_skills_dir().join("dws"));
        // agents 桥接在位，claude 位置被真实目录占位
        std::fs::create_dir_all(paths.agents_skills_dir()).unwrap();
        std::os::unix::fs::symlink(
            paths.skillkit_skills_dir().join("dws"),
            paths.agents_skills_dir().join("dws"),
        )
        .unwrap();
        make_skill(&paths.claude_skills_dir(), "dws");

        let report = import_existing(&paths, false).unwrap();
        assert!(
            report.skipped.iter().any(|s| s.contains("dws（桥接失败")),
            "占位冲突降级 skipped 点名：{:?}",
            report.skipped
        );
        assert!(
            paths
                .claude_skills_dir()
                .join("dws")
                .join("SKILL.md")
                .exists(),
            "占位目录不静默删"
        );
        assert!(
            paths.agents_skills_dir().join("dws").is_symlink(),
            "已正确的 agents 桥接保留"
        );
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
        assert_eq!(report.relocated.len(), 3, "三个迁池");

        let reg = Registry::load(&paths).unwrap();
        let foo = reg.get("unmanaged/foo").unwrap();
        assert_eq!(foo.source, "unmanaged");
        assert!(foo.computed_hash.is_none());
        assert_eq!(foo.scope, Scope::Global);
        let baz = reg.get("unmanaged/baz").unwrap();
        assert_eq!(
            baz.canonical_path,
            paths.skillkit_skills_dir().join("baz").to_string_lossy(),
            "baz 迁入池子（不再指原 claude 位置）"
        );
        // 桥接在位
        assert!(
            paths.agents_skills_dir().join("baz").is_symlink(),
            "agents 桥接 symlink"
        );
        assert!(
            paths.claude_skills_dir().join("baz").is_symlink(),
            "claude 桥接 symlink"
        );
        // foo/bar 同理迁池（断言 canonical 在池）
        for n in ["foo", "bar"] {
            assert!(
                paths
                    .skillkit_skills_dir()
                    .join(n)
                    .join("SKILL.md")
                    .exists(),
                "{n} 迁入池子"
            );
        }
        // codex 历史目录迁空（bar 来自 codex，原目录整个 rename 走）
        assert!(
            !paths.codex_skills_dir().join("bar").exists(),
            "codex/bar 原位置迁空"
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
            paths
                .agents_skills_dir()
                .join("foo")
                .join("SKILL.md")
                .exists(),
            "dry-run 不迁文件（原位置仍是真实目录）"
        );
        assert!(
            !paths.agents_skills_dir().join("foo").is_symlink()
                && !paths.claude_skills_dir().join("foo").exists(),
            "dry-run 不建桥接"
        );
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
