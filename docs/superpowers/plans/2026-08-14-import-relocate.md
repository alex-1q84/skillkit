# import 存量 skill 迁入 canonical 池 实现计划（2026-08-14）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `import-existing` 把导入的 unmanaged skill 物理迁入 `~/.skillkit/.agents/skills/` 池子、原位置用 symlink 桥接取代，统一成 managed global skill 的物理模型；含历史存量补迁（relink）。

**Architecture:** 改 `crates/core/src/import.rs`（无新模块、无新依赖、无新 error 变体）：新增私有 `adopt_into_pool`（迁移+dedup）+ `relink_unmanaged`（遍历 registry 补迁存量 + 补建缺失桥接）+ 改主循环 unmanaged 分支（symlink-src 跳过 + adopt→save→桥接）。`ImportReport` 加 `relocated`/`relinked` 字段。CLI/server 仅 summary 文案（薄壳，签名不变）。

**Tech Stack:** Rust 2021，复用 `crate::symlink::ensure_global_claude`、`Registry`、`Paths`。错误走既有 `SkillkitError::Io`（裸 `?`，`#[from]`）+ `CanonicalCreate`（桥接占位）。

**Spec：** `docs/superpowers/specs/2026-08-14-import-relocate-design.md`（review 3 轮收敛通过）。

## Global Constraints

（源自 spec + CLAUDE.md，每个 task 隐含遵守）

- canonical 池：`~/.skillkit/.agents/skills/<name>/`（`Paths::skillkit_skills_dir()`），单版本扁平。迁入即归池，原位 symlink 桥接（`ensure_global_claude` 两层：池→`~/.agents/skills/<name>`→`~/.claude/skills/<name>`）。
- 只迁真实目录：src 是 symlink 一律跳过（对齐 `import.rs:129`）。`adopt_into_pool` 保持纯迁移职责，symlink 判定留调用方（主循环步骤 0 / relink 归槽判断）。
- 补桥接仅对「canonical 已在池」的 skill 执行（`canonical_path.starts_with(skillkit_skills_dir())`）；dangling/symlink 归槽跳过的不补桥接（防自指环，spec §3.3 / r2 P2-1）。
- rename 同文件系统原子（生产 `$HOME` 下、测试 tempdir 下）；跨卷 mount 返回 `EXDEV` → `SkillkitError::Io`（`error.rs:25-26` `#[from]`，裸 `?` 映射），canonical 不动。
- 顺序约束：adopt → registry save → 桥接（对齐 `install.rs:45-52`）。桥接失败时 registry 已落盘（canonical 指池），下次 import 的 relink 补建桥接收敛。
- `ImportReport` 新增 `relocated`/`relinked` 是 `--json` schema 扩展（新增非破坏），加 schema 锁定测试（CLAUDE.md:54/96）。
- 路径不硬编码；改完必跑 `make format && make lint`；提交前 `make check`。Commit message 中文 Conventional Commits，未获指示不自动 git。

## File Structure

- Modify `crates/core/src/import.rs` — `ImportReport` 加字段；新增私有 `adopt_into_pool` / `relink_unmanaged`；改主循环 unmanaged 分支；更新 + 新增内联测试。
- Modify `crates/cli/src/commands/import.rs` — summary 加 relocated/relinked 计数 + `import_json_schema_locks_fields` 测试。
- Modify `crates/server/src/routes/skills.rs` — import handler summary 加计数。
- Modify `README.md` — import 命令描述（`:94`）+ uninstall 描述（`:107`）措辞同步。

---

### Task 1: ImportReport 加 relocated/relinked 字段 + CLI/server summary + schema 锁定测试

**Files:**
- Modify: `crates/core/src/import.rs:11-21`（`ImportReport`）
- Modify: `crates/cli/src/commands/import.rs:21-27`（summary）+ tests
- Modify: `crates/server/src/routes/skills.rs:343-349`（import handler summary）

**Interfaces:**
- Produces: `ImportReport { relocated: Vec<String>, relinked: Vec<String> }`（新增两字段，`#[derive(Default)]` 自动空）。本 task 后字段始终空（Task 3/4 才填），编译过、summary 显示 0。

- [ ] **Step 1: 加字段**

`crates/core/src/import.rs:11-21` 的 `ImportReport`，`skipped` 后加：

```rust
    /// 新发现并迁入池子的 skill（主循环 unmanaged 分支 adopt）。
    pub relocated: Vec<String>,
    /// 存量补迁入池的 skill（relink_unmanaged）。
    pub relinked: Vec<String>,
```

