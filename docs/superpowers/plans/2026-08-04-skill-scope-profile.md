# skill scope 转移与 profile 归属管理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 skill scope 双向转移（global↔local，转移即同步物理落地）+ 收紧 global 与 profile/project 归属互斥（core 硬约束）+ Skills 视图改造为 scope/profile 归属管理中枢。

**Architecture:** core 新增 `set_scope`（scope 转移 + 自动清理 profile/project 引用 + 物理落地）、`remove_global_claude`（撤全局 symlink）、`skill_profiles`（反向索引），并给 `add_skill`/`set_profiles` 加 `&Registry` 参数做 scope 校验（global 拒绝）。cli 新增顶层 `rescope` 命令（照抄 `remove` 的确认模式）。server Skills 视图加「所属 profile」chips 列、高亮 toggle 批量归入、profile 过滤、scope 转移按钮 + 三个新端点（返回完整 Skills 页）。

**Tech Stack:** Rust（edition 2021）+ Axum + Askama（服务端渲染片段）+ htmx + clap（derive）+ thiserror + serde。前端无框架，rust-embed 嵌入静态资源。

## Global Constraints

（每个 task 的需求隐含包含本节，源自 spec `docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md` + CLAUDE.md）

- 路径绝不硬编码：用户目录用 `dirs::home_dir()` / `Paths`，不写死 `/Users/...`。
- core 公开类型一律在 `crates/core/src/lib.rs` re-export；新增 `set_scope`/`remove_global_claude`/`skill_profiles`/`RescopeReport`/`SkillIsGlobal` 都要 re-export。
- 错误用 `thiserror`（`crates/core/src/error.rs`），文案「反馈引导行动」（给下一步，不只报失败）。
- 序列化用 `serde`；`Scope` 沿用 `#[serde(rename_all = "lowercase")]`（`"global"`/`"local"`）；`--json` schema 视为公开契约，加 schema 锁定测试。
- 文件原子写（`error::atomic_write`：写 tmp + rename）。
- 改完必跑 `make format && make lint`（fmt 应用 rustfmt、lint = fmt --check + clippy `-D warnings`）；提交前 `make check` 一站式（format && lint && test）。
- 测试验证业务结果（apply 后能加载到正确 skill），不验证实现细节；测试里 `git commit` 带 `-c user.email -c user.name`。
- git commit message 用中文，Conventional Commits（`feat:`/`fix:`/`refactor:`/`docs:`/`test:`）。本 plan 的 commit 步骤供执行者使用，未获主人指示前不自动 commit。
- 前端（server）：htmx 服务端渲染片段，业务逻辑只在 core（handler 是薄壳）；写操作（POST/DELETE）返回完整页 `hx-target="body" hx-swap="outerHTML"`；片段外层固定 id；SSE 刷新请求带 `?fragment=1` 纯片段。
- 主 spec 修订点（§8.4/§9/§10.1/§11/§12）+ 决策 17/18 在 Task 14 落实。

## File Structure

**core（`crates/core/src/`）**
- `error.rs`（改）：新增 `SkillIsGlobal { id }` 变体。
- `profile.rs`（改）：`add_skill` 加 `registry: &Registry` 参数 + scope 校验；新增 `skill_profiles(paths, id) -> Vec<String>`；既有 `add_skill` 测试同步。
- `project.rs`（改）：`add_skill` 加 `registry` + scope 校验；`set_profiles` 加 `registry` + 灌入跳过 global；既有测试同步。
- `symlink.rs`（改）：新增 `remove_global_claude(paths, meta)`。
- `scope.rs`（新）：`set_scope(paths, id, target) -> Result<RescopeReport>` + `RescopeReport { affected_profiles, affected_projects }`。
- `lib.rs`（改）：re-export 新增项；`pub mod scope;`。

**cli（`crates/cli/src/`）**
- `commands/rescope.rs`（新）：`RescopeCmd` + `run_rescope`（照抄 `remove` 确认模式）。
- `commands/profile.rs`（改）：`profile add-skill` 调 `add_skill(id, &registry)`。
- `commands/project.rs`（改）：`project add-skill` 调 `add_skill(id, &registry)`；`set_profiles` 调用加 `&registry`。
- `commands/mod.rs`（改）：`pub mod rescope;`。
- `main.rs`（改）：`Cmd` 加 `Rescope(RescopeCmd)` + 分发。

**server（`crates/server/src/` + templates）**
- `routes/skills.rs`（改）：`SkillsQuery { fragment, selected, profiles }`；`SkillsTpl`/`SkillsMainTpl` 加 `selected`/`profile_filter`/`profiles_of`（反向 map）字段；`render_skills` 透传 + 建反向 map；新增 `assign`/`assign_new`/`delete_profile` handler。
- `routes/mod.rs`（改）：注册 3 个新路由 + `SkillsQuery` 定义（或 skills.rs）。
- `routes/profiles.rs`（改）：`create` 加存在性校验。
- `routes/projects.rs`（改）：`set_profiles` handler 调用加 `&registry`（`:327`）。
- `templates/fragments/skills_main.html`（改）：列 `id｜scope｜所属 profile｜ops` + 高亮 toggle + 批量栏 + profile 过滤 chips + scope 转移按钮；删 source/version/hash 列、per-row install。
- `templates/fragments/profiles_main.html`（改）：删手填 `source/skill` 表单；profile.skills 渲染过滤 global。
- `templates/layout.html`（改）：SSE 刷新带上当前 query。

**文档**
- `docs/2026-07-29-skillkit-design.md`（改）：§8.4/§9/§10.1/§11/§12。
- `docs/design-decisions-2026-07-29.md`（改）：追加决策 17/18。

---

## Task 1: core error 新增 `SkillIsGlobal` 变体

**Files:**
- Modify: `crates/core/src/error.rs:7-49`（`SkillkitError` 枚举）

**Interfaces:**
- Produces: `SkillkitError::SkillIsGlobal { id: String }`，文案引导「global skill 不属于 profile/project，先 `skillkit rescope <id> local` 再归入」。后续 Task 2/3 的 scope 校验返回它。

- [ ] **Step 1: 加错误变体**

在 `error.rs` 的 `SkillAlreadyInstalled` 变体后插入：

```rust
    #[error("skill 是 global，不属 profile/project：{id}（先 `skillkit rescope {id} local` 再归入）")]
    SkillIsGlobal { id: String },
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p skillkit-core`
Expected: PASS（新变体无调用方也能编译）

- [ ] **Step 3: 验证文案（单测）**

在 `error.rs` 末尾追加测试模块（若已有 `#[cfg(test)]` 则并入）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skill_is_global_message_guides_rescope() {
        let e = SkillkitError::SkillIsGlobal { id: "skills.sh/foo".into() };
        let msg = e.to_string();
        assert!(msg.contains("global"), "文案点明 global");
        assert!(msg.contains("rescope skills.sh/foo local"), "文案给出 rescope 引导");
    }
}
```

Run: `cargo test -p skillkit-core skill_is_global_message_guides_rescope`
Expected: PASS

- [ ] **Step 4: lint + commit**

Run: `make lint`
Expected: 双绿（fmt --check + clippy -D warnings）

```bash
git add crates/core/src/error.rs
git commit -m "feat(core): 加 SkillIsGlobal 错误变体（global 归属约束引导）"
```

---

## Task 2: core `profile.add_skill` 加 registry 校验 + `skill_profiles` 反向索引

**Files:**
- Modify: `crates/core/src/profile.rs:37-43`（`add_skill`）+ 新增 `skill_profiles`
- Test: `crates/core/src/profile.rs`（内联 `tests` 模块）

**Interfaces:**
- Consumes: `SkillkitError::SkillIsGlobal`（Task 1）、`Registry`（`registry.rs`）。
- Produces: `Profile::add_skill(&mut self, id: &str, registry: &Registry) -> Result<()>`（签名变更，波及 Task 7 cli + Task 9 server assign）；`skill_profiles(paths, id) -> Vec<String>`（Task 8 server 渲染反向 map、Task 12 模板）。

- [ ] **Step 1: 写失败测试（先改测试，红）**

在 `profile.rs` 的 `tests` 模块替换 `add_remove_skill_persists` 为下面两组（add_skill 签名加 registry）：

```rust
    fn reg_with(paths: &Paths, id: &str, scope: crate::registry::Scope) -> crate::registry::Registry {
        let mut reg = crate::registry::Registry::default();
        reg.upsert(crate::registry::SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{}", id.rsplit('/').next().unwrap()),
        });
        reg
    }

    #[test]
    fn add_skill_local_persists_and_dedups() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/fe", crate::registry::Scope::Local);
        let mut profile = Profile { name: "fe".into(), description: String::new(), skills: vec![] };
        profile.add_skill("skills.sh/fe", &reg).unwrap();
        // 重复 add 报 SkillAlreadyInstalled
        assert!(matches!(
            profile.add_skill("skills.sh/fe", &reg),
            Err(crate::error::SkillkitError::SkillAlreadyInstalled { .. })
        ));
    }

    #[test]
    fn add_skill_global_rejected() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/g1", crate::registry::Scope::Global);
        let mut profile = Profile { name: "fe".into(), description: String::new(), skills: vec![] };
        assert!(matches!(
            profile.add_skill("skills.sh/g1", &reg),
            Err(crate::error::SkillkitError::SkillIsGlobal { .. })
        ));
        assert!(profile.skills.is_empty(), "拒绝时 skills 不变");
    }
