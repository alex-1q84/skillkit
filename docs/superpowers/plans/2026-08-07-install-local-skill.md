# 安装本地 skill 实现计划（2026-08-07）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新增 `skillkit install local <目录|zip>`，把外部 skill 安装到 canonical 池、算 sha256 标记 managed、注册为 `local/<name>`，CLI + GUI 双端可用。

**Architecture:** core 新模块 `install_local.rs` 承载全部逻辑（路径归一 zip→tempdir / dir→直接、定位 skill 目录、校验 name、确定性 hash、symlink 拒绝、暂存+三段原子落地、持 "registry" 锁）；CLI 加 `install local` 子命令薄壳；server 加 `POST /skills/install-local` handler + 表单片段。

**Tech Stack:** Rust 2021 + clap（CLI derive）+ axum/askama（server）+ 新依赖 `zip = "2"`、`sha2 = "0.10"`（均 pure rust）。

## Global Constraints

- canonical 池：`~/.skillkit/.agents/skills/<name>/`（`Paths::skillkit_skills_dir()`），单版本扁平，跨 source 同名共享同一目录。
- id 契约 `<source>/<skill>`：本地装用伪 source `local`，id=`local/<name>`，**不进 SourcesStore**（与 `unmanaged` 对称）。
- name 校验：拒空 / `.` / `..` / 纯点串 / 含 `/` 或 `\`；字符集 `[A-Za-z0-9-_.]`；`--name` 与 frontmatter 同一校验；join 后 `target.starts_with(pool)` containment 断言兜底。
- 安全边界（输入含 GitHub zip，不可信）：zip 逐条目 `enclosed_name` 校验（拒 `..`/绝对路径/symlink 条目）；`copy_skill_dir`/`hash_skill_dir` 跳过 symlink（对齐 `import.rs:129`）；解压体积上限 `MAX_ZIP_BYTES=100MiB`、条目上限 `MAX_ZIP_ENTRIES=10000`。
- hash：长度前缀框架 `len(path)‖path‖len(content)‖content`，防碰撞。
- 原子/并发：install_local 全程持 `FileLock(paths,"registry")`（闭 lost-update）；force 三段原子 `target→.old → staging→target → rm .old`；hash 在 rename 前对 staging 算；`Registry::save` 内部会重取 "registry" 锁致**同进程自死锁**，故新增 `Registry::save_raw`（不取锁，调用方持锁）。
- 已知限制（不包）：池物理变更方 uninstall/rescope/install-add 不持锁（既有债），不与 install_local 串行。
- 路径不硬编码：`~` 用 `dirs::home_dir()` 展开。改完源码必跑 `make format && make lint`。Commit message 中文 Conventional Commits，未获指示不自动 git。

---

## File Structure

- Modify `crates/core/Cargo.toml` — 加 `zip`、`sha2` 依赖。
- Modify `crates/core/src/error.rs` — 加 `InvalidLocalSkill`/`AmbiguousSkillArchive`/`SkillPoolOccupied` 变体。
- Create `crates/core/src/install_local.rs` — `install_local()` + 私有 `expand_tilde`/`resolve_skill_dir`/`read_skill_name`/`validate_name`/`collect_files`/`hash_skill_dir`/`copy_skill_dir`/`extract_zip`。
- Modify `crates/core/src/lib.rs` — `pub mod install_local;` + re-export `install_local`。
- Modify `crates/core/src/registry.rs` — 加 `pub(crate) fn save_raw`（不取锁）。
- Modify `crates/cli/src/commands/install.rs` — `InstallSub::Local` + `run_install` 分支。
- Modify `crates/server/src/routes/mod.rs` — 注册 `POST /skills/install-local`。
- Modify `crates/server/src/routes/skills.rs` — `install_local` handler + `InstallLocalForm`。
- Create `crates/server/templates/fragments/install_local_form.html` — 表单片段。
- Modify `crates/server/templates/fragments/skills_main.html` — 工具栏加「安装本地」入口按钮 + 挂载点。

---

### Task 1: core 依赖 + error 变体

**Files:**
- Modify: `crates/core/Cargo.toml`
- Modify: `crates/core/src/error.rs`
- Test: `crates/core/src/error.rs`（内联 unit test）

**Interfaces:**
- Produces: `SkillkitError::InvalidLocalSkill { path: String, reason: String }`、`SkillkitError::AmbiguousSkillArchive { reason: String }`、`SkillkitError::SkillPoolOccupied { name: String, owner_id: Option<String> }`。

- [ ] **Step 1: 加依赖**

`crates/core/Cargo.toml` 的 `[dependencies]` 末尾加（`tempfile` 已在 dev-dependencies）：

```toml
zip = "2"
sha2 = "0.10"
```

- [ ] **Step 2: 写失败测试**

`crates/core/src/error.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn local_skill_errors_guide_action() {
        let a = SkillkitError::InvalidLocalSkill {
            path: "/x".into(),
            reason: "未找到 SKILL.md".into(),
        };
        assert!(a.to_string().contains("SKILL.md"));
        let b = SkillkitError::SkillPoolOccupied {
            name: "foo".into(),
            owner_id: Some("skills.sh/foo".into()),
        };
        assert!(b.to_string().contains("skills.sh/foo"));
        let c = SkillkitError::SkillPoolOccupied {
            name: "foo".into(),
            owner_id: None,
        };
        assert!(c.to_string().contains("孤儿") || c.to_string().contains("foo"));
    }
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib error::tests`
Expected: 编译失败（变体不存在）。

- [ ] **Step 4: 加变体**

`crates/core/src/error.rs` 的 `enum SkillkitError` 内，`UpgradeBlocked` 之后加：

```rust
    #[error("本地 skill 无效：{path}（{reason}）")]
    InvalidLocalSkill { path: String, reason: String },

    #[error("skill 归档结构不明确：{reason}（请直接传 skill 目录路径）")]
    AmbiguousSkillArchive { reason: String },

    #[error(
        "目录 {name} 已被占用：{owner}（先 skillkit skill remove <owner> 再装，或手动删除该目录）",
        owner = owner_id.as_deref().unwrap_or("无 registry 记录的孤儿目录")
    )]
    SkillPoolOccupied {
        name: String,
        owner_id: Option<String>,
    },
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib error::tests`
Expected: PASS。

- [ ] **Step 6: format + lint + commit**

```bash
make format
make lint
git add crates/core/Cargo.toml crates/core/Cargo.lock crates/core/src/error.rs
git commit -m "feat(core): install_local 错误变体 + zip/sha2 依赖"
```

---

### Task 2: 纯函数（validate_name / read_skill_name / resolve_skill_dir）

**Files:**
- Create: `crates/core/src/install_local.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/src/install_local.rs`（内联 unit test）

**Interfaces:**
- Produces: `fn validate_name(name: &str) -> Result<()>`、`fn read_skill_name(skill_md: &Path) -> Result<Option<String>>`、`fn resolve_skill_dir(src: &Path) -> Result<PathBuf>`。

- [ ] **Step 1: lib.rs 挂模块**

`crates/core/src/lib.rs` 的 `pub mod upgrade;` 后加：

```rust
pub mod install_local;
```

- [ ] **Step 2: 写失败测试**

新建 `crates/core/src/install_local.rs`：

```rust
//! 安装本地 skill（目录/zip）到 canonical 池，managed + scope=local。
//! 不可信输入（含 GitHub zip）：name 防逃逸、zip 防穿越、symlink 跳过、体积上限、三段原子落地。
use crate::error::{Result, SkillkitError};
use std::path::{Path, PathBuf};

