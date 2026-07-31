//! 全局 global 桥接：canonical 在池子（~/.skillkit/.agents/skills/），global skill 需
//! symlink 池子→~/.agents/skills/（Cursor 等直读）+ ~/.claude/skills/→~/.agents/skills/（Claude 桥接）。幂等。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Scope, SkillMeta};
use std::path::Path;

/// 为 global skill 建两层 symlink：agents 落地 + Claude 桥接。幂等。
/// local skill 不桥接（等 project apply 落到项目目录）。
pub fn ensure_global_claude(paths: &Paths, meta: &SkillMeta) -> Result<()> {
    if meta.scope != Scope::Global {
        return Ok(());
    }
    let canonical = Path::new(&meta.canonical_path);
    let agents_link = paths.agents_skills_dir().join(&meta.name);
    let claude_link = paths.claude_skills_dir().join(&meta.name);
    // 池子 → ~/.agents/skills/<name>（agent 直读）
    ensure_link(canonical, &agents_link)?;
    // ~/.agents/skills/<name> → ~/.claude/skills/<name>（Claude 桥接）
    ensure_link(&agents_link, &claude_link)?;
    Ok(())
}

/// 幂等建 symlink：link → target。指向正确跳过，指向错误删旧重建，真实目录占位报错不静默删。
fn ensure_link(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Ok(existing) = std::fs::read_link(link) {
        if existing.as_path() == target {
            return Ok(());
        }
        std::fs::remove_file(link).map_err(|e| SkillkitError::Tool {
            message: e.to_string(),
        })?;
    } else if link.exists() {
        // 真实目录占位（非 symlink）：报错引导，不静默删
        return Err(SkillkitError::CanonicalCreate(link.to_path_buf()));
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).map_err(|e| SkillkitError::Tool {
        message: format!("symlink 失败：{e}"),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Paths;
    use tempfile::tempdir;

    fn global_meta(canonical: &str, name: &str) -> SkillMeta {
        SkillMeta {
            id: format!("s/{name}"),
            name: name.into(),
            source: "s".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-07-29T00:00:00Z".into(),
            canonical_path: canonical.into(),
        }
    }

    #[test]
    fn creates_two_links_and_is_idempotent() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 池子真实目录
        let canon = tmp.path().join(".skillkit/.agents/skills/foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();

        let meta = global_meta(&canon.to_string_lossy(), "foo");
        ensure_global_claude(&paths, &meta).unwrap();

        let agents_link = paths.agents_skills_dir().join("foo");
        let claude_link = paths.claude_skills_dir().join("foo");
        assert!(agents_link.is_symlink(), "agents 落地 symlink 已建");
        assert!(claude_link.is_symlink(), "Claude 桥接 symlink 已建");

        // 幂等：再调不报错、链接仍在
        ensure_global_claude(&paths, &meta).unwrap();
        assert!(agents_link.is_symlink());
        assert!(claude_link.is_symlink());
    }

    #[test]
    fn skips_local_scope() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let mut meta = global_meta("whatever", "foo");
        meta.scope = Scope::Local;
        ensure_global_claude(&paths, &meta).unwrap();
        assert!(!paths.claude_skills_dir().exists());
    }
}
