//! 安装本地 skill（目录/zip）到 canonical 池，managed + scope=local。
//! 不可信输入（含 GitHub zip）：name 防逃逸、zip 防穿越、symlink 跳过、体积上限、三段原子落地。
use crate::error::{Result, SkillkitError};
use std::path::{Path, PathBuf};

// Task 5 的 install_local 主函数消费前，下列符号暂无人引用；allow(dead_code) 届时一并移除。
#[allow(dead_code)]
const MAX_ZIP_BYTES: u64 = 100 * 1024 * 1024;
#[allow(dead_code)]
const MAX_ZIP_ENTRIES: usize = 10_000;

/// 校验 skill 名（防 canonical 池路径逃逸）。拒空 / `.` / `..` / 纯点串 / 含分隔符 / 非法字符。
#[allow(dead_code)]
pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.chars().all(|c| c == '.') {
        return Err(SkillkitError::InvalidLocalSkill {
            path: name.into(),
            reason: "skill 名为空、为 . / .. 或纯点串".into(),
        });
    }
    if name.contains('/') || name.contains('\\') {
        return Err(SkillkitError::InvalidLocalSkill {
            path: name.into(),
            reason: "skill 名含路径分隔符".into(),
        });
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(SkillkitError::InvalidLocalSkill {
            path: name.into(),
            reason: "skill 名仅允许字母数字 - _ .".into(),
        });
    }
    Ok(())
}

/// 读 SKILL.md frontmatter 的 name 字段（极简行匹配，零依赖）。无 frontmatter/name 返回 None。
#[allow(dead_code)]
pub(crate) fn read_skill_name(skill_md: &Path) -> Result<Option<String>> {
    let content =
        std::fs::read_to_string(skill_md).map_err(|_| SkillkitError::InvalidLocalSkill {
            path: skill_md.display().to_string(),
            reason: "SKILL.md 不可读".into(),
        })?;
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Ok(None);
    }
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim() == "---" {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("name:") {
            let v = rest.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            if !v.is_empty() {
                return Ok(Some(v.to_string()));
            }
        }
    }
    Ok(None)
}

/// 定位 skill 目录：根有 SKILL.md → 根；唯一子目录有 SKILL.md → 该子目录；否则报错。
#[allow(dead_code)]
pub(crate) fn resolve_skill_dir(src: &Path) -> Result<PathBuf> {
    if src.join("SKILL.md").is_file() {
        return Ok(src.to_path_buf());
    }
    let subdirs: Vec<PathBuf> = std::fs::read_dir(src)
        .map_err(|e| SkillkitError::InvalidLocalSkill {
            path: src.display().to_string(),
            reason: e.to_string(),
        })?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    let with_skill: Vec<&PathBuf> = subdirs
        .iter()
        .filter(|p| p.join("SKILL.md").is_file())
        .collect();
    match with_skill.len() {
        1 => Ok(with_skill[0].clone()),
        0 => Err(SkillkitError::InvalidLocalSkill {
            path: src.display().to_string(),
            reason: "未找到 SKILL.md".into(),
        }),
        _ => Err(SkillkitError::AmbiguousSkillArchive {
            reason: format!("{} 下多个目录含 SKILL.md", src.display()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validate_name_rejects_escape() {
        assert!(validate_name("").is_err());
        assert!(validate_name(".").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name("...").is_err(), "纯点串拒");
        assert!(validate_name("a/b").is_err());
        assert!(validate_name(r"a\b").is_err());
        assert!(validate_name("a b").is_err());
        assert!(validate_name("foo").is_ok());
        assert!(validate_name("foo-bar_1.2").is_ok());
    }

    #[test]
    fn read_skill_name_from_frontmatter() {
        let d = tempdir().unwrap();
        let p = d.path().join("SKILL.md");
        std::fs::write(&p, "---\nname: my-skill\ndescription: x\n---\n# my-skill\n").unwrap();
        assert_eq!(read_skill_name(&p).unwrap().as_deref(), Some("my-skill"));
    }

    #[test]
    fn read_skill_name_handles_quotes_and_missing() {
        let d = tempdir().unwrap();
        let p = d.path().join("SKILL.md");
        std::fs::write(&p, "---\nname: \"quoted\"\n---\n").unwrap();
        assert_eq!(read_skill_name(&p).unwrap().as_deref(), Some("quoted"));
        std::fs::write(&p, "# no frontmatter\n").unwrap();
        assert_eq!(read_skill_name(&p).unwrap().as_deref(), None);
    }

    #[test]
    fn resolve_skill_dir_root_vs_single_child_vs_ambiguous() {
        let d = tempdir().unwrap();
        // 根有 SKILL.md
        std::fs::write(d.path().join("SKILL.md"), "x").unwrap();
        assert_eq!(resolve_skill_dir(d.path()).unwrap(), d.path());
        // 单层父目录：唯一子目录有 SKILL.md
        let d2 = tempdir().unwrap();
        let child = d2.path().join("pkg");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("SKILL.md"), "x").unwrap();
        assert_eq!(resolve_skill_dir(d2.path()).unwrap(), child);
        // 多义：两个子目录都有 SKILL.md
        let d3 = tempdir().unwrap();
        for n in ["a", "b"] {
            let c = d3.path().join(n);
            std::fs::create_dir_all(&c).unwrap();
            std::fs::write(c.join("SKILL.md"), "x").unwrap();
        }
        assert!(matches!(
            resolve_skill_dir(d3.path()),
            Err(SkillkitError::AmbiguousSkillArchive { .. })
        ));
        // 无 SKILL.md
        let d4 = tempdir().unwrap();
        std::fs::create_dir_all(d4.path().join("x")).unwrap();
        assert!(matches!(
            resolve_skill_dir(d4.path()),
            Err(SkillkitError::InvalidLocalSkill { .. })
        ));
    }
}