const MAX_ZIP_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 10_000;

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
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 编译失败（函数未定义）。

- [ ] **Step 4: 实现**

在 `crates/core/src/install_local.rs` 的 `tests` mod 之上加实现：

```rust
/// 校验 skill 名（防 canonical 池路径逃逸）。拒空 / `.` / `..` / 纯点串 / 含分隔符 / 非法字符。
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
pub(crate) fn read_skill_name(skill_md: &Path) -> Result<Option<String>> {
    let content = std::fs::read_to_string(skill_md).map_err(|_| SkillkitError::InvalidLocalSkill {
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
            let v = rest
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .trim();
            if !v.is_empty() {
                return Ok(Some(v.to_string()));
            }
        }
    }
    Ok(None)
}

/// 定位 skill 目录：根有 SKILL.md → 根；唯一子目录有 SKILL.md → 该子目录；否则报错。
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
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 4 tests PASS。

- [ ] **Step 6: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/install_local.rs crates/core/src/lib.rs
git commit -m "feat(core): install_local 纯函数（name 校验/读名/定位 skill 目录）"
```

---

### Task 3: hash_skill_dir（长度前缀）+ copy_skill_dir（跳 symlink）

**Files:**
- Modify: `crates/core/src/install_local.rs`
- Test: 同文件内联（含碰撞对抗）

**Interfaces:**
- Produces: `fn hash_skill_dir(dir: &Path) -> Result<String>`、`fn copy_skill_dir(src: &Path, dst: &Path) -> Result<()>`。

- [ ] **Step 1: 写失败测试**

`install_local.rs` 的 `mod tests` 内追加：

```rust
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
        assert_eq!(hash_skill_dir(a.path()).unwrap(), hash_skill_dir(b.path()).unwrap());
        write_tree(b.path(), &[("SKILL.md", "changed")]);
        assert_ne!(hash_skill_dir(a.path()).unwrap(), hash_skill_dir(b.path()).unwrap());
    }

    #[test]
    fn hash_length_prefix_prevents_collision() {
        // 树 A {a:"bc"} 与 B {ab:"c"} 无定界会撞同一字节流；长度前缀必须让二者不同。
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        write_tree(a.path(), &[("a", "bc")]);
        write_tree(b.path(), &[("ab", "c")]);
        assert_ne!(hash_skill_dir(a.path()).unwrap(), hash_skill_dir(b.path()).unwrap());
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 编译失败（函数未定义；`sha2` 已在 Task1 加）。

- [ ] **Step 3: 实现**

`install_local.rs` 实现区追加：

```rust
use sha2::{Digest, Sha256};
use std::io::Read;

/// 递归收集目录下所有非 symlink 文件（相对路径）。symlink 不参与（对齐 import.rs 约定）。
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
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 3 new tests PASS。