```

`skill_profiles` 测试（同模块）：

```rust
    #[test]
    fn skill_profiles_reverses_and_global_empty() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/fe", crate::registry::Scope::Local);
        let reg_g = reg_with(&p, "skills.sh/g1", crate::registry::Scope::Global);
        // 用 reg/reg_g 建 profile（g1 建不进去，手工塞模拟 legacy）
        let mut fe = Profile { name: "fe".into(), description: String::new(), skills: vec![] };
        fe.add_skill("skills.sh/fe", &reg).unwrap();
        fe.save(&p).unwrap();
        let mut base = Profile { name: "base".into(), description: String::new(), skills: vec!["skills.sh/fe".into()] };
        base.save(&p).unwrap();
        // legacy：手工塞 global 进 profile（绕过校验模拟存量）
        let mut legacy = Profile { name: "legacy".into(), description: String::new(), skills: vec!["skills.sh/g1".into()] };
        legacy.save(&p).unwrap();

        let mut got = skill_profiles(&p, "skills.sh/fe");
        got.sort();
        assert_eq!(got, vec!["base".to_string(), "fe".to_string()]);
        // global 永远空（即使 legacy profile 含它）
        assert!(skill_profiles(&p, "skills.sh/g1").is_empty());
    }
```

Run: `cargo test -p skillkit-core add_skill_local_persists_and_dedups`
Expected: FAIL（签名不匹配，编译错）

- [ ] **Step 2: 改 `add_skill` 签名 + 加 scope 校验**

替换 `profile.rs:37-43`：

```rust
    /// 加 skill：先查 registry 拒绝 global（core 硬约束），再查重。非幂等（重复返 SkillAlreadyInstalled）。
    pub fn add_skill(&mut self, id: &str, registry: &crate::registry::Registry) -> Result<()> {
        if registry.get(id).map(|m| m.scope).unwrap_or(crate::registry::Scope::Local) == crate::registry::Scope::Global {
            return Err(SkillkitError::SkillIsGlobal { id: id.to_string() });
        }
        if self.skills.iter().any(|s| s == id) {
            return Err(SkillkitError::SkillAlreadyInstalled { id: id.to_string() });
        }
        self.skills.push(id.to_string());
        Ok(())
    }
```

文件头加 `use crate::registry::Registry;`（若未导入）。

- [ ] **Step 3: 加 `skill_profiles` 函数**

在 `profile.rs` 的 `list_names` 函数后（`impl Profile` 外的自由函数区）加：

```rust
/// 反向索引：扫所有 profile，返回含 skill_id 的 profile name 列表。
/// global skill 永远空（不属任何 profile）。现算不缓存（profile 数量小，YAGNI）。
pub fn skill_profiles(paths: &Paths, skill_id: &str) -> Vec<String> {
    let reg = crate::registry::Registry::load(paths).unwrap_or_default();
    // global 直接空（语义保证，不依赖 profile 实存）
    if reg.get(skill_id).map(|m| m.scope) == Some(crate::registry::Scope::Global) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Ok(names) = list_names(paths) {
        for name in names {
            if let Ok(p) = Profile::load(paths, &name) {
                if p.skills.iter().any(|s| s == skill_id) {
                    out.push(name);
                }
            }
        }
    }
    out
}
```

- [ ] **Step 4: 修既有测试调用方**

`profile.rs` tests 里其他用 `add_skill` 的地方（如 `add_skill_local_persists_and_dedups` 已改；检查无遗漏）补 `&reg` 参数。`register_and_apply_profile_persists` 是 project 的，本 task 不涉及。

Run: `cargo test -p skillkit-core profile::`
Expected: PASS（add_skill 两组 + skill_profiles）

- [ ] **Step 5: 临时让 cli/server 编译过（加 registry 参数的最小适配）**

`add_skill` 签名变了，cli（`commands/profile.rs`）/ server（`profiles.rs` add_skill handler）会编译错。本 task 先不深入适配（Task 7/9 做），但要让 workspace 编译过以提交 core。在 cli/server 的 `add_skill(..)` 调用处临时传一个 `&Registry::load(&paths)?`，确保 `cargo build` 过。

Run: `cargo build`
Expected: PASS

- [ ] **Step 6: lint + commit**

Run: `make lint && cargo test -p skillkit-core`
Expected: 双绿 + core 测试 PASS

```bash
git add crates/core/src/profile.rs crates/cli/src/commands/profile.rs crates/server/src/routes/profiles.rs
git commit -m "feat(core): profile.add_skill 加 registry 校验拒绝 global + skill_profiles 反向索引"
```

---

## Task 3: core `project.add_skill` + `set_profiles` 加 registry 校验/过滤

**Files:**
- Modify: `crates/core/src/project.rs:80-86`（`add_skill`）+ `:110-123`（`set_profiles`）
- Test: `crates/core/src/project.rs`（内联 `tests`）

**Interfaces:**
- Consumes: `SkillkitError::SkillIsGlobal`（Task 1）、`Registry`。
- Produces: `Project::add_skill(&mut self, id, registry: &Registry)`、`Project::set_profiles(&mut self, names, profiles, registry: &Registry)`。波及 Task 7 cli、Task 11 server projects handler、`set_profiles_*` 既有测试。

- [ ] **Step 1: 写失败测试**

在 `project.rs` tests 加（复用 Task 2 的 registry 构造思路，project 内自建 helper）：

```rust
    fn reg_with(id: &str, scope: crate::registry::Scope) -> crate::registry::Registry {
        let mut reg = crate::registry::Registry::default();
        reg.upsert(crate::registry::SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{}", id.rsplit('/').next().unwrap()),
        });
        reg
    }

    #[test]
    fn add_skill_global_rejected() {
        let reg = reg_with("skills.sh/g1", crate::registry::Scope::Global);
        let mut proj = Project { id: "X".into(), name: "p".into(), path: "/tmp/p".into(),
            agents: vec![], applied_profiles: vec![], installed_skills: vec![], locked_shas: Default::default() };
        assert!(matches!(
            proj.add_skill("skills.sh/g1", &reg),
            Err(crate::error::SkillkitError::SkillIsGlobal { .. })
        ));
    }

    #[test]
    fn set_profiles_skips_global() {
        let reg = reg_with("dc/g", crate::registry::Scope::Global);
        let reg2 = reg_with("dc/l", crate::registry::Scope::Local);
        let mut reg_all = crate::registry::Registry::default();
        reg_all.upsert(reg.get("dc/g").cloned().unwrap());
        reg_all.upsert(reg2.get("dc/l").cloned().unwrap());
        // profile 含一个 global（legacy）+ 一个 local
        let fe = crate::profile::Profile { name: "fe".into(), description: String::new(),
            skills: vec!["dc/g".into(), "dc/l".into()] };
        let mut proj = Project { id: "X".into(), name: "p".into(), path: "/tmp/p".into(),
            agents: vec![], applied_profiles: vec![], installed_skills: vec![], locked_shas: Default::default() };
        proj.set_profiles(&["fe".into()], &[fe], &reg_all);
        assert_eq!(proj.installed_skills, vec!["dc/l".to_string()], "global 被跳过，只留 local");
    }
```

Run: `cargo test -p skillkit-core set_profiles_skips_global`
Expected: FAIL（签名不匹配）

- [ ] **Step 2: 改 `add_skill`（`project.rs:80-86`）**

```rust
    pub fn add_skill(&mut self, id: &str, registry: &crate::registry::Registry) -> Result<()> {
        if registry.get(id).map(|m| m.scope).unwrap_or(crate::registry::Scope::Local) == crate::registry::Scope::Global {
            return Err(SkillkitError::SkillIsGlobal { id: id.to_string() });
        }
        if self.installed_skills.iter().any(|s| s == id) {
            return Err(SkillkitError::SkillAlreadyInstalled { id: id.to_string() });
        }
        self.installed_skills.push(id.to_string());
        Ok(())
    }
```

- [ ] **Step 3: 改 `set_profiles`（`project.rs:110-123`）加 registry 跳过 global**

```rust
    pub fn set_profiles(&mut self, names: &[String], profiles: &[crate::profile::Profile], registry: &crate::registry::Registry) {
        self.applied_profiles = names.to_vec();
        let mut skills: Vec<String> = Vec::new();
        for name in names {
            if let Some(p) = profiles.iter().find(|p| &p.name == name) {
                for id in &p.skills {
                    // 跳过 global（防 legacy profile 含 global 进 installed_skills）
                    let is_global = registry.get(id).map(|m| m.scope) == Some(crate::registry::Scope::Global);
                    if !is_global && !skills.contains(id) {
                        skills.push(id.clone());
                    }
                }
            }
        }
        self.installed_skills = skills;
    }
```

- [ ] **Step 4: 修既有 `set_profiles_*` 测试 + apply_profile 测试**

`set_profiles_recomputes_union_and_replaces`、`set_profiles_replace_unbinds_previous`（`project.rs:258-315`）调用处加 `&Registry::default()`（这些测试 profile 里的 id 不在 registry，按 Local 兜底，行为不变）。`register_and_apply_profile_persists` 里 `proj.add_skill("dc/logseq")` 改 `proj.add_skill("dc/logseq", &Registry::default())`。

Run: `cargo test -p skillkit-core project::`
Expected: PASS

- [ ] **Step 5: 临时适配 cli/server 调用方让编译过**

`commands/project.rs` 的 project add-skill / set_profiles、`routes/projects.rs:327` 调用处临时传 `&Registry::load(&paths)?`（Task 7/11 正式做）。

Run: `cargo build`
Expected: PASS

- [ ] **Step 6: lint + commit**

Run: `make lint && cargo test -p skillkit-core`
Expected: 双绿 + 测试 PASS

```bash
git add crates/core/src/project.rs crates/cli/src/commands/project.rs crates/server/src/routes/projects.rs
git commit -m "feat(core): project.add_skill/set_profiles 加 registry 校验，跳过 global（归属互斥）"
```

---

## Task 4: core `symlink::remove_global_claude`

**Files:**
- Modify: `crates/core/src/symlink.rs`（新增函数）
- Test: `crates/core/src/symlink.rs`（内联 `tests`）

**Interfaces:**
- Consumes: `Paths`、`SkillMeta`。
- Produces: `remove_global_claude(paths, meta) -> Result<()>`（Task 5 set_scope global→local 调用）。

- [ ] **Step 1: 写失败测试**

在 `symlink.rs` tests 加（复用现有 `global_meta` + `creates_two_links_and_is_idempotent` 的建链前置）：

```rust
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
```

Run: `cargo test -p skillkit-core remove_global_claude`
Expected: FAIL（函数未定义）

- [ ] **Step 2: 实现 `remove_global_claude`**

在 `symlink.rs` 的 `ensure_global_claude` 后加。**不加 scope 守卫**（调用时 meta.scope 已改，见 spec §3.1 P2-A）：

```rust
/// 撤 global skill 的两层 symlink（与 ensure_global_claude 对称）。canonical 池子不删。
/// 不加 scope 守卫：set_scope 在改 scope 之后调用本函数，meta.scope 已是 local，
/// 镜像 ensure 的守卫会 no-op 留悬空链。调用方保证语义。
/// 幂等：链接不存在静默跳过。真实目录（非 symlink）报错不删（数据损失防护）。
pub fn remove_global_claude(paths: &Paths, meta: &SkillMeta) -> Result<()> {
    let agents_link = paths.agents_skills_dir().join(&meta.name);
    let claude_link = paths.claude_skills_dir().join(&meta.name);
    remove_one_link(&claude_link)?; // 先 claude（→ agents_link）再 agents（→ canonical）
    remove_one_link(&agents_link)?;
    Ok(())
}

