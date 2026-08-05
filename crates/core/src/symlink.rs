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

/// 撤 global skill 的两层 symlink（与 ensure_global_claude 对称）。canonical 池子不删。
/// 不加 scope 守卫：set_scope 在改 scope 之后调用本函数，meta.scope 已是 local，
/// 镜像 ensure 的守卫会 no-op 留悬空链。调用方保证语义（见 spec §3.1 P2-A）。
/// 幂等：链接不存在静默跳过。真实目录（非 symlink）报错不删（数据损失防护，对齐 ensure_link 守卫）。
pub fn remove_global_claude(paths: &Paths, meta: &SkillMeta) -> Result<()> {
    let agents_link = paths.agents_skills_dir().join(&meta.name);
    let claude_link = paths.claude_skills_dir().join(&meta.name);
    remove_one_link(&claude_link)?; // 先 claude（→ agents_link）再 agents（→ canonical），反序留悬空链
    remove_one_link(&agents_link)?;
    Ok(())
}

/// 删单个 symlink：不存在跳过（幂等）；真实目录/文件占位报错不删（对齐 ensure_link 守卫）。
fn remove_one_link(link: &Path) -> Result<()> {
    if !link.exists() && std::fs::symlink_metadata(link).is_err() {
        return Ok(()); // 完全不存在，幂等跳过（dangling symlink 的 exists 也是 false，但 metadata Ok，不进此分支）
    }
    if std::fs::symlink_metadata(link)?.file_type().is_symlink() {
        std::fs::remove_file(link).map_err(|e| SkillkitError::Tool {
            message: e.to_string(),
        })?;
    } else {
        // 真实目录/文件占位：报错不删
        return Err(SkillkitError::CanonicalCreate(link.to_path_buf()));
    }
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

    #[test]
    fn remove_global_claude_deletes_links_idempotent() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let canon = tmp.path().join(".skillkit/.agents/skills/foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let meta = global_meta(&canon.to_string_lossy(), "foo");
        ensure_global_claude(&paths, &meta).unwrap();
        let agents_link = paths.agents_skills_dir().join("foo");
        let claude_link = paths.claude_skills_dir().join("foo");
        assert!(agents_link.is_symlink() && claude_link.is_symlink());

        remove_global_claude(&paths, &meta).unwrap();
        assert!(!agents_link.exists(), "agents symlink 已删");
        assert!(!claude_link.exists(), "claude symlink 已删");
        assert!(canon.exists(), "canonical 池子保留");

        // 幂等：再删不报错（缺失跳过）
        remove_global_claude(&paths, &meta).unwrap();
    }

    #[test]
    fn remove_global_claude_refuses_real_dir() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // ~/.agents/skills/foo 是真实目录（用户手工放），不是 symlink
        let real = paths.agents_skills_dir().join("foo");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("SKILL.md"), "x").unwrap();
        let meta = global_meta(&real.to_string_lossy(), "foo");
        assert!(remove_global_claude(&paths, &meta).is_err(), "真实目录不删");
        assert!(real.exists(), "真实目录保留");
    }
}