- [ ] **Step 5: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/install_local.rs
git commit -m "feat(core): install_local hash（长度前缀）+ copy（跳 symlink）"
```

---

### Task 4: extract_zip（enclosed_name + 体积上限）

**Files:**
- Modify: `crates/core/src/install_local.rs`
- Test: 同文件内联（含 ZipSlip / zip bomb）

**Interfaces:**
- Produces: `fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()>`（用模块常量 `MAX_ZIP_BYTES`/`MAX_ZIP_ENTRIES`）。

- [ ] **Step 1: 写失败测试**

`install_local.rs` 的 `mod tests` 内追加（用 `zip` crate 在内存构造测试 zip）：

```rust
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    use std::io::{Seek, Write};

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zw = ZipWriter::new(buf);
        for (name, data) in entries {
            zw.start_file(name, SimpleFileOptions::default()).unwrap();
            zw.write_all(data).unwrap();
        }
        let cur = zw.finish().unwrap();
        cur.into_inner()
    }

    #[test]
    fn extract_zip_normal_layouts() {
        for (entries, expect_file) in [
            (vec![("foo/SKILL.md", b"---\nname: foo\n---\n" as &[u8])], "foo/SKILL.md"),
            (vec![("SKILL.md", b"x")], "SKILL.md"),
        ] {
            let z = build_zip(&entries);
            let tmp = tempdir().unwrap();
            let zp = tmp.path().join("a.zip");
            std::fs::write(&zp, &z).unwrap();
            let out = tempdir().unwrap();
            extract_zip(&zp, out.path()).unwrap();
            assert!(out.path().join(expect_file).exists(), "应解出 {expect_file}");
        }
    }

    #[test]
    fn extract_zip_rejects_traversal() {
        let z = build_zip(&[("../evil.txt", b"x")]);
        let tmp = tempdir().unwrap();
        let zp = tmp.path().join("a.zip");
        std::fs::write(&zp, &z).unwrap();
        let out = tempdir().unwrap();
        assert!(extract_zip(&zp, out.path()).is_err(), "路径穿越条目必须拒");
        assert!(!tmp.path().join("evil.txt").exists());
    }

    #[test]
    fn extract_zip_rejects_bomb() {
        // 超条目上限（MAX_ZIP_ENTRIES=10000）：造 10001 个条目
        let buf = std::io::Cursor::new(Vec::new());
        let mut zw = ZipWriter::new(buf);
        for i in 0..=MAX_ZIP_ENTRIES {
            zw.start_file(&format!("f{i}"), SimpleFileOptions::default()).unwrap();
            zw.write_all(b"x").unwrap();
        }
        let z = zw.finish().unwrap().into_inner();
        let tmp = tempdir().unwrap();
        let zp = tmp.path().join("a.zip");
        std::fs::write(&zp, &z).unwrap();
        let out = tempdir().unwrap();
        assert!(extract_zip(&zp, out.path()).is_err(), "超条目上限拒");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 编译失败（`extract_zip` 未定义）。

- [ ] **Step 3: 实现**

`install_local.rs` 实现区追加（`std::io::Read` 已在 Task3 引入）：

```rust
/// 解压 zip 到 dest，逐条目安全校验：enclosed_name（拒 `..`/绝对路径）+ 拒 symlink 条目
/// + 总体积/条目上限（防 zip bomb）。对齐 spec §3.6。
pub(crate) fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path).map_err(|_| SkillkitError::InvalidLocalSkill {
        path: zip_path.display().to_string(),
        reason: "zip 不可读".into(),
    })?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SkillkitError::InvalidLocalSkill {
            path: zip_path.display().to_string(),
            reason: format!("解压失败：{e}（文件损坏或非 zip）"),
        })?;
    std::fs::create_dir_all(dest)?;
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        if i + 1 > MAX_ZIP_ENTRIES {
            return Err(SkillkitError::InvalidLocalSkill {
                path: zip_path.display().to_string(),
                reason: "文件数超上限（疑似 zip bomb）".into(),
            });
        }
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SkillkitError::InvalidLocalSkill {
                path: zip_path.display().to_string(),
                reason: e.to_string(),
            })?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(SkillkitError::InvalidLocalSkill {
                path: zip_path.display().to_string(),
                reason: format!("含不安全条目（路径穿越）：{}", entry.name()),
            });
        };
        let outpath = dest.join(enclosed);
        if !outpath.starts_with(dest) {
            return Err(SkillkitError::InvalidLocalSkill {
                path: zip_path.display().to_string(),
                reason: format!("条目越界：{}", entry.name()),
            });
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = entry.unix_permissions().unwrap_or(0o644);
            if mode & 0o170000 == 0o120000 {
                return Err(SkillkitError::InvalidLocalSkill {
                    path: zip_path.display().to_string(),
                    reason: format!("含 symlink 条目：{}", entry.name()),
                });
            }
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            let mut buf = [0u8; 8192];
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                if total > MAX_ZIP_BYTES {
                    return Err(SkillkitError::InvalidLocalSkill {
                        path: zip_path.display().to_string(),
                        reason: "解压体积超上限（疑似 zip bomb）".into(),
                    });
                }
                outfile.write_all(&buf[..n])?;
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 3 new tests PASS。

- [ ] **Step 5: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/install_local.rs
git commit -m "feat(core): install_local zip 解压（enclosed_name + 体积上限）"
```

---

### Task 5: Registry::save_raw + install_local 编排 + 集成测试

**Files:**
- Modify: `crates/core/src/registry.rs`
- Modify: `crates/core/src/install_local.rs`
- Modify: `crates/core/src/lib.rs`（re-export）
- Test: `crates/core/src/install_local.rs`（内联集成）

**Interfaces:**
- Produces: `pub fn install_local(paths: &Paths, src_path: &str, name: Option<&str>, scope: Scope, force: bool) -> Result<SkillMeta>`。
- Consumes: `crate::install::now_iso()`、`crate::symlink::ensure_global_claude`、`crate::lock::FileLock`、`Registry::{load,save_raw,upsert,skill_id,get}`。

- [ ] **Step 1: Registry::save_raw**

`crates/core/src/registry.rs` 的 `impl Registry` 内 `save` 方法后加（不取锁，调用方持 "registry"）：

```rust
    /// 写 registry，不获取锁（调用方须已持 "registry" 锁）。供持锁全流程的调用方用，
    /// 避免 install_local 已持锁时 Registry::save 再取同 key 致同进程 flock 自死锁。
    pub(crate) fn save_raw(&self, paths: &Paths) -> Result<()> {
        let path = paths.registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::error::atomic_write(&path, &serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
```

- [ ] **Step 2: 写失败测试**

`install_local.rs` 的 `mod tests` 内追加集成测试：

```rust
    use crate::install_local;
    use crate::paths::Paths;
    use crate::registry::{Registry, Scope, SkillMeta};

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    fn make_skill_dir(parent: &Path, name: &str) -> PathBuf {
        let d = parent.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: x\n---\n# {name}\n"),
        )
        .unwrap();
        d
    }

    #[test]
    fn install_local_dir_lands_managed_local() {
        let p = paths();
        let src = tempdir().unwrap();
        let d = make_skill_dir(src.path(), "foo");
        let meta = install_local(&p, &d.display().to_string(), None, Scope::Local, false).unwrap();
        assert_eq!(meta.id, "local/foo");
        assert_eq!(meta.source, "local");
        assert_eq!(meta.scope, Scope::Local);
        assert!(meta.computed_hash.is_some(), "managed（有 hash）");
        assert!(p.skillkit_skills_dir().join("foo").join("SKILL.md").exists());
        // 无 global symlink
        assert!(!p.claude_skills_dir().exists());
        assert_eq!(Registry::load(&p).unwrap().get("local/foo").unwrap().name, "foo");
    }

    #[test]
    fn install_local_name_override_and_frontmatter_escape() {
        let p = paths();
        let src = tempdir().unwrap();
        let d = make_skill_dir(src.path(), "foo");
        // --name 覆盖
        let m = install_local(&p, &d.display().to_string(), Some("renamed"), Scope::Local, false).unwrap();
        assert_eq!(m.id, "local/renamed");
        // frontmatter 恶意 name（..）也拒
        let src2 = tempdir().unwrap();
        let bad = src2.path().join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("SKILL.md"), "---\nname: ..\n---\n").unwrap();
        assert!(install_local(&p, &bad.display().to_string(), None, Scope::Local, false).is_err());
    }

    #[test]
    fn install_local_conflict_and_force() {
        let p = paths();
        let src = tempdir().unwrap();
        let d = make_skill_dir(src.path(), "foo");
        install_local(&p, &d.display().to_string(), None, Scope::Local, false).unwrap();
        // 重复装（无 force）→ SkillAlreadyInstalled
        assert!(matches!(
            install_local(&p, &d.display().to_string(), None, Scope::Local, false),
            Err(SkillkitError::SkillAlreadyInstalled { .. })
        ));
        // force 覆盖
        std::fs::write(d.join("extra.md"), "changed").unwrap();
        let m = install_local(&p, &d.display().to_string(), None, Scope::Local, true).unwrap();
        assert!(p.skillkit_skills_dir().join("foo").join("extra.md").exists());
        // 无 .old/.staging 残留
        let leftover: Vec<_> = std::fs::read_dir(p.skillkit_skills_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".foo"))
            .collect();
        assert!(leftover.is_empty(), "force 后无暂存/old 残留：{leftover:?}");
        let _ = m;
    }

    #[test]
    fn install_local_refuses_cross_source_occupant() {
        let p = paths();
        // 模拟 skills.sh/foo 已占池 skills/foo
        let canon = p.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let mut reg = Registry::load(&p).unwrap();
        reg.upsert(SkillMeta {
            id: "skills.sh/foo".into(),
            name: "foo".into(),
            source: "skills.sh".into(),
            scope: Scope::Local,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "x".into(),
            canonical_path: canon.display().to_string(),
        });
        reg.save(&p).unwrap();

        let src = tempdir().unwrap();
        let d = make_skill_dir(src.path(), "foo");
        let err = install_local(&p, &d.display().to_string(), None, Scope::Local, false).unwrap_err();
        assert!(matches!(err, SkillkitError::SkillPoolOccupied { .. }));
        // force 也不跨 source 删
        let err2 = install_local(&p, &d.display().to_string(), None, Scope::Local, true).unwrap_err();
        assert!(matches!(err2, SkillkitError::SkillPoolOccupied { .. }));
        assert!(canon.join("SKILL.md").exists(), "skills.sh/foo 的 canonical 未被删");
    }

    #[test]
    fn install_local_orphan_target_no_owner() {
        let p = paths();
        // target 存在但无 registry 记录（孤儿）
        let canon = p.skillkit_skills_dir().join("foo");
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let src = tempdir().unwrap();
        let d = make_skill_dir(src.path(), "foo");
        let err = install_local(&p, &d.display().to_string(), None, Scope::Local, false).unwrap_err();
        match err {
            SkillkitError::SkillPoolOccupied { owner_id, .. } => {
                assert!(owner_id.is_none(), "孤儿目录 owner_id=None");
            }
            other => panic!("应为 SkillPoolOccupied(None)：{other:?}"),
        }
    }

    #[test]
    fn install_local_preserves_other_registry_entries() {
        // lost-update 防护：install_local 不覆盖 registry 其他 skill 条目
        let p = paths();
        let mut reg = Registry::load(&p).unwrap();
        reg.upsert(SkillMeta {
            id: "skills.sh/bar".into(),
            name: "bar".into(),
            source: "skills.sh".into(),
            scope: Scope::Local,
            version: None,
            computed_hash: Some("x".into()),
            installed_at: "x".into(),
            canonical_path: p.skillkit_skills_dir().join("bar").display().to_string(),
        });
        reg.save(&p).unwrap();

        let src = tempdir().unwrap();
        let d = make_skill_dir(src.path(), "foo");
        install_local(&p, &d.display().to_string(), None, Scope::Local, false).unwrap();

        let after = Registry::load(&p).unwrap();
        assert!(after.get("skills.sh/bar").is_ok(), "其他 skill 条目保留");
        assert!(after.get("local/foo").is_ok());
    }
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 编译失败（`install_local` 未定义）。

- [ ] **Step 4: 实现 install_local**

`install_local.rs` 实现区追加：

```rust
use crate::lock::FileLock;
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};