- [ ] **Step 2: CLI summary 引用新字段**

`crates/cli/src/commands/import.rs:21-27` 的 `println!` 改为：

```rust
        println!(
            "imported {}（入池迁址 {}，含存量补迁 {}），reinstalled {}，skipped {}",
            report.imported.len(),
            report.relocated.len(),
            report.relinked.len(),
            report.reinstalled.len(),
            report.skipped.len()
        );
```

- [ ] **Step 3: server import handler summary**

`crates/server/src/routes/skills.rs:343-349` 的 `format!` 改为：

```rust
            let summary = format!(
                "imported {}（入池迁址 {}，含存量补迁 {}），reinstalled {}，skipped {}",
                r.imported.len(),
                r.relocated.len(),
                r.relinked.len(),
                r.reinstalled.len(),
                r.skipped.len()
            );
```

- [ ] **Step 4: 写 schema 锁定测试**

`crates/cli/src/commands/import.rs` 的 `mod tests` 内追加（import 命令此前缺 schema 锁定测试，本次补齐，对齐 `install.rs:228` 写法）：

```rust
    #[test]
    fn import_json_schema_locks_fields() {
        let json = serde_json::json!({
            "imported": ["foo"],
            "unmanaged": ["foo"],
            "reinstalled": [],
            "skipped": [],
            "relocated": ["foo"],
            "relinked": ["bar"],
        });
        let s = json.to_string();
        for f in [
            "\"imported\"", "\"unmanaged\"", "\"reinstalled\"",
            "\"skipped\"", "\"relocated\"", "\"relinked\"",
        ] {
            assert!(s.contains(f), "import --json schema 应含 {f}：{s}");
        }
    }
```

- [ ] **Step 5: 跑测试 + 编译**

Run: `cargo build && cargo test -p skillkit-cli --lib import`
Expected: build PASS（字段空但不报错）+ schema 测试 PASS。

- [ ] **Step 6: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/import.rs crates/cli/src/commands/import.rs crates/server/src/routes/skills.rs
git commit -m "feat(core): ImportReport 加 relocated/relinked 字段 + summary + --json schema 锁定"
```

---

### Task 2: adopt_into_pool 私有纯函数（迁移 + dedup + 幂等）

**Files:**
- Modify: `crates/core/src/import.rs`（顶部 use + 新增私有函数 + 内联单测）

**Interfaces:**
- Produces: `fn adopt_into_pool(paths: &Paths, name: &str, src: &Path) -> Result<PathBuf>`（Task 3 relink、Task 4 主循环调用）。纯迁移职责，不判 src 类型（调用方过滤 symlink）。

- [ ] **Step 1: 顶部 use 加 PathBuf**

`crates/core/src/import.rs:8` 的 `use std::path::Path;` 改为：

```rust
use std::path::{Path, PathBuf};
```

- [ ] **Step 2: 写失败测试**

`import.rs` 的 `mod tests` 内追加（在现有 `make_skill` helper 后）：

```rust
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
```

- [ ] **Step 3: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib import::tests::adopt_into_pool`
Expected: 编译失败（`adopt_into_pool` 未定义）。

- [ ] **Step 4: 实现**

`import.rs` 在 `read_git_remote` 函数后（`try_reinstall` 前）加：

```rust
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
```

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib import::tests::adopt_into_pool`
Expected: 3 tests PASS。

- [ ] **Step 6: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/import.rs
git commit -m "feat(core): import 新增 adopt_into_pool（真实目录迁池+dedup+幂等）"
```

---

### Task 3: relink_unmanaged（存量补迁 + 补建缺失桥接）+ import_existing 开头调用

**Files:**
- Modify: `crates/core/src/import.rs`（新增 `relink_unmanaged` + `import_existing` 开头调用 + 内联单测）

**Interfaces:**
- Consumes: `adopt_into_pool`（Task 2）、`crate::symlink::ensure_global_claude`、`Registry`/`SkillMeta`/`Scope`。
- Produces: `fn relink_unmanaged(paths, report: &mut ImportReport, dry_run: bool) -> Result<()>`，`import_existing` 开头调用。填 `report.relinked`。

- [ ] **Step 1: 写失败测试**

`import.rs` 的 `mod tests` 内追加：