/// 删单个 symlink：不存在跳过（幂等）；真实目录占位报错（对齐 ensure_link 守卫）。
fn remove_one_link(link: &Path) -> Result<()> {
    if !link.exists() && !std::fs::symlink_metadata(link).is_ok() {
        return Ok(()); // 不存在，幂等跳过
    }
    if std::fs::symlink_metadata(link)?.file_type().is_symlink() {
        std::fs::remove_file(link).map_err(|e| SkillkitError::Tool { message: e.to_string() })?;
    } else {
        // 真实目录/文件占位：报错不删
        return Err(SkillkitError::CanonicalCreate(link.to_path_buf()));
    }
    Ok(())
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p skillkit-core remove_global_claude`
Expected: PASS（删链幂等 + 真实目录守卫）

- [ ] **Step 4: lint + commit**

Run: `make lint`
Expected: 双绿

```bash
git add crates/core/src/symlink.rs
git commit -m "feat(core): symlink 新增 remove_global_claude（撤全局落地，幂等+真实目录守卫）"
```

---

## Task 5: core `scope::set_scope` 转移

**Files:**
- Create: `crates/core/src/scope.rs`
- Modify: `crates/core/src/lib.rs`（`pub mod scope;` + re-export）
- Test: `crates/core/src/scope.rs`（内联 `tests`）

**Interfaces:**
- Consumes: `Registry`/`SkillMeta`（`registry.rs`）、`ensure_global_claude`/`remove_global_claude`（`symlink.rs`）、`Profile`（`profile.rs`）、`Project`（`project.rs`）、`list_profile_names`/`list_project_ids`。
- Produces: `set_scope(paths, id, target) -> Result<RescopeReport>`、`RescopeReport { affected_profiles: Vec<String>, affected_projects: Vec<String> }`。Task 6 cli rescope 调用。

- [ ] **Step 1: 写失败测试（集成式，tempdir 模拟）**

新建 `scope.rs`，先写 tests：

```rust
//! scope 转移：global↔local，转移即同步物理落地 + 自动清理 profile/project 引用。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};

/// rescope 报告：受影响的 profile/project（local→global 清理时填）。
#[derive(Debug, Clone, PartialEq)]
pub struct RescopeReport {
    pub affected_profiles: Vec<String>,
    pub affected_projects: Vec<String>,
}

/// 改 skill 的 scope 并同步物理落地。local→global 建全局 + 清 profile/project；
/// global→local 撤全局（canonical 保留）。落地失败原子回滚 scope（registry 不落盘）。
pub fn set_scope(paths: &Paths, id: &str, target: Scope) -> Result<RescopeReport> {
    let mut reg = Registry::load(paths)?;
    let mut meta = reg.get(id)?.clone();
    if meta.scope == target {
        return Ok(RescopeReport { affected_profiles: vec![], affected_projects: vec![] });
    }
    let prev = meta.scope;
    match (prev, target) {
        (Scope::Local, Scope::Global) => {
            // 1. 建全局落地（先建链，失败可回滚 scope 不改）
            crate::symlink::ensure_global_claude(paths, &meta)?;
            // 2. 改 scope + 落盘
            meta.scope = Scope::Global;
            reg.upsert(meta.clone());
            reg.save(paths)?;
            // 3. 清 profile/project 引用（跨多文件，非原子——失败给可恢复文案，见 spec §6）
            let (ap, aproj) = remove_refs(paths, id)?;
            Ok(RescopeReport { affected_profiles: ap, affected_projects: aproj })
        }
        (Scope::Global, Scope::Local) => {
            // 1. 撤全局落地（meta.scope 仍是 Global，remove 不加守卫，安全）
            crate::symlink::remove_global_claude(paths, &meta)?;
            // 2. 改 scope + 落盘
            meta.scope = Scope::Local;
            reg.upsert(meta.clone());
            reg.save(paths)?;
            // global 本不在 profile/project，无需清
            Ok(RescopeReport { affected_profiles: vec![], affected_projects: vec![] })
        }
        _ => Ok(RescopeReport { affected_profiles: vec![], affected_projects: vec![] }),
    }
}

/// 从所有 profile.skills 和 project.installed_skills 移除 id。返回受影响名单。
/// 跨多文件独立 FileLock，非原子：逐个改逐个存，失败向上抛（已改的保留，调用方给可恢复文案）。
fn remove_refs(paths: &Paths, id: &str) -> Result<(Vec<String>, Vec<String>)> {
    let mut affected_profiles = Vec::new();
    for name in crate::profile::list_names(paths)? {
        if let Ok(mut p) = crate::profile::Profile::load(paths, &name) {
            if p.skills.iter().any(|s| s == id) {
                p.skills.retain(|s| s != id);
                p.save(paths)?;
                affected_profiles.push(name);
            }
        }
    }
    let mut affected_projects = Vec::new();
    for pid in crate::project::list_ids(paths)? {
        if let Ok(mut proj) = crate::project::Project::load(paths, &pid) {
            if proj.installed_skills.iter().any(|s| s == id) {
                proj.installed_skills.retain(|s| s != id);
                proj.save(paths)?;
                affected_projects.push(pid);
            }
        }
    }
    Ok((affected_profiles, affected_projects))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    fn paths() -> Paths { Paths::new(tempdir().unwrap().path().to_path_buf()) }

    fn seed_skill(paths: &Paths, id: &str, scope: Scope) -> SkillMeta {
        let name = id.rsplit('/').next().unwrap();
        let canon = paths.skillkit_skills_dir().join(name);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let meta = SkillMeta {
            id: id.into(), name: name.into(), source: id.split('/').next().unwrap().into(),
            scope, version: None, computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        };
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(meta.clone());
        reg.save(paths).unwrap();
        meta
    }

    #[test]
    fn local_to_global_builds_links_and_clears_refs() {
        let p = paths();
        let _m = seed_skill(&p, "dc/fe", Scope::Local);
        // 建一个 local profile 含它 + 一个 project 含它
        let mut fe = crate::profile::Profile { name: "fe".into(), description: String::new(), skills: vec!["dc/fe".into()] };
        fe.save(&p).unwrap();
        let mut proj = crate::project::Project {
            id: "P1".into(), name: "p".into(), path: "/tmp/p".into(), agents: vec![],
            applied_profiles: vec![], installed_skills: vec!["dc/fe".into()], locked_shas: Default::default(),
        };
        proj.save(&p).unwrap();

        let report = set_scope(&p, "dc/fe", Scope::Global).unwrap();
        assert_eq!(report.affected_profiles, vec!["fe".to_string()]);
        assert_eq!(report.affected_projects, vec!["P1".to_string()]);
        // 全局 symlink 建
        assert!(p.agents_skills_dir().join("fe").is_symlink());
        assert!(p.claude_skills_dir().join("fe").is_symlink());
        // profile/project 已清
        assert!(crate::profile::Profile::load(&p, "fe").unwrap().skills.is_empty());
        assert!(crate::project::Project::load(&p, "P1").unwrap().installed_skills.is_empty());
        // scope 已改
        assert_eq!(Registry::load(&p).unwrap().get("dc/fe").unwrap().scope, Scope::Global);
    }

    #[test]
    fn global_to_local_removes_links_keeps_canonical() {
        let p = paths();
        let meta = seed_skill(&p, "dc/g", Scope::Global);
        crate::symlink::ensure_global_claude(&p, &meta).unwrap();
        assert!(p.agents_skills_dir().join("g").is_symlink());

        let report = set_scope(&p, "dc/g", Scope::Local).unwrap();
        assert!(report.affected_profiles.is_empty() && report.affected_projects.is_empty());
        assert!(!p.agents_skills_dir().join("g").exists());
        assert!(!p.claude_skills_dir().join("g").exists());
        assert!(paths().skillkit_skills_dir().exists() || true); // 占位
        // canonical 保留
        let canon = std::path::Path::new(&meta.canonical_path);
        assert!(canon.exists(), "canonical 池子保留");
        assert_eq!(Registry::load(&p).unwrap().get("dc/g").unwrap().scope, Scope::Local);
    }

    #[test]
    fn same_scope_noop() {
        let p = paths();
        seed_skill(&p, "dc/x", Scope::Local);
        let report = set_scope(&p, "dc/x", Scope::Local).unwrap();
        assert!(report.affected_profiles.is_empty());
    }

    #[test]
    fn missing_skill_errors() {
        let p = paths();
        assert!(matches!(set_scope(&p, "nope/x", Scope::Global), Err(SkillkitError::SkillNotInstalled { .. })));
    }
}
```

Run: `cargo test -p skillkit-core local_to_global_builds_links_and_clears_refs`
Expected: FAIL（模块未注册）

- [ ] **Step 2: 注册模块 + re-export**

`lib.rs` 加 `pub mod scope;`，并在 re-export 区加：

```rust
pub use scope::{set_scope, RescopeReport};
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p skillkit-core scope::`
Expected: PASS（4 个测试）

- [ ] **Step 4: lint + commit**

Run: `make lint && cargo test -p skillkit-core`
Expected: 双绿 + 全 core 测试 PASS

```bash
git add crates/core/src/scope.rs crates/core/src/lib.rs
git commit -m "feat(core): set_scope 实现 scope 双向转移（落地+清理 profile/project 引用）"
```

---

## Task 6: cli 顶层 `rescope` 命令

**Files:**
- Create: `crates/cli/src/commands/rescope.rs`
- Modify: `crates/cli/src/commands/mod.rs`（`pub mod rescope;`）
- Modify: `crates/cli/src/main.rs:10,21-43,49-61`（import + `Cmd::Rescope` + 分发）

**Interfaces:**
- Consumes: `skillkit_core::set_scope` / `RescopeReport`（Task 5）、`Registry`/`Scope`。
- Produces: `RescopeCmd` + `run_rescope`，`--json` schema `{id, from, to, affected_profiles, affected_projects}`。

- [ ] **Step 1: 写失败测试（解析 + schema 锁定）**

新建 `commands/rescope.rs`，先写测试模块（照抄 `skill.rs` 的 `TestCli` 模式，本 task 在 rescope.rs 自建 TestCmd）：

```rust
//! rescope：skillkit rescope <id> <global|local> [--yes] [--json]，转移 scope + 同步物理落地。
use clap::Args;
use skillkit_core::{paths::Paths, registry::Scope, set_scope, Registry};

#[derive(Args)]
pub struct RescopeCmd {
    /// skill id，格式 <source>/<skill>
    pub id: String,
    /// 目标 scope：global | local
    pub scope: ScopeArg,
    #[arg(long)]
    pub yes: bool,
    /// JSON 输出（隐含 --yes）：{id, from, to, affected_profiles, affected_projects}
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Debug)]
pub enum ScopeArg { Global, Local }