/// 展开 `~` / `~/x` 到 home（dirs::home_dir）。其余原样返回。
fn expand_tilde(p: &str) -> String {
    if p == "~" {
        return dirs::home_dir().map(|h| h.display().to_string()).unwrap_or_else(|| p.into());
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    p.into()
}

/// 安装本地 skill（目录/zip）到 canonical 池，managed + scope。全程持 "registry" 锁。
pub fn install_local(
    paths: &Paths,
    src_path: &str,
    name: Option<&str>,
    scope: Scope,
    force: bool,
) -> Result<SkillMeta> {
    let _lock = FileLock::acquire(paths, "registry")?;

    let expanded = expand_tilde(src_path);
    let src = Path::new(&expanded);
    if !src.exists() {
        return Err(SkillkitError::InvalidLocalSkill {
            path: src_path.into(),
            reason: "路径不存在（需是含 SKILL.md 的目录或 .zip）".into(),
        });
    }

    // zip → 解压到 tempdir；目录 → 直接用。_zip_tmp 保活到复制完成。
    let (_zip_tmp, root) = if src.is_file()
        && src.extension().and_then(|e| e.to_str()) == Some("zip")
    {
        let tmp = tempfile::TempDir::new()?;
        extract_zip(src, tmp.path())?;
        (Some(tmp), tmp.path().to_path_buf())
    } else if src.is_dir() {
        (None, src.to_path_buf())
    } else {
        return Err(SkillkitError::InvalidLocalSkill {
            path: src_path.into(),
            reason: "需是含 SKILL.md 的目录或 .zip".into(),
        });
    };

    let skill_dir = resolve_skill_dir(&root)?;

    let resolved_name = match name {
        Some(n) => n.to_string(),
        None => read_skill_name(&skill_dir.join("SKILL.md"))?.ok_or_else(|| {
            SkillkitError::InvalidLocalSkill {
                path: src_path.into(),
                reason: "SKILL.md 缺 name 字段且未传 --name".into(),
            }
        })?,
    };
    validate_name(&resolved_name)?;

    let pool = paths.skillkit_skills_dir();
    let target = pool.join(&resolved_name);
    // containment 断言兜底（即便校验有遗漏也不让 target 落池外）
    if !target.starts_with(&pool) || resolved_name.contains("..") {
        return Err(SkillkitError::InvalidLocalSkill {
            path: src_path.into(),
            reason: "target 越界".into(),
        });
    }

    let local_id = Registry::skill_id("local", &resolved_name);
    let reg = Registry::load(paths)?;
    let existing_local = reg.get(&local_id).ok().cloned();
    let other_owner = reg
        .skills
        .values()
        .find(|m| m.id != local_id && PathBuf::from(&m.canonical_path) == target)
        .cloned();

    // 冲突判定（键 = registry id；防跨 source 误删）
    if let Some(owner) = &other_owner {
        return Err(SkillkitError::SkillPoolOccupied {
            name: resolved_name.clone(),
            owner_id: Some(owner.id.clone()),
        });
    }
    if existing_local.is_some() && !force {
        return Err(SkillkitError::SkillAlreadyInstalled { id: local_id });
    }
    if existing_local.is_none() && target.exists() {
        return Err(SkillkitError::SkillPoolOccupied {
            name: resolved_name.clone(),
            owner_id: None,
        }); // 孤儿目录
    }

    // 复制到暂存 + hash（rename 前算，缩小就位后失败面）
    let pid = std::process::id();
    let staging = pool.join(format!(".{resolved_name}.staging-{pid}"));
    let old = pool.join(format!(".{resolved_name}.old-{pid}"));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    copy_skill_dir(&skill_dir, &staging)?;
    let hash = hash_skill_dir(&staging)?;

    // 原子就位：force 三段（target→.old → staging→target），非 force 单段 rename。
    let force_mode = existing_local.is_some();
    if force_mode {
        std::fs::rename(&target, &old).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging);
            SkillkitError::Io(e)
        })?;
        if let Err(e) = std::fs::rename(&staging, &target) {
            let _ = std::fs::rename(&old, &target); // 还原旧内容
            return Err(SkillkitError::Io(e));
        }
    } else if let Err(e) = std::fs::rename(&staging, &target) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(SkillkitError::Io(e));
    }

    // registry upsert + save_raw（持同一把锁，不重取）
    let meta = SkillMeta {
        id: local_id,
        name: resolved_name.clone(),
        source: "local".into(),
        scope,
        version: None,
        computed_hash: Some(hash),
        installed_at: crate::install::now_iso(),
        canonical_path: target.display().to_string(),
    };
    let mut reg = reg;
    reg.upsert(meta.clone());
    if let Err(e) = reg.save_raw(paths) {
        // 回滚：非 force 删 target；force 还原 old
        let _ = std::fs::remove_dir_all(&target);
        if force_mode {
            let _ = std::fs::rename(&old, &target);
        }
        return Err(e);
    }
    if force_mode {
        let _ = std::fs::remove_dir_all(&old); // 清旧（best-effort）
    }

    if scope == Scope::Global {
        crate::symlink::ensure_global_claude(paths, &meta)?;
    }
    Ok(meta)
}
```

- [ ] **Step 5: lib.rs re-export**

`crates/core/src/lib.rs` 的 `pub use install::{install, uninstall};` 后加：

```rust
pub use install_local::install_local;
```

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib install_local`
Expected: 6 new tests PASS（含 cross-source、orphan、lost-update、force 无残留）。