```rust
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
        assert!(!canon.exists(), "原位置迁空");
        // 桥接建（agents 位置=原 canon，迁空后建 symlink）
        assert!(paths.agents_skills_dir().join("foo").is_symlink(), "agents 桥接");
        assert!(paths.claude_skills_dir().join("foo").is_symlink(), "claude 桥接");
        let m = Registry::load(&paths).unwrap().get("unmanaged/foo").unwrap();
        assert_eq!(m.canonical_path, pool.to_string_lossy(), "registry canonical 更新");
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
        assert!(paths.agents_skills_dir().join("foo").is_symlink(), "补建 agents 桥接");
        assert!(paths.claude_skills_dir().join("foo").is_symlink(), "补建 claude 桥接");
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
        assert!(!paths.agents_skills_dir().join("foo").exists(), "无 agents symlink");
        assert!(!paths.claude_skills_dir().join("foo").exists(), "无 claude symlink");
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
        assert!(!paths.agents_skills_dir().join("foo").is_symlink(), "dry_run 不建桥接");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib import::tests::relink`
Expected: 编译失败（`relink_unmanaged` 未定义）。

- [ ] **Step 3: 实现 relink_unmanaged**

`import.rs` 在 `adopt_into_pool` 后加：

```rust
/// 遍历 registry 的 unmanaged global skill：
/// - canonical 不在池且是真实目录 → adopt 入池 + 更新 canonical_path + 立即 save（对齐 §3.2 顺序）
/// - canonical 不在池但 dangling/symlink → warn 跳过，**不**补桥接（防自指环，spec §3.3）
/// - canonical 已在池（含刚归槽）→ 补建缺失桥接（ensure_global_claude 幂等）
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
            let target = adopt_into_pool(paths, &meta.name, canon)?;
            meta.canonical_path = target.to_string_lossy().into_owned();
            // 立即落盘（每 skill adopt 后 save，失败面可推导）
            let mut reg = Registry::load(paths)?;
            reg.upsert(meta.clone());
            reg.save(paths)?;
            report.relinked.push(meta.name.clone());
        }
        // canonical 已在池（刚归槽或本就在）：补建缺失桥接（幂等，在位跳过）
        if !dry_run {
            crate::symlink::ensure_global_claude(paths, &meta)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: import_existing 开头调用 relink**

`crates/core/src/import.rs:23-24` 的 `import_existing` 开头：

```rust
pub fn import_existing(paths: &Paths, dry_run: bool) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    relink_unmanaged(paths, &mut report, dry_run)?;
    let reg = Registry::load(paths)?;
```

（在 `let mut report = ImportReport::default();` 后、`let reg = Registry::load` 前插入 `relink_unmanaged` 调用。）

- [ ] **Step 5: 跑测试确认通过**

Run: `cargo test -p skillkit-core --lib import::tests::relink`
Expected: 5 tests PASS。

- [ ] **Step 6: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/import.rs
git commit -m "feat(core): import relink_unmanaged 存量补迁入池 + 补建缺失桥接"
```

---

### Task 4: 主循环 unmanaged 分支改造（symlink-src 跳过 + adopt→save→桥接）+ 更新现有测试

**Files:**
- Modify: `crates/core/src/import.rs:63-109`（主循环 unmanaged 分支）
- Modify: `crates/core/src/import.rs:196-242`（更新 `import_registers_unmanaged_and_skips_invalid`）
- Add: 内联测试（symlink-src 跳过、跨目录同名中断）

**Interfaces:**
- Consumes: `adopt_into_pool`（Task 2）、`crate::symlink::ensure_global_claude`、`crate::install::now_iso`。
- 填 `report.relocated`（新发现迁池）。`import_existing` 签名不变。

- [ ] **Step 1: 写失败测试（symlink-src 跳过 + 跨目录同名中断）**

`import.rs` 的 `mod tests` 内追加：

```rust
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
    fn import_cross_dir_same_name_agents_claude_aborts() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // agents/foo + claude/foo 均真实目录
        make_skill(&paths.agents_skills_dir(), "foo");
        make_skill(&paths.claude_skills_dir(), "foo");
        // agents/foo adopt 入池后建 claude 桥接撞 claude/foo 真实目录 → CanonicalCreate
        let result = import_existing(&paths, false);
        assert!(result.is_err(), "agents+claude 同名真实目录应中断");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p skillkit-core --lib import::tests::import_skips_symlink_src`
Expected: symlink src 当前走旧 unmanaged 分支（rename symlink）→ 测试 FAIL（池子出现 symlink 或行为不符）。

- [ ] **Step 3: 改造主循环 unmanaged 分支**

替换 `crates/core/src/import.rs:89-108`（`} else if !dry_run {` … `report.imported.push(name);` 这段 unmanaged 真跑+dry_run 分支）为：