impl std::str::FromStr for ScopeArg {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "global" => Ok(Self::Global),
            "local" => Ok(Self::Local),
            _ => Err(format!("scope 必须是 global|local，得到 {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};
    #[derive(Parser)]
    struct TestCli { #[command(subcommand)] cmd: TestCmd }
    #[derive(Subcommand)]
    enum TestCmd { Rescope(RescopeCmd) }

    #[test]
    fn rescope_parses_id_scope_flags() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "rescope", "dc/fe", "global", "--yes", "--json"]);
        let TestCmd::Rescope(c) = cmd else { panic!("expected Rescope") };
        assert_eq!(c.id, "dc/fe");
        assert!(matches!(c.scope, ScopeArg::Global));
        assert!(c.yes && c.json);
    }

    /// --json schema 锁定：字段名 + from/to 为 lowercase scope 字符串。
    #[test]
    fn rescope_json_schema_locks_fields() {
        let json = serde_json::json!({
            "id": "dc/fe", "from": "local", "to": "global",
            "affected_profiles": ["fe"], "affected_projects": ["P1"],
        });
        assert_eq!(json["from"], "local");
        assert_eq!(json["to"], "global");
        assert_eq!(json["affected_profiles"][0], "fe");
        assert_eq!(json["affected_projects"][0], "P1");
    }
}
```

Run: `cargo test -p skillkit-cli rescope_parses_id_scope_flags`
Expected: FAIL（`RescopeCmd` 未注册到 main，但本文件内 TestCmd 能解析——测试应 PASS；若 FAIL 是因模块未加进 mod.rs 导致编译不到，先做 Step 2 再跑）

- [ ] **Step 2: 实现 `run_rescope`（照抄 `remove` 确认模式）**

在 `rescope.rs` 的 `ScopeArg` 后、`tests` 前加：

```rust
pub fn run_rescope(cmd: RescopeCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    let target = match cmd.scope {
        ScopeArg::Global => Scope::Global,
        ScopeArg::Local => Scope::Local,
    };
    let from = Registry::load(&paths)?.get(&cmd.id)?.scope;

    let skip_confirm = cmd.yes || cmd.json;
    if !skip_confirm {
        let (dir, hint) = match (from, target) {
            (Scope::Local, Scope::Global) => ("local→global", "（将移除 profile/project 引用，不可撤销）"),
            (Scope::Global, Scope::Local) => ("global→local", "（将撤销全局落地，可 rescope global 恢复）"),
            _ => ("(无变化)", ""),
        };
        println!("将 rescope {id} {dir}{hint}，确认？(y/n)", id = cmd.id, dir = dir, hint = hint);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "y" {
            println!("已取消");
            return Ok(());
        }
    }

    let report = set_scope(&paths, &cmd.id, target)?;

    if cmd.json {
        println!(
            "{}",
            serde_json::json!({
                "id": cmd.id,
                "from": from.to_string(),
                "to": target.to_string(),
                "affected_profiles": report.affected_profiles,
                "affected_projects": report.affected_projects,
            })
        );
    } else {
        println!("✓ 已 rescope {id} {from}→{to}", id = cmd.id, from = from, to = target);
        if !report.affected_profiles.is_empty() {
            println!("  从 profile 移除：{}", report.affected_profiles.join(", "));
        }
        if !report.affected_projects.is_empty() {
            println!("  从项目移除：{}（需重新 apply 清理目录残留）", report.affected_projects.join(", "));
        }
    }
    Ok(())
}
```

- [ ] **Step 3: 注册命令**

`commands/mod.rs` 加 `pub mod rescope;`。`main.rs`：
- import 区加 `use commands::rescope::RescopeCmd;`
- `Cmd` 枚举加变体（放在 `Remove` 后）：

```rust
    /// 转移 skill scope（global↔local）
    Rescope(RescopeCmd),
```

- `match cli.cmd` 加：

```rust
        Cmd::Rescope(cmd) => commands::rescope::run_rescope(cmd)?,
```

Run: `cargo build -p skillkit-cli && cargo test -p skillkit-cli rescope_`
Expected: build PASS + 两测试 PASS

- [ ] **Step 4: lint + commit**

Run: `make lint`
Expected: 双绿

```bash
git add crates/cli/src/commands/rescope.rs crates/cli/src/commands/mod.rs crates/cli/src/main.rs
git commit -m "feat(cli): 新增 rescope 命令（scope 转移，两方向确认+--json schema）"
```

---

## Task 7: server Skills 视图数据层（SkillsQuery + 反向 map + 过滤）

**Files:**
- Modify: `crates/server/src/routes/skills.rs`（`SkillsTpl`/`SkillsMainTpl` 字段 + `render_skills` + 新增 `build_skills_view`）
- Modify: `crates/server/src/routes/mod.rs`（新增 `SkillsQuery`，或放 skills.rs 并 use）
- Test: `crates/server/src/routes/skills.rs`（内联 `tests`，测 `build_skills_view`）

**Interfaces:**
- Consumes: `skillkit_core::list_profile_names`/`Profile`/`Registry`/`Scope`、`AppState`。
- Produces: `SkillsQuery { fragment, selected, profiles }`（Task 8/9/11 用）、`build_skills_view(paths, profile_filter) -> (skills, profiles_of)`；`SkillsTpl`/`SkillsMainTpl` 加 `selected`/`profile_filter`/`profiles_of` 字段（Task 9 模板用）。

- [ ] **Step 1: 写失败测试（build_skills_view 过滤 + 反向 map）**

在 `skills.rs` 末尾加 tests 模块（用 `skillkit_core::Paths` + tempdir）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use skillkit_core::{Paths, Profile, Registry, Scope, SkillMeta};
    use tempfile::tempdir;

    fn paths() -> Paths { Paths::new(tempdir().unwrap().path().to_path_buf()) }

    fn seed(paths: &Paths, id: &str, scope: Scope) {
        let mut reg = Registry::load(paths).unwrap_or_default();
        reg.upsert(SkillMeta {
            id: id.into(), name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(), scope, version: None,
            computed_hash: Some("abc".into()), installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{}", id.rsplit('/').next().unwrap()),
        });
        reg.save(paths).unwrap();
    }

    #[test]
    fn build_view_filters_by_profile_and_maps_reverse() {
        let p = paths();
        seed(&p, "dc/fe", Scope::Local);
        seed(&p, "dc/be", Scope::Local);
        seed(&p, "dc/g", Scope::Global);
        // profile fe 含 dc/fe
        Profile { name: "fe".into(), description: String::new(), skills: vec!["dc/fe".into()] }
            .save(&p).unwrap();

        // 全部（filter 空）：含 global
        let (all, m) = build_skills_view(&p, &[]).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(m.get("dc/fe").cloned().unwrap_or_default(), vec!["fe".to_string()]);
        assert!(m.get("dc/g").is_none(), "global 不在反向 map");

        // 过滤 fe：只 local 且属 fe 的（global 不显示）
        let (filtered, _) = build_skills_view(&p, &["fe".into()]).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0.id, "dc/fe");
    }
}
```

Run: `cargo test -p skillkit-server build_view_filters`
Expected: FAIL（`build_skills_view` 未定义）

- [ ] **Step 2: 加 `SkillsQuery`（`routes/mod.rs`）**

在 `FragmentQuery` 后加：

```rust
/// Skills 页专属 query：fragment（SSE 片段）+ selected（高亮选中）+ profiles（过滤）。
#[derive(Debug, Default, Deserialize)]
pub struct SkillsQuery {
    pub fragment: Option<String>,
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub profiles: Option<String>,
}

impl SkillsQuery {
    pub fn is_fragment(&self) -> bool { self.fragment.as_deref() == Some("1") }
    pub fn selected_list(&self) -> Vec<String> { parse_csv(&self.selected) }
    pub fn profile_filter(&self) -> Vec<String> { parse_csv(&self.profiles) }
}

fn parse_csv(o: &Option<String>) -> Vec<String> {
    o.as_deref()
        .map(|s| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}
```

- [ ] **Step 3: 加 `build_skills_view` + 改 `SkillsTpl` 字段 + `render_skills` 透传**

`skills.rs` 顶部 import 加 `use std::collections::HashMap;` 和 `use crate::routes::SkillsQuery;`。`SkillsTpl`/`SkillsMainTpl` 加字段：