- [ ] **Step 7: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/install_local.rs crates/core/src/registry.rs crates/core/src/lib.rs
git commit -m "feat(core): install_local 编排（锁/三段原子/归属反查/孤儿）+ save_raw"
```

---

### Task 6: CLI `install local` 子命令

**Files:**
- Modify: `crates/cli/src/commands/install.rs`
- Test: 同文件内联

**Interfaces:**
- Consumes: `skillkit_core::install_local`。

- [ ] **Step 1: 写失败测试**

`crates/cli/src/commands/install.rs` 的 `mod tests` 内追加：

```rust
    #[derive(serde::Serialize)]
    struct MetaShape {
        id: String,
        source: String,
        scope: String,
        computed_hash: Option<String>,
        canonical_path: String,
    }

    #[test]
    fn install_local_parses_flags() {
        let TestCli { cmd } = TestCli::parse_from([
            "skillkit", "local", "./foo", "--name", "bar", "--scope", "global", "--force", "--json",
        ]);
        match cmd {
            InstallSub::Local {
                path,
                name,
                scope,
                force,
                json,
            } => {
                assert_eq!(path, "./foo");
                assert_eq!(name.as_deref(), Some("bar"));
                assert_eq!(scope, Scope::Global);
                assert!(force);
                assert!(json);
            }
            _ => panic!("应为 Local"),
        }
    }

    #[test]
    fn install_local_json_schema_locks_fields() {
        let m = MetaShape {
            id: "local/foo".into(),
            source: "local".into(),
            scope: "local".into(),
            computed_hash: Some("abc".into()),
            canonical_path: "/x/foo".into(),
        };
        let j = serde_json::to_string(&m).unwrap();
        for f in ["\"id\"", "\"source\"", "\"scope\"", "\"computed_hash\"", "\"canonical_path\""] {
            assert!(j.contains(f), "json schema 应含 {f}：{j}");
        }
    }
