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

use sha2::{Digest, Sha256};
use std::io::Read;

/// 递归收集目录下所有非 symlink 文件（相对路径）。symlink 不参与（对齐 import.rs 约定）。
#[allow(dead_code)]
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if std::fs::symlink_metadata(&p)?.file_type().is_symlink() {
            continue; // 跳过 symlink，防池外内容入 hash
        }
        if p.is_dir() {
            collect_files(root, &p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// 确定性 sha256（长度前缀防碰撞）：按相对路径排序，每文件写 len(path)‖path‖len(content)‖content。
#[allow(dead_code)]
pub(crate) fn hash_skill_dir(dir: &Path) -> Result<String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for f in &files {
        let rel = f.strip_prefix(dir).unwrap_or(f);
        let rel_bytes = rel.to_string_lossy();
        let content = std::fs::read(f)?;
        hasher.update((rel_bytes.len() as u64).to_le_bytes());
        hasher.update(rel_bytes.as_bytes());
        hasher.update((content.len() as u64).to_le_bytes());
        hasher.update(&content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 递归复制目录，跳过 symlink（防把 ~/.ssh 等池外文件拷入 canonical 池）。
#[allow(dead_code)]
pub(crate) fn copy_skill_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        if std::fs::symlink_metadata(&p)?.file_type().is_symlink() {
            continue;
        }
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_skill_dir(&p, &target)?;
        } else {
            std::fs::copy(&p, &target)?;
        }
    }
    Ok(())
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

    use sha2::{Digest, Sha256};

    fn write_tree(root: &Path, files: &[(&str, &str)]) {
        for (name, content) in files {
            let p = root.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, content).unwrap();
        }
    }

    #[test]
    fn hash_is_deterministic_and_content_sensitive() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        write_tree(a.path(), &[("SKILL.md", "x"), ("lib/y.md", "z")]);
        write_tree(b.path(), &[("SKILL.md", "x"), ("lib/y.md", "z")]);
        assert_eq!(
            hash_skill_dir(a.path()).unwrap(),
            hash_skill_dir(b.path()).unwrap()
        );
        write_tree(b.path(), &[("SKILL.md", "changed")]);
        assert_ne!(
            hash_skill_dir(a.path()).unwrap(),
            hash_skill_dir(b.path()).unwrap()
        );
    }

    #[test]
    fn hash_length_prefix_prevents_collision() {
        // 树 A {a:"bc"} 与 B {ab:"c"} 无定界会撞同一字节流；长度前缀必须让二者不同。
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        write_tree(a.path(), &[("a", "bc")]);
        write_tree(b.path(), &[("ab", "c")]);
        assert_ne!(
            hash_skill_dir(a.path()).unwrap(),
            hash_skill_dir(b.path()).unwrap()
        );
    }

    #[test]
    fn copy_skill_dir_skips_symlinks() {
        let src = tempdir().unwrap();
        let dst = tempdir().unwrap();
        std::fs::write(src.path().join("SKILL.md"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", src.path().join("evil")).unwrap();
        copy_skill_dir(src.path(), dst.path()).unwrap();
        assert!(dst.path().join("SKILL.md").exists());
        assert!(!dst.path().join("evil").exists(), "symlink 不复制");
    }
}