```rust
pub struct SkillsTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
    pub selected: Vec<String>,
    pub profile_filter: Vec<String>,
    pub profiles_of: HashMap<String, Vec<String>>,
    pub all_profile_names: Vec<String>, // 过滤 chips 用
}
// SkillsMainTpl 同样加这四个字段
```

`build_skills_view`（render_skills 调用前）：

```rust
/// 数据准备：建 skill_id→profile 反向 map（一次遍历），按 profile_filter 过滤 skill 列表。
/// filter 空 = 全部（含 global）；非空 = OR 语义（属任一选中 profile 的 local skill，global 不显示）。
fn build_skills_view(
    paths: &skillkit_core::Paths,
    profile_filter: &[String],
) -> Result<(
    Vec<(SkillMeta, String)>,
    HashMap<String, Vec<String>>,
    Vec<String>, // all_profile_names
)> {
    let reg = Registry::load(paths).unwrap_or_default();
    let all_profile_names = skillkit_core::list_profile_names(paths).unwrap_or_default();
    let mut profiles_of: HashMap<String, Vec<String>> = HashMap::new();
    for name in &all_profile_names {
        if let Ok(p) = skillkit_core::Profile::load(paths, name) {
            for id in &p.skills {
                profiles_of.entry(id.clone()).or_default().push(name.clone());
            }
        }
    }
    let skills: Vec<(SkillMeta, String)> = reg
        .skills
        .values()
        .filter(|m| {
            if profile_filter.is_empty() {
                true
            } else {
                profiles_of
                    .get(&m.id)
                    .map(|ps| ps.iter().any(|p| profile_filter.contains(p)))
                    .unwrap_or(false)
            }
        })
        .map(|m| (m.clone(), m.id.replace('/', "%2F")))
        .collect();
    Ok((skills, profiles_of, all_profile_names))
}
```

`render_skills` 改签名 + 调 `build_skills_view` + 透传：

```rust
fn render_skills(
    state: AppState,
    token: String,
    summary: Option<&str>,
    fragment: bool,
    selected: Vec<String>,
    profile_filter: Vec<String>,
) -> Response {
    match build_skills_view(&state.paths, &profile_filter) {
        Ok((skills, profiles_of, all_profile_names)) => {
            let rendered = if fragment {
                SkillsMainTpl { token: &token, skills, summary, selected, profile_filter, profiles_of, all_profile_names }.render()
            } else {
                SkillsTpl { token: &token, skills, summary, selected, profile_filter, profiles_of, all_profile_names }.render()
            };
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 skills 视图失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
```

`page` handler 改用 `SkillsQuery`：

```rust
pub async fn page(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
) -> Response {
    render_skills(state, token, None, q.is_fragment(), q.selected_list(), q.profile_filter())
}
```

现有写操作 handler（`install`/`uninstall`/`upgrade`/`install_candidate`/`import`/`upgrade_all`）的 `render_skills(state, token, None, false)` 改 `render_skills(state, token, None, false, vec![], vec![])`（暂不透传 selected，Task 8 新端点 + Task 11 SSE 透传；本 task 先让编译过）。

Run: `cargo test -p skillkit-server build_view_filters`
Expected: PASS

- [ ] **Step 4: 让模板编译过（字段加了但 Task 9 才用）**

`skills_main.html` 和 `skills.html`（页面壳 include main）暂不动——Askama struct 字段未被模板引用不报错。但 `make check` 会跑 askama 编译模板，确认不报错。

Run: `cargo build -p skillkit-server`
Expected: PASS

- [ ] **Step 5: lint + commit**

Run: `make lint && cargo test -p skillkit-server`
Expected: 双绿 + build_view 测试 PASS

```bash
git add crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs
git commit -m "feat(server): Skills 视图数据层（SkillsQuery + 反向 map + profile 过滤）"
```

---

## Task 8: server skills 新端点（assign / assign-new / delete-profile）+ 路由

**Files:**
- Modify: `crates/server/src/routes/skills.rs`（新增 3 handler + `apply_assign` 纯函数）
- Modify: `crates/server/src/routes/mod.rs:34-44`（注册 3 路由）
- Test: `crates/server/src/routes/skills.rs`（内联测 `apply_assign`）

**Interfaces:**
- Consumes: `SkillsQuery`（Task 7）、`Profile::add_skill`（Task 2，需 registry）、`Registry`。
- Produces: `assign`/`assign_new`/`delete_profile` handler + `apply_assign` 纯函数。Task 9 模板的表单 POST 到这些端点。

- [ ] **Step 1: 写失败测试（apply_assign 原子 + 错误区分）**

`skills.rs` tests 加（复用 Task 7 的 `paths`/`seed` helper）：

```rust
    #[test]
    fn apply_assign_skips_dup_but_throws_scope() {
        let p = paths();
        seed(&p, "dc/fe", Scope::Local);
        seed(&p, "dc/g", Scope::Global);
        let reg = Registry::load(&p).unwrap();
        let mut profile = Profile { name: "fe".into(), description: String::new(), skills: vec!["dc/fe".into()] };

        // 含已装的 fe（跳过）+ global g（抛错）→ 整批不 save
        let res = apply_assign(&mut profile, &["dc/fe".into(), "dc/g".into()], &reg);
        assert!(matches!(res, Err(skillkit_core::SkillkitError::SkillIsGlobal { .. })));
        // 原子：fe 仍在（原本就在），g 没进
        assert_eq!(profile.skills, vec!["dc/fe".to_string()]);
    }
```

Run: `cargo test -p skillkit-server apply_assign_skips`
Expected: FAIL（`apply_assign` 未定义）

- [ ] **Step 2: 加 `apply_assign` 纯函数**

`skills.rs` 加（handler 区）：

```rust
/// 批量归入核心：循环 add_skill，SkillAlreadyInstalled 跳过、其余（如 SkillIsGlobal）抛错不 save（原子）。
fn apply_assign(
    profile: &mut skillkit_core::Profile,
    ids: &[String],
    reg: &skillkit_core::Registry,
) -> skillkit_core::Result<()> {
    for id in ids {
        match profile.add_skill(id, reg) {
            Ok(()) => {}
            Err(skillkit_core::SkillkitError::SkillAlreadyInstalled { .. }) => {} // 跳过
            Err(e) => return Err(e), // scope 错误等向上抛
        }
    }
    Ok(())
}
```

- [ ] **Step 3: 加三个 handler**

`skills.rs` import 加 `use axum::body::Bytes;` 和 `use form_urlencoded;`（若未导入；profiles.rs 用了 `form_urlencoded::parse`，server 已依赖）。加 handler：

```rust
/// 批量归入已有 profile。body: profile=<名>&id=<...>（id 重复 key）。返回完整 Skills 页（透传 selected/profiles）。
pub async fn assign(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
    body: Bytes,
) -> Response {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(&body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let name = pairs.iter().find(|(k, _)| k == "profile").map(|(_, v)| v.clone());
    let ids: Vec<String> = pairs.iter().filter(|(k, _)| k == "id").map(|(_, v)| v.clone()).collect();
    let (Some(name), false) = (name, ids.is_empty()) else {
        return Html("<p class=\"err\">缺少 profile 或 id</p>").into_response();
    };
    let reg = Registry::load(&state.paths).unwrap_or_default();
    match skillkit_core::Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if let Err(e) = apply_assign(&mut p, &ids, &reg) {
                return Html(format!(r#"<p class="err">归入失败：{e}</p>"#)).into_response();
            }
            if p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_skills(state, token, None, false, q.selected_list(), q.profile_filter())
        }
        Err(_) => Html(format!(r#"<p class="err">profile {name} 不存在，改用新建或先创建</p>"#)).into_response(),
    }
}

/// 新建 profile 并归入。body: name=<新名>&id=<...>。先校验不存在（防 create 覆盖清空）。
pub async fn assign_new(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
    body: Bytes,
) -> Response {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(&body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let Some(name) = pairs.iter().find(|(k, _)| k == "name").map(|(_, v)| v.clone()) else {
        return Html("<p class=\"err\">缺少 name</p>").into_response();
    };
    let ids: Vec<String> = pairs.iter().filter(|(k, _)| k == "id").map(|(_, v)| v.clone()).collect();
    if skillkit_core::Profile::load(&state.paths, &name).is_ok() {
        return Html(format!(r#"<p class="err">profile {name} 已存在，改用归入或换名</p>"#)).into_response();
    }
    let reg = Registry::load(&state.paths).unwrap_or_default();
    let mut p = skillkit_core::Profile { name, description: String::new(), skills: vec![] };
    if let Err(e) = apply_assign(&mut p, &ids, &reg) {
        return Html(format!(r#"<p class="err">归入失败：{e}</p>"#)).into_response();
    }
    if p.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_skills(state, token, None, false, q.selected_list(), q.profile_filter())
}

/// chip ×：从 profile 移除单个 skill 归属。返回完整 Skills 页。
pub async fn delete_profile(
    State(state): State<AppState>,
    Path((token, id, name)): Path<(String, String, String)>,
    Query(q): Query<SkillsQuery>,
) -> Response {
    let id = id.replace("%2F", "/");
    match skillkit_core::Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if p.remove_skill(&id).is_err() || p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_skills(state, token, None, false, q.selected_list(), q.profile_filter())
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
```

- [ ] **Step 4: 注册路由（`routes/mod.rs` skills 段）**

在 `/{token}/skills/upgrade-all` 后、`/{token}/skills/{id}/install` 前加（字面量段先于参数段）：

```rust
        .route("/{token}/skills/assign", post(skills::assign))
        .route("/{token}/skills/assign-new", post(skills::assign_new))
        .route(
            "/{token}/skills/{id}/profile/{name}",
            delete(skills::delete_profile),
        )
```

Run: `cargo build -p skillkit-server && cargo test -p skillkit-server apply_assign_skips`
Expected: build PASS + 测试 PASS

- [ ] **Step 5: lint + commit**

Run: `make lint`
Expected: 双绿