```

> 说明：`install_local_parses_flags` 里 `TestCli` 当前只解析 `Add`；Step3 改 `InstallSub` 加 `Local` 后 `TestCli`（与 `InstallSub` 同形）自动覆盖 `Local`，断言生效。先把 `try_into().unwrap_or_else` 那行写成普通 `TestCli::parse_from(...)`（删 try_into），见 Step3 后的修正。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-cli --lib install`
Expected: 编译失败（`InstallSub::Local` 不存在）。

- [ ] **Step 3: 加 Local 变体 + run_install 分支**

`crates/cli/src/commands/install.rs`：

`enum InstallSub` 加 `Local` 变体（在 `Add { ... }` 后）：

```rust
    /// 安装本地 skill：skillkit install local <目录|zip> [--name N] [--scope global|local] [--force] [--json]
    Local {
        /// skill 目录或 .zip 路径（支持 ~/）
        path: String,
        /// 覆盖 skill 名（默认读 SKILL.md frontmatter name）
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_parser = parse_scope, default_value = "local")]
        scope: Scope,
        /// 覆盖已存在的 local/<name>
        #[arg(long)]
        force: bool,
        /// JSON 输出 SkillMeta
        #[arg(long)]
        json: bool,
    },
```

`run_install` 的 `match cmd.cmd` 加分支（`Add { ... } =>` 块后）：

