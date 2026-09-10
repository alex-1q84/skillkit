//! registry.json schema 迁移：按 version 字段逐级升级，CLI/server 启动时自动执行。
//! 原则：确定性回填（数据来自本地 skills-lock.json，不猜不联网）；查不到的证据留空；
//! 迁移幂等，失败由调用方决定是否阻塞（建议 warn 不阻塞，旧 schema 仍可读）。
use crate::error::Result;
use crate::paths::Paths;
use crate::registry::Registry;

/// 当前 schema 版本。新增迁移时：写 `migrate_v{n}_xxx` 函数 + 在 run_migrations 加一级 + 此数 +1。
pub const CURRENT_VERSION: u32 = 1;

/// 迁移 registry 到 CURRENT_VERSION。返回是否发生了迁移（含仅升版本号的空迁移）。
/// 持 "registry" 锁执行，与并发写方（install/import 等）串行化。
pub fn migrate(paths: &Paths) -> Result<bool> {
    let _lock = crate::lock::FileLock::acquire(paths, "registry")?;
    let mut reg = Registry::load(paths)?; // 已持锁，load 不重取
    if reg.version >= CURRENT_VERSION {
        return Ok(false);
    }
    let from = reg.version;
    run_migrations(paths, &mut reg, from)?;
    reg.version = CURRENT_VERSION;
    reg.save_raw(paths)?; // 已持锁，不重取（同进程 flock 自死锁）
    Ok(true)
}

/// 从 from 版本逐级升到 CURRENT。每级迁移只补证据充分的字段。
fn run_migrations(paths: &Paths, reg: &mut Registry, from: u32) -> Result<()> {
    if from < 1 {
        backfill_spec(paths, reg)?;
    }
    Ok(())
}

/// v0→v1：registry 源（skills.sh）managed 条目补 spec（原始安装名 owner/repo@skill）。
/// owner/repo 取自 npx 的 skills-lock.json（install 时 npx 记录的来源，确定性数据）；
/// lock 里查不到的条目保持 None，不猜。
fn backfill_spec(paths: &Paths, reg: &mut Registry) -> Result<()> {
    let sources = crate::npx::read_lock_sources(paths)?;
    for meta in reg.skills.values_mut() {
        if meta.source != "skills.sh" || meta.spec.is_some() || meta.computed_hash.is_none() {
            continue;
        }
        if let Some(owner_repo) = sources.get(&meta.name) {
            meta.spec = Some(format!("{owner_repo}@{}", meta.name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Scope, SkillMeta};
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    /// v0 形态：无 version 字段 + skills.sh 条目无 spec（serde default 补位）。
    fn v0_meta(name: &str, source: &str, hash: Option<&str>) -> SkillMeta {
        SkillMeta {
            id: format!("{source}/{name}"),
            name: name.into(),
            source: source.into(),
            spec: None,
            scope: Scope::Global,
            version: None,
            computed_hash: hash.map(str::to_string),
            installed_at: "2026-08-01T00:00:00Z".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{name}"),
        }
    }

    /// 种 npx lock：短名 → owner/repo + hash（migrate 只读 source 字段）。
    fn seed_lock(paths: &Paths, entries: &[(&str, &str)]) {
        let skills: serde_json::Map<String, serde_json::Value> = entries
            .iter()
            .map(|(n, src)| {
                (
                    (*n).into(),
                    serde_json::json!({"computedHash": "h", "source": src, "sourceType": "github"}),
                )
            })
            .collect();
        let path = paths.skillkit_dir().join("skills-lock.json");
        std::fs::create_dir_all(paths.skillkit_dir()).unwrap();
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::json!({"version": 1, "skills": skills}))
                .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn v1_backfills_spec_from_lock() {
        let p = paths();
        let mut reg = Registry::default();
        reg.upsert(v0_meta("pdf", "skills.sh", Some("h1")));
        reg.upsert(v0_meta("humanizer", "local", Some("h2"))); // 非 registry 源，不动
        reg.upsert(v0_meta("legacy", "unmanaged", None)); // unmanaged，不动
        reg.save(&p).unwrap();
        seed_lock(&p, &[("pdf", "anthropics/skills")]);

        assert!(migrate(&p).unwrap());
        let reg = Registry::load(&p).unwrap();
        assert_eq!(reg.version, CURRENT_VERSION);
        assert_eq!(
            reg.get("skills.sh/pdf").unwrap().spec.as_deref(),
            Some("anthropics/skills@pdf")
        );
        assert_eq!(reg.get("local/humanizer").unwrap().spec, None);
        assert_eq!(reg.get("unmanaged/legacy").unwrap().spec, None);
    }

    #[test]
    fn v1_lock_missing_keeps_spec_none_but_upgrades_version() {
        let p = paths();
        let mut reg = Registry::default();
        reg.upsert(v0_meta("pdf", "skills.sh", Some("h1")));
        reg.save(&p).unwrap(); // 无 lock 文件

        assert!(migrate(&p).unwrap());
        let reg = Registry::load(&p).unwrap();
        // 证据不足不猜：spec 保持 None，但版本号已升，不再重复迁移
        assert_eq!(reg.get("skills.sh/pdf").unwrap().spec, None);
        assert_eq!(reg.version, CURRENT_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let p = paths();
        let mut reg = Registry::default();
        reg.upsert(v0_meta("pdf", "skills.sh", Some("h1")));
        reg.save(&p).unwrap();
        seed_lock(&p, &[("pdf", "anthropics/skills")]);

        assert!(migrate(&p).unwrap());
        // 第二次：已到 CURRENT，直接返回 false，不再写盘
        assert!(!migrate(&p).unwrap());
    }
}