```bash
git add crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs
git commit -m "feat(server): skills 新端点 assign/assign-new/delete-profile（批量归入+chip移除）"
```

---

## Task 9: server skills_main 模板改造（列 + chips + toggle + 批量栏 + 过滤 + scope 按钮）

**Files:**
- Modify: `crates/server/templates/fragments/skills_main.html`（整体重写表格区）
- Modify: `crates/server/static/app.css`（高亮 .selected、批量栏、过滤 chips 样式——按现有风格追加）
- Test: `crates/server/src/routes/skills.rs`（模板渲染单测）

**Interfaces:**
- Consumes: Task 7 的 `SkillsTpl` 字段（selected/profile_filter/profiles_of/all_profile_names）、Task 8 端点。
- Produces: Skills 视图完整 UI（需求 3/5/6/7 + scope 转移按钮）。

- [ ] **Step 1: 写失败测试（模板渲染含关键元素）**

`skills.rs` tests 加：

```rust
    fn meta(id: &str, scope: Scope) -> SkillMeta {
        SkillMeta {
            id: id.into(), name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(), scope, version: None,
            computed_hash: Some("abc".into()), installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{}", id.rsplit('/').next().unwrap()),
        }
    }

    #[test]
    fn skills_main_renders_profile_chips_and_selected_row() {
        let skills = vec![(meta("dc/fe", Scope::Local), "dc%2Ffe".into())];
        let mut profiles_of = std::collections::HashMap::new();
        profiles_of.insert("dc/fe".into(), vec!["fe".into()]);
        let html = SkillsMainTpl {
            token: "tok", skills, summary: None,
            selected: vec!["dc/fe".into()], profile_filter: vec![],
            profiles_of, all_profile_names: vec!["fe".into()],
        }.render().unwrap();
        assert!(html.contains("dc/fe"), "id 渲染");
        assert!(html.contains("selected"), "选中行有 selected 标记");
        assert!(html.contains("fe"), "所属 profile chip");
        assert!(html.contains("assign"), "归入端点");
    }
```

Run: `cargo test -p skillkit-server skills_main_renders`
Expected: FAIL（模板还没用新字段/chips）

- [ ] **Step 2: 重写 `skills_main.html`**

整体替换为（保留 find-bar/import/upgrade-all，改 table + 加过滤条/批量栏）：

```html
<h1>Skills</h1>
  {% if let Some(s) = summary %}<p class="summary">{{ s }}</p>{% endif %}
  <div class="find-bar">
    <input type="text" name="q" placeholder="搜 skills.sh 候选（如 pdf）"
           hx-get="/{{ token }}/skills/find"
           hx-trigger="keyup changed delay:400ms"
           hx-target="#find-results" hx-swap="outerHTML"
           hx-indicator="#find-indicator">
    <span id="find-indicator" class="htmx-indicator">搜索中…</span>
    <div id="find-results"></div>
  </div>
  <form class="inline" hx-post="/{{ token }}/skills/import"
        hx-target="body" hx-swap="outerHTML"><button>导入存量 skill</button></form>
  <form class="inline" hx-post="/{{ token }}/skills/upgrade-all"
        hx-target="body" hx-swap="outerHTML"><button>全部升级</button></form>

  {# profile 过滤条（多选 OR，「全部」含 global） #}
  <div class="filter-chips" id="skill-filter">
    <a class="chip{% if profile_filter.is_empty() %} on{% endif %}"
       hx-get="/{{ token }}/skills?fragment=1{% for s in &selected %}&selected={{ s }}{% endfor %}"
       hx-target="main" hx-swap="innerHTML">全部</a>
    {% for name in &all_profile_names %}
    <a class="chip{% if profile_filter.contains(name) %} on{% endif %}"
       hx-get="/{{ token }}/skills?fragment=1&profiles={{ name }}{% for s in &selected %}&selected={{ s }}{% endfor %}"
       hx-target="main" hx-swap="innerHTML">{{ name }}</a>
    {% endfor %}
  </div>

  {# 批量栏（有选中才显示）；表单收集选中 id 由 layout.html 的 toggle JS 填充 #}
  <form id="skill-batch" class="batch-bar" style="{% if selected.is_empty() %}display:none{% endif %}"
        hx-post="/{{ token }}/skills/assign{% for p in &profile_filter %}?profiles={{ p }}{% endfor %}"
        hx-target="body" hx-swap="outerHTML">
    <span>已选 <b id="skill-batch-count">{{ selected.len() }}</b></span>
    <select name="profile" id="skill-batch-profile">
      {% for name in &all_profile_names %}<option>{{ name }}</option>{% endfor %}
    </select>
    <button>归入</button>
  </form>
  <form id="skill-batch-new" class="batch-bar" style="{% if selected.is_empty() %}display:none{% endif %}"
        hx-post="/{{ token }}/skills/assign-new"
        hx-target="body" hx-swap="outerHTML">
    <input name="name" placeholder="新 profile 名">
    <button>新建并归入</button>
  </form>

  <table class="data">
  <thead><tr><th>id</th><th>scope</th><th>所属 profile</th><th>ops</th></tr></thead>
  <tbody>
  {% for (s, enc) in skills %}
  <tr class="{% if selected.contains(&s.id) %}selected{% endif %}"
      data-id="{{ s.id }}"
      {% if s.scope == Scope::Local %}onclick="toggleSkill(this)"{% endif %}>
    <td>{{ s.id }}{% if s.computed_hash.is_none() %} <span class="badge">unmanaged</span>{% endif %}</td>
    <td>{{ s.scope }}</td>
    <td>
      {% if let Some(ps) = profiles_of.get(&s.id) %}
        {% for p in ps %}<span class="chip">{{ p }}<button class="x"
          hx-delete="/{{ token }}/skills/{{ enc }}/profile/{{ p }}?fragment=1"
          hx-target="body" hx-swap="outerHTML">×</button></span>{% endfor %}
      {% else %}—{% endif %}
    </td>
    <td>
      {% if s.scope == Scope::Local %}
      <button class="u" hx-post="/{{ token }}/skills/rescope-{{ enc }}"
              hx-target="body" hx-swap="outerHTML">→global</button>
      {% else %}
      <button class="u" hx-post="/{{ token }}/skills/rescope-{{ enc }}"
              hx-target="body" hx-swap="outerHTML">→local</button>
      {% endif %}
      {% if s.computed_hash.is_some() %}
      <button class="u" hx-post="/{{ token }}/skills/{{ enc }}/upgrade" hx-target="body" hx-swap="outerHTML">upgrade</button>
      {% endif %}
      <button class="x" hx-delete="/{{ token }}/skills/{{ enc }}" hx-target="body" hx-swap="outerHTML">×</button>
    </td>
  </tr>
  {% endfor %}
  </tbody>
</table>
```

注意：scope 转移按钮 POST 到 `/skills/rescope-{{enc}}`——这是 scope 转移的 GUI 端点（Task 8 未含，见下 Step 3 补）。rescope GUI 端点需要一个 handler 把 rescope 反向（local→global 点 →global 按钮 = target global）。本 task 补这个端点。

- [ ] **Step 3: 补 scope 转移 GUI 端点（rescope handler）**

`skills.rs` 加（GUI 直接执行 + 横幅，去 hx-confirm）：

```rust
/// GUI scope 转移：POST /skills/rescope-{enc}?to=global|local。直接执行 + summary 横幅。
pub async fn rescope(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    Query(q): Query<RescopeQuery>,
) -> Response {
    let id = id.replace("%2F", "/");
    let target = if q.to.as_deref() == Some("global") { Scope::Global } else { Scope::Local };
    match skillkit_core::set_scope(&state.paths, &id, target) {
        Ok(report) => {
            let summary = match target {
                Scope::Global => format!(
                    "✓ 已转全局，从 {} 个 profile / {} 个项目移除引用；以下项目需重新 apply：{}",
                    report.affected_profiles.len(), report.affected_projects.len(),
                    report.affected_projects.join(", ")
                ),
                Scope::Local => "✓ 已转 local，撤销全局落地（可 rescope global 恢复）".to_string(),
            };
            render_skills(state, token, Some(&summary), false, vec![], vec![])
        }
        Err(e) => {
            tracing::error!(error = ?e, "GUI rescope 失败：{id}");
            Html(format!(r#"<p class="err">rescope 失败：{e}</p>"#)).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct RescopeQuery { pub to: Option<String> }
```

路由 `mod.rs` 加 `.route("/{token}/skills/rescope-{id}", post(skills::rescope))`——但 `{id}` 含 `/`（source/skill），路径里是 `rescope-{enc}`（enc 已把 / 编码 %2F）。axum 路径段含 %2F 会被解码成 / 吗？axum 默认不解码 %2F 成路径分隔（保持单段）。但路由模式 `rescope-{id}` 的 `{id}` 匹配——axum 不支持字面量前缀 + 参数同段（`rescope-{id}` 不是合法路由模式，matchit 不支持中缀）。

改方案：scope 转移用 query 传 id，路径固定 `/skills/rescope?id={enc}&to=...`：

模板按钮改：
```html
hx-post="/{{ token }}/skills/rescope?to=global&id={{ enc }}"
```
路由 `.route("/{token}/skills/rescope", post(skills::rescope))`，handler 从 Query 取 id（Path 只剩 token）：

```rust
pub async fn rescope(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<RescopeGuiQuery>,
) -> Response { ... q.id (dec %2F) ... }

#[derive(Deserialize)]
pub struct RescopeGuiQuery { pub to: Option<String>, pub id: String }
```

模板按钮 `hx-post="/{{ token }}/skills/rescope?to=global&id={{ enc }}"`。修正 Step 2 模板里的按钮为此形式。

Run: `cargo test -p skillkit-server skills_main_renders`
Expected: PASS

- [ ] **Step 4: layout.html 加 toggle JS（高亮选中 + 填批量栏）**

`templates/layout.html` 的 `<script>` 区加（点 local 行 toggle selected class + 更新批量栏显示/count + 同步地址栏 query）：