```rust
        InstallSub::Local {
            path,
            name,
            scope,
            force,
            json,
        } => {
            let meta = skillkit_core::install_local(&paths, &path, name.as_deref(), scope, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&meta)?);
            } else {
                let short = meta
                    .computed_hash
                    .as_deref()
                    .map(|h| h.chars().take(12).collect::<String>())
                    .unwrap_or_else(|| "?".into());
                println!(
                    "✓ 已安装 {} → {}（sha256: {short}）",
                    meta.id,
                    meta.canonical_path
                );
            }
        }
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p skillkit-cli --lib install`
Expected: 2 tests PASS。

- [ ] **Step 5: 全量 check + commit**

```bash
make check
git add crates/cli/src/commands/install.rs
git commit -m "feat(cli): install local 子命令（目录/zip，--name/--scope/--force/--json）"
```

---

### Task 7: server handler + 路由 + 表单片段

**Files:**
- Modify: `crates/server/src/routes/mod.rs`
- Modify: `crates/server/src/routes/skills.rs`
- Create: `crates/server/templates/fragments/install_local_form.html`
- Modify: `crates/server/templates/fragments/skills_main.html`
- Test: `crates/server/tests/routes.rs`

**Interfaces:**
- Consumes: `skillkit_core::install_local`。
- Produces: `POST /{token}/skills/install-local`（form: path/name/scope/force）→ 成功返回完整 Skills 页，失败 `error_response`。

- [ ] **Step 1: 写失败测试**

`crates/server/tests/routes.rs` 内追加（参照 `skills_install_candidate_registers_skill` 范式）：

```rust
#[tokio::test]
async fn skills_install_local_lands_dir() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 造一个 skill 目录在 fakehome 之外（放 dir 的兄弟）
    let skill_dir = dir.path().join("myskill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: myskill\n---\n# myskill\n",
    )
    .unwrap();
    let app = skillkit_server::app(state.clone());
    let body = format!("path={}&scope=local", skill_dir.display());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/install-local")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reg = skillkit_core::Registry::load(&state.paths).unwrap();
    let m = reg.get("local/myskill").expect("应登记 local/myskill");
    assert_eq!(m.scope, skillkit_core::Scope::Local);
    assert!(m.computed_hash.is_some());
}

#[tokio::test]
async fn skills_install_local_conflict_returns_error_json() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 先占 skills.sh/foo
    let canon = state.paths.skillkit_skills_dir().join("foo");
    std::fs::create_dir_all(&canon).unwrap();
    std::fs::write(canon.join("SKILL.md"), "x").unwrap();
    use skillkit_core::{Registry, SkillMeta};
    let mut reg = Registry::load(&state.paths).unwrap();
    reg.upsert(SkillMeta {
        id: "skills.sh/foo".into(),
        name: "foo".into(),
        source: "skills.sh".into(),
        scope: skillkit_core::Scope::Local,
        version: None,
        computed_hash: Some("a".into()),
        installed_at: "x".into(),
        canonical_path: canon.display().to_string(),
    });
    reg.save(&state.paths).unwrap();

    let skill_dir = dir.path().join("foo");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "---\nname: foo\n---\n").unwrap();
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/install-local")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(format!(
                    "path={}&scope=local",
                    skill_dir.display()
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY); // error_response 422
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-server --test routes skills_install_local`
Expected: 编译失败（路由/handler 不存在）。

- [ ] **Step 3: 注册路由**

`crates/server/src/routes/mod.rs` 的 `protected()` 内，`.route("/{token}/skills/import", post(skills::import))` 后加：

```rust
        .route(
            "/{token}/skills/install-local",
            get(skills::install_local_form).post(skills::install_local),
        )
```

- [ ] **Step 4: handler + 表单模板**

`crates/server/src/routes/skills.rs`：handler 内对 core 的 `install_local` 用全限定 `skillkit_core::install_local(...)` 调用（与本地 handler 同名但经 `skillkit_core::` 限定不冲突），`use` 区不动。

在 `RescopeGuiQuery` 结构后、`render_str` 前加表单结构 + 两个 handler：

```rust
#[derive(Deserialize)]
pub struct InstallLocalForm {
    pub path: String,
    pub name: Option<String>,
    pub scope: Option<String>,
    pub force: Option<String>,
}

#[derive(Template)]
#[template(path = "fragments/install_local_form.html")]
pub struct InstallLocalFormTpl<'a> {
    pub token: &'a str,
}

/// 「安装本地」按钮 hx-get 拉取表单片段，挂到挂载点。
pub async fn install_local_form(
    State(_state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    render_str(InstallLocalFormTpl { token: &token }.render())
}

/// POST 安装本地 skill（目录/zip）。成功返回完整 Skills 页，失败 error_response（toast）。
pub async fn install_local(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<InstallLocalForm>,
) -> Response {
    let scope = if matches!(f.scope.as_deref(), Some("global")) {
        Scope::Global
    } else {
        Scope::Local
    };
    let force = matches!(f.force.as_deref(), Some("on") | Some("true") | Some("1"));
    match skillkit_core::install_local(&state.paths, &f.path, f.name.as_deref(), scope, force) {
        Ok(_) => render_skills(
            state,
            token,
            Some(&format!("✓ 已安装本地 skill：{}", f.path)),
            false,
            vec![],
            vec![],
        ),
        Err(e) => {
            tracing::error!(error = ?e, "install-local 失败：{}", f.path);
            error_response(format!("安装失败：{e}"))
        }
    }
}
```