```rust
        } else {
            // unmanaged 分支（无 package）
            let canon_path = Path::new(&canonical);
            // 步骤 0：symlink src → skipped + continue（dry_run 分叉前生效，防 rename symlink 产悬空 canonical）
            if std::fs::symlink_metadata(canon_path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                report
                    .skipped
                    .push(format!("{name}（symlink，只迁真实目录）"));
                continue;
            }
            if dry_run {
                registered.insert(name.clone());
                report.unmanaged.push(name.clone());
                report.relocated.push(name.clone());
                report.imported.push(name);
                continue;
            }
            // adopt → registry save → 桥接（对齐 install.rs:45-52 顺序）
            let target = adopt_into_pool(paths, &name, canon_path)?;
            let meta = SkillMeta {
                id: Registry::skill_id("unmanaged", &name),
                name: name.clone(),
                source: "unmanaged".into(),
                scope: Scope::Global,
                version: None,
                computed_hash: None,
                installed_at: crate::install::now_iso(),
                canonical_path: target.to_string_lossy().into_owned(),
            };
            let mut reg = Registry::load(paths)?;
            reg.upsert(meta.clone());
            reg.save(paths)?;
            crate::symlink::ensure_global_claude(paths, &meta)?;
            registered.insert(name.clone());
            report.unmanaged.push(name.clone());
            report.relocated.push(name.clone());
            report.imported.push(name);
            continue; // 防落末尾 imported.push 双计
        }
```

> 说明：原结构是 `if pkg {} else if !dry_run {} else {}` 三分支 + 末尾 `report.imported.push(name)`。改造后 unmanaged 合并为单 `else` 分支（内含 symlink 判断 + dry_run 分叉），分支内自管 `imported.push` 后 `continue`；末尾 `report.imported.push(name)`（import.rs:108）只对 reinstall 成功生效（reinstall 成功不 continue）。确认 reinstall 分支（import.rs:71-88）行为不变。

- [ ] **Step 4: 更新现有测试 import_registers_unmanaged_and_skips_invalid**

`crates/core/src/import.rs:196-242` 的断言改造。把对 `baz.canonical_path` 的断言（`:231-235`）从原位置改为池子，并加桥接断言：

```rust
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
                paths.skillkit_skills_dir().join(n).join("SKILL.md").exists(),
                "{n} 迁入池子"
            );
        }
```

同时该测试末尾对 `report.unmanaged.len() == 3` 的断言保留（foo/bar/baz 仍计 unmanaged）；可补 `assert_eq!(report.relocated.len(), 3, "三个迁池");`。

- [ ] **Step 5: 确认其余 3 个现有测试仍 PASS（必要时微调）**

- `import_dry_run_writes_nothing`（`:244`）：dry_run 不迁文件、registry 空。改造后 dry_run 分支仍只统计（不 adopt/桥接）。断言不变，PASS。
- `import_dry_run_dedups_same_name_across_dirs`（`:258`）：dry_run 预报 unmanaged。改造后 dry_run 也 push relocated（预报）。若该测试断言 `report.unmanaged` 计数，不变；可补 relocated 预报断言（可选）。
- `import_is_idempotent`（`:277`）：二次跑 relink 发现已入池 + 桥接在位（ensure 幂等跳过）、主循环 registered 含 foo 跳过。`report.imported.is_empty()` 仍成立。PASS。

Run: `cargo test -p skillkit-core --lib import::tests`
Expected: 全部 PASS（4 现有 + 3 Task2 + 5 Task3 + 2 本 task 新增）。

- [ ] **Step 6: format + lint + commit**

```bash
make format
make lint
git add crates/core/src/import.rs
git commit -m "feat(core): import 主循环 unmanaged 分支迁池+桥接，symlink-src 跳过"
```

---

### Task 5: README 文档同步 + 全量验证

**Files:**
- Modify: `README.md:94`（import 命令描述）、`:107`（uninstall 描述）

**Interfaces:** 无代码，文档对齐 spec §10。

- [ ] **Step 1: 更新 README import 命令描述**

`README.md:94` 当前 `skillkit import-existing  # 扫描存量目录，可溯重装入池 + 无源登记` 改为：

```
skillkit import-existing                         # 扫描存量目录，可溯重装入池 + 无源迁入池（原位 symlink 桥接）
```

并在该命令组下补一句说明（`:94-96` 区间）：

> 无源（unmanaged）skill 物理迁入 canonical 池，原位置（`~/.agents/skills/`、`~/.claude/skills/`）用 symlink 桥接取代；已登记的存量 unmanaged 也会被补迁入池（幂等）。