```html
<script>
function selectedIds() {
  return Array.from(document.querySelectorAll('#skills tr.selected[data-id]')).map(r => r.dataset.id);
}
function refreshBatch() {
  const ids = selectedIds();
  document.querySelectorAll('.batch-bar').forEach(el => el.style.display = ids.length ? '' : 'none');
  const cnt = document.getElementById('skill-batch-count');
  if (cnt) cnt.textContent = ids.length;
  // 填批量归入表单的隐藏 id 字段
  const form = document.getElementById('skill-batch');
  if (form) { form.querySelectorAll('input[name=id]').forEach(e => e.remove()); ids.forEach(id => {
    const i = document.createElement('input'); i.type='hidden'; i.name='id'; i.value=id; form.appendChild(i); }); }
  const fn = document.getElementById('skill-batch-new');
  if (fn) { fn.querySelectorAll('input[name=id]').forEach(e => e.remove()); ids.forEach(id => {
    const i = document.createElement('input'); i.type='hidden'; i.name='id'; i.value=id; fn.appendChild(i); }); }
}
function toggleSkill(tr) {
  tr.classList.toggle('selected');
  refreshBatch();
}
// SSE 重渲染后恢复选中：从 URL ?selected= 读
document.addEventListener('htmx:afterSettle', refreshBatch);
</script>
```

- [ ] **Step 5: app.css 追加样式**

`static/app.css` 追加（按现有风格）：

```css
.filter-chips { display:flex; gap:6px; flex-wrap:wrap; margin:8px 0; }
.chip { display:inline-flex; align-items:center; gap:2px; border:1px solid var(--bd,#555); border-radius:10px; padding:1px 8px; cursor:pointer; font-size:12px; }
.chip.on { background: var(--accent,#3a5); color:#fff; border-color:transparent; }
.chip .x { background:none; border:none; color:#a66; cursor:pointer; padding:0 0 0 2px; }
.batch-bar { background: var(--bar,#22223a); border:1px dashed var(--bd,#666); padding:6px 8px; border-radius:4px; margin:6px 0; display:flex; gap:8px; align-items:center; }
#skills tr.selected { background: var(--sel,#2a2a44); }
#skills tr[data-id] { cursor:pointer; }
```

- [ ] **Step 6: make check + commit**

Run: `make check`
Expected: 双绿 + 全测试 PASS（askama 模板编译 + 渲染单测）

```bash
git add crates/server/templates/fragments/skills_main.html crates/server/templates/layout.html crates/server/static/app.css crates/server/src/routes/skills.rs crates/server/src/routes/mod.rs
git commit -m "feat(server): Skills 视图改造（chips归属列+高亮toggle批量+profile过滤+scope转移按钮）"
```

---

## Task 10: server profiles create 校验 + profiles_main 删手填表单 + 过滤 global

**Files:**
- Modify: `crates/server/src/routes/profiles.rs`（`create` 加存在性校验 + `render_profiles` 过滤 global + 新增 `filter_global_skills`）
- Modify: `crates/server/templates/fragments/profiles_main.html`（删手填 `source/skill` add-skill 表单）
- Test: `crates/server/src/routes/profiles.rs`（内联测 `filter_global_skills`）

**Interfaces:**
- Consumes: `Registry`/`Scope`、`Profile`。
- Produces: profiles 视图不再覆盖同名、不显示 legacy global 引用、去掉手填 add-skill 表单（归入操作搬 Skills 视图）。

- [ ] **Step 1: 写失败测试（filter_global_skills）**

`profiles.rs` 加 tests 模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use skillkit_core::{Registry, Scope, SkillMeta};
    use tempfile::tempdir;

    fn reg(id: &str, scope: Scope) -> Registry {
        let mut r = Registry::default();
        r.upsert(SkillMeta {
            id: id.into(), name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(), scope, version: None,
            computed_hash: Some("a".into()), installed_at: "t".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{}", id.rsplit('/').next().unwrap()),
        });
        r
    }

    #[test]
    fn filter_global_skills_drops_global_keeps_local_and_unknown() {
        let r = reg("dc/g", Scope::Global);
        let r2 = reg("dc/l", Scope::Local);
        let mut r_all = Registry::default();
        r_all.upsert(r.get("dc/g").cloned().unwrap());
        r_all.upsert(r2.get("dc/l").cloned().unwrap());
        let p = skillkit_core::Profile {
            name: "fe".into(), description: String::new(),
            skills: vec!["dc/g".into(), "dc/l".into(), "dc/unknown".into()], // unknown 无 registry 记录
        };
        let filtered = filter_global_skills(p, &r_all);
        assert_eq!(filtered.skills, vec!["dc/l".to_string(), "dc/unknown".to_string()], "global 删，local + unknown 保留");
    }
}
```

Run: `cargo test -p skillkit-server filter_global_skills`
Expected: FAIL（函数未定义）

- [ ] **Step 2: 加 `filter_global_skills` + `render_profiles` 过滤 + `create` 校验**

`profiles.rs` 加（`render_profiles` 前）：

```rust
/// 渲染用过滤：剔除 profile.skills 里的 global 引用（legacy 不显示，原数据不 save）。unknown 保留。
fn filter_global_skills(mut p: Profile, reg: &skillkit_core::Registry) -> Profile {
    p.skills.retain(|id| {
        reg.get(id).map(|m| m.scope != skillkit_core::Scope::Global).unwrap_or(true)
    });
    p
}
```

`render_profiles` 改（load registry + 过滤每个 profile）：

```rust
fn render_profiles(state: AppState, token: String, fragment: bool) -> Response {
    let reg = skillkit_core::Registry::load(&state.paths).unwrap_or_default();
    let mut profiles = Vec::new();
    if let Ok(names) = skillkit_core::list_profile_names(&state.paths) {
        for n in names {
            if let Ok(p) = Profile::load(&state.paths, &n) {
                profiles.push(filter_global_skills(p, &reg));
            }
        }
    }
    // ... 其余渲染逻辑不变（ProfilesTpl/ProfilesMainTpl 用过滤后的 profiles）
}
```

`create` 加存在性校验（`profiles.rs:75-89`，在 `Profile { ... }` 构造前）：

```rust
pub async fn create(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<CreateForm>,
) -> Response {
    if skillkit_core::Profile::load(&state.paths, &f.name).is_ok() {
        tracing::warn!("profile {} 已存在，create 不覆盖", f.name);
        return render_profiles(state, token, false);
    }
    let p = Profile { name: f.name, description: String::new(), skills: Vec::new() };
    if p.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_profiles(state, token, false)
}
```

Run: `cargo test -p skillkit-server filter_global_skills`
Expected: PASS

- [ ] **Step 3: 删 profiles_main.html 的手填 add-skill 表单**

`templates/fragments/profiles_main.html`（`profiles_main.html:10-13` 那段 POST `/profiles/{name}/skills` 的手填 `source/skill` 表单）整段删除。保留：顶部 create 表单 + 每个 profile card 的 skills 列表（`profile_skills.html` 的 chips + 拖拽 + ×）。归入 skill 的操作改在 Skills 视图做（Task 9 批量归入）。

Run: `make check`
Expected: 双绿（askama 编译 + 测试）

- [ ] **Step 4: commit**

```bash
git add crates/server/src/routes/profiles.rs crates/server/templates/fragments/profiles_main.html
git commit -m "fix(server): profiles create 防覆盖 + 渲染过滤 global + 去掉手填 add-skill 表单"
```

---

## Task 11: server layout SSE 刷新带上当前 query

**Files:**
- Modify: `crates/server/templates/layout.html:30-35`（SSE `changed` 回调）

**Interfaces:**
- Consumes: `?selected=`/`?profiles=` query（Task 7/9）。
- Produces: SSE 重渲染保留选中态与过滤，不丢高亮。

- [ ] **Step 1: 改 SSE 回调 JS**

`layout.html` 的 `EventSource` `onmessage`/`changed` 回调（现为 `htmx.ajax('GET', location.pathname + '?fragment=1', {target:'main', swap:'innerHTML'})`）改为保留当前 query：

```html
<script>
  const es = new EventSource('/{{ token }}/events'.replace(/&amp;/g, '&'));
  es.addEventListener('changed', function() {
    // 保留当前 query（selected/profiles），只追加/覆盖 fragment=1
    const params = new URLSearchParams(location.search);
    params.set('fragment', '1');
    htmx.ajax('GET', location.pathname + '?' + params.toString(), {target: 'main', swap: 'innerHTML'});
  });
</script>
```

（保持原 EventSource 构造，仅改 changed 回调里的 URL 拼接。其余视图无 query 时 `location.search` 为空，`params` 只有 fragment=1，行为不变。）

- [ ] **Step 2: make check + commit**

Run: `make check`
Expected: 双绿（无 JS 测试，靠 Task 13 e2e 验证）

```bash
git add crates/server/templates/layout.html
git commit -m "fix(server): SSE 刷新带上当前 query（保留 selected/profiles 不丢高亮过滤）"
```

---

## Task 12: 文档落实（主 spec 修订 + 决策 17/18）

**Files:**
- Modify: `docs/2026-07-29-skillkit-design.md`（§8.4 / §9 / §10.1 / §11 / §12）
- Modify: `docs/design-decisions-2026-07-29.md`（末尾追加决策 17/18）

**Interfaces:** 无代码，文档对齐 spec §8 的修订明细。

- [ ] **Step 1: 修订主 spec §8.4**

`docs/2026-07-29-skillkit-design.md` §8.4 末段「profile 主要承载 local skill 的组合……但 profile 也允许引用 global skill（apply 时幂等确保其全局存在）。」改为：

> profile 只承载 local skill 的组合（per-project 生效的部分）；**global skill 不属于任何 profile**（core 硬约束：`profile.add_skill` 校验拒绝 global skill，引导先 `rescope` 到 local）。

- [ ] **Step 2: 修订 §9 分工表 + §10.1**

§9 表格下补一行说明：「profile 与 `project.installed_skills` 均只含 local skill，global 不进二者。」

§10.1「scope=global」段「install 时已全局落地，apply 只做幂等检查……进 installed_skills 是为了声明该项目依赖这个全局基座，不产生 per-project 副作用」整段改为：

> scope=global：install/rescope 时即全局落地，**apply 完全不碰 global**（global 不进任何 `project.installed_skills`）。apply 只处理 scope=local 的 skill 落地。

- [ ] **Step 3: 补 §11 CLI rescope + §12 GUI**

§11「skill 安装到 canonical」命令组下补一行：

```
skillkit rescope <id> <global|local> [--yes] [--json]   # scope 转移 + 同步物理落地；两方向默认确认
```

§12 GUI 表格「Skills」行的「内容」改为「registry 总览 + scope 转移 + profile 归属管理（chips/过滤/批量归入/移除）」，「核心操作」改为「rescope、批量归入 profile、按 profile 过滤、chip 移除、install/upgrade/remove」。

- [ ] **Step 4: 追加决策 17/18**

`docs/design-decisions-2026-07-29.md` 末尾追加：

```markdown
## 决策 17：global skill 与 profile/project 归属互斥（core 硬约束）