新建 `crates/server/templates/fragments/install_local_form.html`：

```html
<form class="install-local-form"
      hx-post="/{{ token }}/skills/install-local"
      hx-target="body" hx-swap="outerHTML">
  <label>路径（目录或 .zip，支持 ~/）
    <input type="text" name="path" required placeholder="~/skills/my-skill 或 ./pkg.zip" />
  </label>
  <label>name（可选，默认读 SKILL.md）
    <input type="text" name="name" />
  </label>
  <label>scope
    <select name="scope">
      <option value="local" selected>local</option>
      <option value="global">global</option>
    </select>
  </label>
  <label><input type="checkbox" name="force" value="on" /> 覆盖已存在</label>
  <button type="submit" class="pill-btn">安装</button>
  <button type="button" onclick="this.closest('.install-local-panel').innerHTML=''"
          class="pill-btn">取消</button>
</form>
```

- [ ] **Step 5: skills_main.html 加入口**

读 `crates/server/templates/fragments/skills_main.html`，在 `.toolbar`（页头动作行）内「导入存量 skill」按钮旁加挂载点 + 按钮：

```html
<span class="install-local-panel" id="install-local-panel"></span>
<button class="pill-btn"
        hx-get="/{{ token }}/skills/install-local"
        hx-target="#install-local-panel"
        hx-swap="innerHTML">安装本地</button>
```

（精确插入点：`.toolbar` 内现有动作按钮同级。先 Read 该文件确认 `.toolbar` 结构再插入。）

- [ ] **Step 6: 跑测试确认通过**

Run: `cargo test -p skillkit-server --test routes skills_install_local`
Expected: 2 tests PASS。

- [ ] **Step 7: 全量 check + commit**

```bash
make check
git add crates/server/src/routes/mod.rs crates/server/src/routes/skills.rs \
        crates/server/templates/fragments/install_local_form.html \
        crates/server/templates/fragments/skills_main.html
git commit -m "feat(server): install-local 端点 + 表单浮层（CLI+GUI 本地装 skill）"
```

---

## Self-Review

**Spec coverage（逐节核对）：**
- §3.1 source/id `local/<name>` 不进 SourcesStore → Task5（`source: "local"`、不调 SourcesStore）✓
- §3.2 数据流（zip→tempdir / resolve / name / containment / registry-id 冲突 / 归属反查 / 暂存 / hash 前置 / 三段原子 / save_raw / 两段回滚 / global symlink）→ Task5 ✓
- §3.3 布局兼容（根 / 单层父目录 / 多义）→ Task2 `resolve_skill_dir` ✓
- §3.4 name 校验 + containment → Task2 + Task5 ✓
- §3.5 hash 长度前缀 + version None → Task3 + Task5 ✓
- §3.6 zip enclosed_name / symlink / 体积上限 → Task4 + Task3（copy/hash 跳 symlink）✓
- §3.7 锁 key="registry" + save_raw 防自死锁 + 已知限制（池物理变更方不持锁）→ Task5 ✓
- §4 CLI 接口 → Task6 ✓
- §5 GUI 端点 + 表单 → Task7 ✓
- §6 错误（InvalidLocalSkill/AmbiguousSkillArchive/SkillPoolOccupied）→ Task1 ✓
- §7 组件 + 依赖（zip/sha2）→ Task1 + Task5 ✓
- §8 测试（resolve/read_name/validate 对抗、hash 碰撞、cross-source、orphan、zip 穿越/bomb、symlink 跳过、force 无残留、lost-update、--json schema、global symlink）→ Task2/3/4/5/6/7 ✓

**说明（非缺口，诚实记录）：**
- §3.7 force 三段中「rename 之间瞬时 target 缺席」对并发读（apply）是既有读侧不锁问题，与写侧已知限制同源，不单测。
- save_raw 失败回滚的故障注入需 hook，本计划以「force 后无 .old/.staging 残留」+「孤儿检测」+「cross-source 不误删」三个可观测契约覆盖，回滚代码路径简单（remove_dir_all + rename 还原）。

**Placeholder scan：** 无 TBD/TODO；Task7 Step5 的模板插入点要求先 Read 文件确认结构（因 skills_main.html 经多轮样式改动），属实现期必读动作，非占位。

**Type consistency：** `install_local(paths, src_path: &str, name: Option<&str>, scope, force) -> Result<SkillMeta>` 在 Task5（定义）、Task6（CLI `name.as_deref()`）、Task7（handler `f.name.as_deref()`）三处签名一致 ✓；`SkillPoolOccupied { name, owner_id: Option<String> }` 在 Task1（定义）、Task5（构造）一致 ✓。
