//! install/uninstall：委托 npx skills 下载到 canonical 池子（~/.skillkit/.agents/skills/），
//! 读 skills-lock.json 记 computed_hash，登记 registry。scope=global 额外 symlink
//! 池子→~/.agents/skills/ + Claude 桥接。
use crate::error::{Result, SkillkitError};
use crate::npx;
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};
use crate::source::SourcesStore;
use std::path::PathBuf;

/// 安装：调 npx skills add 下载到池子，记 computed_hash，登记 registry。
/// `package` 由调用方解析（固定源用 source.package；registry 源由 CLI 层 find 选）。
/// scope=global 时额外 symlink 池子→~/.agents/skills/ + Claude 桥接，立即可用。
pub fn install(
    paths: &Paths,
    source_name: &str,
    skill_name: &str,
    package: &str,
    scope: Scope,
) -> Result<SkillMeta> {
    let store = SourcesStore::load(paths)?;
    let source = store.get(source_name)?.clone();

    let target = paths.skillkit_skills_dir().join(skill_name);
    if target.exists() {
        return Err(SkillkitError::SkillAlreadyInstalled {
            id: skill_name.to_string(),
        });
    }

    npx::add(paths, package, skill_name)?;
    let hash = npx::read_computed_hash(paths, skill_name)?;

    let id = Registry::skill_id(&source.name, skill_name);
    let meta = SkillMeta {
        id: id.clone(),
        name: skill_name.to_string(),
        source: source.name,
        scope,
        version: None,
        computed_hash: Some(hash),
        installed_at: now_iso(),
        canonical_path: target.display().to_string(),
    };
    let mut reg = Registry::load(paths)?;
    reg.upsert(meta.clone());
    reg.save(paths)?;

    // global：池子 → ~/.agents/skills/（agent 直读）+ ~/.claude/skills/（Claude 桥接）
    if scope == Scope::Global {
        crate::symlink::ensure_global_claude(paths, &meta)?;
    }
    Ok(meta)
}

/// 卸载：删 canonical 池子 + registry 记录 + 同步 npx skills lock。
pub fn uninstall(paths: &Paths, id: &str) -> Result<()> {
    let mut reg = Registry::load(paths)?;
    let meta = reg.get(id)?.clone();
    let target = PathBuf::from(&meta.canonical_path);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|_| SkillkitError::CanonicalCreate(target.clone()))?;
    }
    let _ = npx::remove(paths, &meta.name); // 同步 lock，失败不阻塞（registry 是事实源）
    reg.remove(id)?.save(paths)?;
    Ok(())
}

/// 当前时间 ISO 字符串（UTC RFC3339）。
fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}
