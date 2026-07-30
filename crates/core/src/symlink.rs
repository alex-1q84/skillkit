//! 全局 Claude symlink 桥接：Claude 不直读 ~/.agents/skills，需 symlink 到
//! ~/.claude/skills/<name> → canonical。幂等。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Scope, SkillMeta};
use std::path::Path;

/// 为 global skill 建 ~/.claude/skills/<name> → canonical 的 symlink。幂等：
/// 已存在且指向正确则跳过，指向错误则删旧重建，真实目录占位则报错不静默删。
pub fn ensure_global_claude(paths: &Paths, meta: &SkillMeta) -> Result<()> {
    if meta.scope != Scope::Global {
        return Ok(()); // 只桥接 global；local 的 project 落地在 M1
    }
    let link = paths.claude_skills_dir().join(&meta.name);
    let target = Path::new(&meta.canonical_path);
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Ok(existing) = std::fs::read_link(&link) {
        if existing.as_path() == target {
            return Ok(());
        }
        // 指向错误：删旧重建
        std::fs::remove_file(&link).map_err(|e| SkillkitError::Git {
            message: e.to_string(),
        })?;
    } else if link.exists() {
        // 真实目录占位（非 symlink）：报错引导，不静默删
        return Err(SkillkitError::CanonicalCreate(link));
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, &link).map_err(|e| SkillkitError::Git {
        message: format!("symlink 失败：{e}"),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Scope, SkillMeta};
    use crate::Paths;
    use tempfile::tempdir;

    fn global_meta(canonical: &str, name: &str) -> SkillMeta {
        SkillMeta {
            id: format!("s/{name}"),
            name: name.into(),
            source: "s".into(),
            scope: Scope::Global,
            version: None,
            commit_sha: Some("abc".into()),
            installed_at: "2026-07-29T00:00:00Z".into(),
            canonical_path: canonical.into(),
        }
    }

    #[test]
    fn creates_symlink_and_is_idempotent() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // 造 canonical 真实目录
        let canon = tmp.path().join(".agents/skills/foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();

        let meta = global_meta(&canon.to_string_lossy(), "foo");
        ensure_global_claude(&paths, &meta).unwrap();
        let link = paths.claude_skills_dir().join("foo");
        assert!(link.is_symlink());

        // 幂等：再调一次不报错、链接仍在
        ensure_global_claude(&paths, &meta).unwrap();
        assert!(link.is_symlink());
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