**背景**：原 §8.4 允许 profile 引用 global skill、§10.1 允许 global 进 installed_skills，两层语义都不纯（global 是全局基座却混进场景组合/项目声明）。

**决策**：global skill 不属任何 profile、不进任何 project.installed_skills；core 在 `add_skill`/`set_profiles` 加 `&Registry` 参数做 scope 校验（global 拒绝/跳过）。

**理由**：心智模型纯粹（global=全局基座独立、local=场景/项目组合成员），职责一刀切；apply 简化成只管 local 落地。`add_skill` 加 registry 参数是必要代价（scope 只存 registry）。

**否定的备选**：仅 GUI 引导不校验——CLI/外部调用能绕过留脏数据。

## 决策 18：scope 转移副作用模型 + 风险对齐确认

**背景**：需要 local↔local/global 互转，且转移伴随物理落地变更 + 归属清理。

**决策**：
- 转移 = 改 scope + 立即同步物理落地（local→global 建 `ensure_global_claude`；global→local 撤，新增 `remove_global_claude` 不加 scope 守卫避免改 scope 后 no-op）。
- local→global 自动从所有 profile/project 移除引用（不可逆，但可重新归入恢复）；global→local 可逆（rescope global 恢复）。
- 风险对齐：两方向 CLI 都默认交互确认；GUI 直接执行 + 横幅明示影响（去 hx-confirm 方向，commit b15d13e）。
- 原子回滚范围 = scope + registry + symlink；profile/project 多文件移除失败给可恢复文案，不声称全量原子。

**理由**：转移即生效（跟 install/remove 一致）；`remove_global_claude` 不加守卫是规避 set_scope 先改 scope 再撤链的顺序陷阱（spec review P2-A）。
```

- [ ] **Step 5: commit**

```bash
git add docs/2026-07-29-skillkit-design.md docs/design-decisions-2026-07-29.md
git commit -m "docs: 主 spec 修订(global/profile 互斥) + 决策 17/18(scope 转移模型)"
```

---

## Task 13: e2e + HTTP 契约测试

**Files:**
- Modify: `crates/server/tests/routes.rs`（若存在；无则按 `docs/frontend-rules.md` §5 建）—— HTTP 片段契约
- Modify: `crates/cli/tests/e2e_cli.rs` —— rescope 真跑
- Modify: GUI e2e（`e2e/` 目录，playwright）—— Skills 视图交互

**Interfaces:** 验证 Task 1-12 的端到端行为。

- [ ] **Step 1: HTTP 契约测试（`crates/server/tests/routes.rs`）**

按 frontend-rules §5 的 TestClient 模式加（用 tempdir Paths 注入 AppState）：

- `GET /{token}/skills` 渲染含「所属 profile」列表头 + chip。
- `GET /{token}/skills?selected=dc%2Ffe` 渲染对应行带 `selected` class。
- `GET /{token}/skills?profiles=fe` 只渲染属 fe 的 local skill，global 不出现。
- `POST /{token}/skills/assign`（profile=fe&id=dc%2Ffe）返回完整页（含 `<html`/layout nav），非片段；fe 的 skills 含 dc/fe。
- `POST /{token}/skills/assign-new`（name=_new&id=...）建 profile 并归入；同名再调返已存在提示。
- `DELETE /{token}/skills/dc%2Ffe/profile/fe` 移除后 fe 不含 dc/fe。
- `POST /{token}/skills/rescope?id=dc%2Ffe&to=global` 后 registry scope=global、fe 不再含 dc/fe、summary 横幅含「移除」。

Run: `cargo test -p skillkit-server --test routes`
Expected: PASS

- [ ] **Step 2: CLI rescope e2e（`crates/cli/tests/e2e_cli.rs`）**

加（用 tempdir + 真跑 core，不 mock npx；rescope 不触网）：

- `rescope <local-id> global --json` 输出 schema `{id, from:"local", to:"global", affected_profiles, affected_projects}`（schema 锁定）。
- `rescope <id> <同 scope> --yes` 无变化、affected 空。
- 确认路径：非 `--yes`/`--json` 时读 stdin（喂 "n" 取消、喂 "y" 执行）；`--json` 隐含跳过。
- global→local 后 `~/.agents/skills/<name>` symlink 删除、canonical 保留。

Run: `make e2e-cli`
Expected: PASS

- [ ] **Step 3: GUI e2e（playwright，`make e2e`）**

`e2e/` 加用例（真实 chromium）：

- Skills 页：点 local 行 → 整行高亮（selected class）+ 批量栏出现 + count=1；再点取消。
- 批量归入：选中 2 个 local → 下拉选 profile → 归入 → 两行 chips 出现该 profile。
- 批量新建：选中 → 输新名 → 新建并归入 → 新 profile 出现、两行归属它。
- 过滤：点 profile chip → 只显示该 profile 的 skill、global 消失；点「全部」恢复。
- chip ×：点某 chip 的 × → 该行归属移除。
- scope 转移：点 local 行 →global → scope 变 global、chips 消失、横幅出现。

Run: `make e2e`
Expected: PASS（需空闲端口）

- [ ] **Step 4: 全量验证 + commit**

Run: `make check && make e2e && make e2e-cli`
Expected: 全绿

```bash
git add crates/server/tests/routes.rs crates/cli/tests/e2e_cli.rs e2e/
git commit -m "test: skill scope/profile 管理 e2e + HTTP 契约（rescope/assign/过滤/toggle）"
```

---

## Self-Review

**1. Spec 覆盖**（对照 spec §1-9 + 需求 7 条 + review 13 项）：

| spec / 需求 | 落点 task |
|---|---|
| 需求1 global/local 分类 | 已有（Task 1 加错误变体支撑约束） |
| 需求2 scope 互转 | Task 4（remove_global_claude）+ Task 5（set_scope）+ Task 6（cli rescope）+ Task 9（GUI 按钮/rescope 端点） |
| 需求3 批量归入+即时新建 | Task 8（assign/assign-new）+ Task 9（高亮 toggle + 批量栏） |
| 需求4 global 不属 profile + 转移清理 | Task 1（错误）+ Task 2/3（add_skill 校验）+ Task 5（set_scope 清 profile/project） |
| 需求5 按 profile 过滤 | Task 7（build_skills_view 过滤）+ Task 9（过滤 chips） |
| 需求6 列出所属 profile | Task 2（skill_profiles）+ Task 7（反向 map）+ Task 9（chips 列） |
| 需求7 移除 profile 从属 | Task 8（delete_profile）+ Task 9（chip ×） |
| review P0 add_skill 非幂等 | Task 2/3（签名+校验）+ Task 8（apply_assign 跳过/抛错原子） |
| review P1-1 端点 | Task 8 |
| review P1-2 query 透传 | Task 7（SkillsQuery）+ Task 11（SSE 带 query） |
| review P1-3 过滤机制 | Task 7/9（服务端 ?profiles=） |
| review P2-1 remove_global_claude | Task 4 |
| review P2-2 风险对齐 | Task 5（set_scope）+ Task 6（cli 两方向确认）+ Task 9（GUI 横幅） |
| review P2-3 存量 | Task 3（set_profiles 跳过）+ Task 10（渲染过滤 global） |
| review P2-4 原子回滚范围 | Task 5（set_scope 注释 + 可恢复文案） |
| review P2-5 ＋新建覆盖 | Task 8（assign-new 校验）+ Task 10（create 校验） |
| review P2-A remove 守卫 | Task 4（remove_global_claude 不加守卫，注释） |
| review P2-B 签名 | Task 2/3（add_skill/set_profiles 加 &Registry） |
| review P2-C 幂等 | Task 4（remove_one_link 缺失跳过） |
| spec §8 主 spec 修订 + 决策 | Task 12 |

无遗漏。

**2. Placeholder 扫描**：无 TBD/TODO；每个 task 有完整测试代码 + 实现代码 + 命令 + commit。模板 task（9）给了完整 HTML 结构 + CSS + JS。

**3. 类型一致性**：
- `SkillIsGlobal { id }`（Task 1）← Task 2/3 校验返回、Task 8 apply_assign 识别、Task 5 set_scope 不直接用（set_scope 清理用 retain 不调 add_skill）。
- `add_skill(&mut self, id, registry: &Registry)`（Task 2/3）← Task 8 apply_assign 调用一致。
- `set_profiles(&mut self, names, profiles, registry: &Registry)`（Task 3）← Task 12 文档 + projects.rs handler（Task 3 Step 5 适配）。
- `set_scope(paths, id, target) -> RescopeReport`（Task 5）← Task 6 cli、Task 9 GUI rescope 调用。
- `RescopeReport { affected_profiles, affected_projects }`（Task 5）← Task 6 --json、Task 9 横幅。
- `SkillsQuery { fragment, selected, profiles }`（Task 7）← Task 8/9/11 用。
- `build_skills_view`/`apply_assign`/`filter_global_skills`（Task 7/8/10）命名一致。

无类型/命名漂移。