- [ ] **Step 2: 更新 uninstall 描述**

`README.md:107` 当前 `unmanaged skill（无源存量）只删登记不删目录。` 语义不变（uninstall 仍不删 canonical），但 canonical 现在在池子，措辞微调为：

```
unmanaged skill（无源存量）只删登记不删 canonical 目录（迁池后 canonical 在 ~/.skillkit/.agents/skills/）。
```

- [ ] **Step 3: 全量验证**

Run: `make check`
Expected: format + lint（clippy -D warnings）+ 全测试 PASS。

Run: `make run ARGS="import-existing --dry-run"`（手动走查预览，确认 summary 含「入池迁址 / 含存量补迁」计数）。

- [ ] **Step 4: commit**

```bash
git add README.md
git commit -m "docs: README 同步 import 迁池语义 + uninstall canonical 位置说明"
```

---

## Self-Review

**Spec coverage（逐节核对）：**

| spec 节 | 落点 task |
|---|---|
| §3.1 adopt_into_pool（迁移+dedup+幂等+纯职责） | Task 2 |
| §3.2 主循环 unmanaged 分支（步骤0 symlink 跳过 continue + dry_run 分叉前 + adopt→save→桥接） | Task 4 |
| §3.3 relink_unmanaged（归槽 + 补桥接 starts_with(pool) 前置 + dangling/symlink 跳过不补桥接 + save 时机 + 权衡声明） | Task 3 |
| §3.4 桥接复用 ensure_global_claude（不改 symlink.rs） | Task 3/4（调用） |
| §3.5 rename 原子 + EXDEV→Io 兜底 | Task 2（裸 `?` 走 Io） |
| §4 CLI summary（relocated/relinked 计数） | Task 1 |
| §5 server import handler summary | Task 1 |
| §6 错误处理表（symlink-src skipped / 桥接占位 CanonicalCreate / 跨目录同名中断 / rename Io / dangling warn / relink 破坏性风险） | Task 2/3/4（行为 + 测试） |
| §7 组件（ImportReport 加字段 + 无新依赖/模块/error 变体 + Io 非 Tool） | Task 1/2 |
| §8 测试（现有4更新 + adopt dedup/幂等 + relink 补迁/补桥接/dangling/symlink/dry_run + symlink-src跳过 + 跨目录同名中断 + import_json schema 锁定） | Task 1/2/3/4 |
| §10 README 同步 | Task 5 |
| r1 P1（adopt symlink 防护） | Task 4 步骤 0 |
| r2 P2-1（补桥接自指 symlink，starts_with 前置） | Task 3（`if !canon.starts_with(&pool)` + continue 不补桥接） |
| r2 P2-2（Io 非 Tool） | Task 2（裸 `?`）+ Task 1（summary 不涉变体） |
| r2 P2-3（跨目录同名中断） | Task 4 测试 |

无遗漏。

**Placeholder scan：** 无 TBD/TODO。Task 4 Step 3 的主循环替换给了完整代码 + 对末尾 `imported.push` 的影响说明；Task 4 Step 4 的测试断言改造给了关键断言块（foo/bar/baz 迁池 + 桥接 symlink），实现时按现有测试结构套入。

**Type consistency：**
- `adopt_into_pool(paths, name: &str, src: &Path) -> Result<PathBuf>`（Task 2 定义）← Task 3 relink、Task 4 主循环调用签名一致。
- `relink_unmanaged(paths, report: &mut ImportReport, dry_run: bool) -> Result<()>`（Task 3 定义）← import_existing 开头调用一致。
- `ImportReport { relocated, relinked }`（Task 1）← Task 3 填 relinked、Task 4 填 relocated、CLI/server summary 引用一致。
- 错误变体：FS 错误走 `Io`（`#[from]`，Task 2 裸 `?`）；桥接占位走 `CanonicalCreate`（ensure_global_claude 内部，不改）。无新变体。

**已知限制（诚实声明）：**
- import_existing 未持 `FileLock`（既有债，spec §3.5），低频人工操作，不在本 plan 引入。
- uninstall 对 unmanaged 仍不删 canonical（spec §6 连带影响），canonical 现在池子，uninstall 后留孤儿 + 桥接 dangling——本 plan 不改 uninstall 范围。
- Task 4 跨目录同名中断测试只断言 `is_err()`，不断言中断前的部分状态（agents/foo 可能已 adopt + registry 已 save），符合 spec §6「重跑持续撞占位直到清理」语义。
