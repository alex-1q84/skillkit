# skill find / list / remove 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: 用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐 task 实现。步骤用 `- [ ]` 跟踪。

**Goal:** 给 skillkit CLI 增加 `find`（搜 skills.sh）/ `list`（列已装）/ `remove`（卸载，完全替换 uninstall）三个顶层命令，并同步 GUI 原型 Skills 视图。

**Architecture:** 复用 core 现有能力（`npx::find` / `Registry` / `uninstall`），cli 新增薄壳模块 `commands/skill.rs` 承载三个顶层命令；`install.rs` 删除 `Uninstall` 回归「安装」单一职责，其 registry 源 `--json` 分支改为复用 `skill::print_candidates`。GUI 原型 Skills 视图按 server 真实 GUI 补全（find 搜索框、remove ×、unmanaged badge、列对齐）。

**Tech Stack:** Rust 2021 / clap derive / serde / anyhow（cli）/ thiserror（core）/ assert_cmd + tempfile（e2e）。

## Global Constraints

- 路径绝不硬编码，一律 `Paths::production()` / `dirs::home_dir()`（CLAUDE.md §7）。
- cli 顶层用 `anyhow` 聚合；core 已用 `thiserror`，本计划不动 core。
- `--json` 输出 schema 视为公开契约，变更需谨慎，三者均加 schema 锁定测试（CLAUDE.md §6/§8）。
- 危险操作（remove）默认交互确认，`--yes` 跳过，`--json` 隐含跳过（CLAUDE.md §6）。
- 注释中文；commit message 中文 + Conventional Commits；不自动 git，每个 task 末尾 commit 由执行阶段按主人指示触发。
- 测试里跑 `git commit` 必带 `-c user.email -c user.name`（CLAUDE.md §8）。
- 改完源码必跑 `make format && make lint`（§9）；core 公开类型经 `lib.rs` re-export。
- 文件输出走 `println!`（CLI 用户输出），日志才用 `tracing`（现有 install.rs 同风格）。

## File Structure

- **Create** `crates/cli/src/commands/skill.rs` — skill 实体三命令（Find/List/Remove）+ 公共 `print_candidates` + 渲染纯函数 + 内联单测。
- **Modify** `crates/cli/src/commands/mod.rs` — 加 `pub mod skill;`。
- **Modify** `crates/cli/src/main.rs` — `Cmd` 加 `Find`/`List`/`Remove`、删 `Uninstall`，`match` 分发同步。
- **Modify** `crates/cli/src/commands/install.rs` — 删 `UninstallCmd`/`run_uninstall`/`print_registry_candidates`；registry 源 `--json` 分支改调 `skill::print_candidates`。
- **Modify** `crates/cli/tests/e2e_cli.rs` — `uninstall_*` 用例改 `remove`，新增 remove 确认/find/list e2e。
- **Modify** `README.md` — 命令参考 `uninstall` → `remove`，补 `find`/`list`。
- **Modify** `docs/sessions/2026-07-29-skillkit-design.md` §1.1 — 命令表面更新。
- **Modify** `demo/index.html` — Skills 视图补 find 搜索框 / remove × / unmanaged badge + mock / upgrade 仅 managed / install 切 scope 表单 / 列对齐 server。

---

### Task 1: skill.rs 骨架 + find 命令

**Files:**
- Create: `crates/cli/src/commands/skill.rs`
- Modify: `crates/cli/src/commands/mod.rs`
- Modify: `crates/cli/src/main.rs`

**Interfaces:**
- Consumes: `skillkit_core::npx::find(paths, &query) -> Result<Vec<Candidate>>`（`npx.rs:50`，`Candidate = {spec:String, url:Option<String>}`，已 `derive(Serialize)`）、`skillkit_core::Paths::production()`、`skillkit_core::Candidate`。
- Produces: `pub struct FindCmd { pub query: String, pub json: bool }`、`pub fn run_find(FindCmd) -> anyhow::Result<()>`、`pub fn print_candidates(&Paths, query: &str, json: bool) -> anyhow::Result<()>`（Task 4 install 复用）。

- [ ] **Step 1: 写失败测试（clap 解析 + Candidate --json schema）**

新建 `crates/cli/src/commands/skill.rs`，先只放测试骨架（实现暂缺，编译应失败）：

```rust
//! skill 实体的查询与移除：find（搜 skills.sh）/ list（列已装）/ remove（卸载，替换 uninstall）。
//! 复用 core 的 npx::find / Registry / uninstall，cli 只做薄壳与展示。
use clap::Args;
use skillkit_core::paths::Paths;

/// find：skillkit find <query> [--json]，搜 skills.sh registry，纯展示候选不安装。
#[derive(Args)]
pub struct FindCmd {
    /// skill 名（搜 skills.sh registry）
    pub query: String,
    /// JSON 输出：候选数组 [{spec,url}]
    #[arg(long)]
    pub json: bool,
}

/// 输出 find 候选：json=true 序列化数组，否则编号列表。install 的 registry 源 --json 分支也复用。
pub fn print_candidates(_paths: &Paths, _query: &str, _json: bool) -> anyhow::Result<()> {
    unimplemented!("Task 1 Step 3 实现")
}

pub fn run_find(_cmd: FindCmd) -> anyhow::Result<()> {
    unimplemented!("Task 1 Step 3 实现")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};
    use skillkit_core::Candidate;

    /// 测试入口：自建同形 Parser 解析顶层命令（main.rs 的 Cli 私有，这里复刻命令变体）。
    /// 后续 task 给 TestCmd 累积追加 List/Remove 变体。
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(Subcommand)]
    enum TestCmd {
        Find(FindCmd),
    }

    #[test]
    fn find_parses_query_and_json() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "find", "pdf", "--json"]);
        let TestCmd::Find(FindCmd { query, json }) = cmd else {
            panic!("expected Find")
        };
        assert_eq!(query, "pdf");
        assert!(json);
    }

    #[test]
    fn find_defaults_json_false() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "find", "pdf"]);
        let TestCmd::Find(FindCmd { json, .. }) = cmd else {
            panic!("expected Find")
        };
        assert!(!json);
    }

    /// --json schema 锁定：Candidate 序列化为 {"spec","url"}（不依赖 npx，纯序列化契约）。
    #[test]
    fn find_json_schema_locks_candidate_fields() {
        let cs = vec![
            Candidate { spec: "anthropics/skills@pdf".into(), url: Some("https://skills.sh/a".into()) },
            Candidate { spec: "openai/skills@pdf".into(), url: None },
        ];
        let json = serde_json::to_string(&cs).unwrap();
        assert_eq!(json, r#"[{"spec":"anthropics/skills@pdf","url":"https://skills.sh/a"},{"spec":"openai/skills@pdf","url":null}]"#);
    }
}
```

- [ ] **Step 2: 注册模块让测试可发现，跑测试看失败**

`crates/cli/src/commands/mod.rs` 末尾加一行：

```rust
pub mod skill;
```

`crates/cli/src/main.rs` 改两处。`use` 区（第 5-11 行）加：

```rust
use commands::skill::FindCmd;
```

`Cmd` 枚举（第 21-38 行）在 `Install(InstallCmd)` 后加变体：

```rust
    /// 搜 skills.sh registry 中的 skill 候选
    Find(FindCmd),
```

`match cli.cmd`（第 44-53 行）加分支：

```rust
        Cmd::Find(cmd) => commands::skill::run_find(cmd)?,
```

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests 2>&1`
预期：`find_parses_query_and_json` / `find_defaults_json_false` / `find_json_schema_locks_candidate_fields` 三个 PASS（前两个纯解析，第三个纯序列化，均不触达 `unimplemented!`）。

- [ ] **Step 3: 实现 print_candidates 与 run_find**

替换 `crates/cli/src/commands/skill.rs` 中两个 `unimplemented!` 函数：

```rust
use skillkit_core::npx;   // 补到文件顶部 use 区

/// 输出 find 候选：json=true 序列化数组，否则编号列表。install 的 registry 源 --json 分支也复用。
pub fn print_candidates(paths: &Paths, query: &str, json: bool) -> anyhow::Result<()> {
    let cs = npx::find(paths, query)?;
    if cs.is_empty() {
        anyhow::bail!("在 skills.sh 未找到 skill：{query}");
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&cs)?);
    } else {
        println!("在 skills.sh 找到 {} 个候选：", cs.len());
        for (i, c) in cs.iter().take(20).enumerate() {
            println!("  [{i}] {}  {}", c.spec, c.url.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn run_find(cmd: FindCmd) -> anyhow::Result<()> {
    print_candidates(&Paths::production(), &cmd.query, cmd.json)
}
```

顶部 `use` 合并为一处：

```rust
use clap::Args;
use skillkit_core::{npx, paths::Paths};
```

- [ ] **Step 4: 跑测试 + 编译**

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests 2>&1 && cargo build -p skillkit-cli 2>&1`
预期：三测全 PASS；编译通过。

- [ ] **Step 5: 加 find 真跑 npx 的 e2e（#[ignore]）**

`crates/cli/tests/e2e_cli.rs` 在 import-existing 段后（第 90 行下方分隔注释后）插入新段：

```rust
// ===========================================================================
// find（真跑 npx skills find）
// ===========================================================================

#[test]
#[ignore = "需真跑 npx skills find（联网）；cargo test -- --ignored 手动跑"]
fn find_json_returns_candidate_array() {
    // Given/When：find pdf --json（query 选 skills.sh 上确实存在的 skill 名）
    let env = Env::new();
    let out = env.skillkit().args(["find", "pdf", "--json"]).assert().success();
    // Then：stdout 是 JSON 数组，元素含 spec 字段（不断言具体值，skills.sh 内容会变）
    let body: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("find --json 应输出合法 JSON 数组");
    let arr = body.as_array().expect("应为数组");
    assert!(!arr.is_empty(), "pdf 应至少有一个候选");
    assert!(arr[0].get("spec").is_some(), "候选元素含 spec 字段");
}
```

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests 2>&1`
预期：常规三测仍 PASS（新 e2e 默认跳过）。

- [ ] **Step 6: format + lint**

运行：`make format && make lint 2>&1`
预期：fmt 无改动（或已应用），clippy `-D warnings` 零 warning。

- [ ] **Step 7: commit**

```bash
git add crates/cli/src/commands/skill.rs crates/cli/src/commands/mod.rs crates/cli/src/main.rs crates/cli/tests/e2e_cli.rs
git commit -m "feat(cli): 新增 skill find 命令——搜 skills.sh 候选 + --json"
```

---

### Task 2: list 命令

**Files:**
- Modify: `crates/cli/src/commands/skill.rs`
- Modify: `crates/cli/src/main.rs`

**Interfaces:**
- Consumes: `skillkit_core::Registry::load(&Paths) -> Result<Registry>`（`Registry.skills: BTreeMap<String, SkillMeta>`）、`skillkit_core::SkillMeta`（9 字段：id/name/source/scope/version/computed_hash/installed_at/canonical_path）、`skillkit_core::Scope`（`Global`/`Local`）。
- Produces: `pub struct ListCmd { pub json: bool }`、`pub fn run_list(ListCmd) -> anyhow::Result<()>`、私有 `fn render_list_table(&[SkillMeta]) -> String` / `fn render_list_json(&[SkillMeta]) -> anyhow::Result<String>` / `fn scope_str(Scope) -> &'static str`（Scope 是 Copy，传值，避免 clippy `trivially_copy_pass_by_ref`）。

- [ ] **Step 1: 写失败测试（clap 解析 + 渲染纯函数 + schema）**

在 `skill.rs` 的 `mod tests` 内追加：

```rust
    use skillkit_core::{Scope, SkillMeta};

    fn meta(id: &str, scope: Scope, hash: Option<&str>) -> SkillMeta {
        SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: Some("1.0.0".into()),
            computed_hash: hash.map(str::to_string),
            installed_at: "2026-08-01T00:00:00Z".into(),
            canonical_path: format!("~/.skillkit/.agents/skills/{}", id.rsplit('/').next().unwrap()),
        }
    }

    #[test]
    fn list_parses_json_flag() {
        // 先给 Task 1 的 TestCmd 枚举追加 List 变体：`List(ListCmd)`
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "list", "--json"]);
        let TestCmd::List(ListCmd { json }) = cmd else {
            panic!("expected List")
        };
        assert!(json);
    }

    #[test]
    fn list_table_marks_unmanaged() {
        let skills = vec![
            meta("skills.sh/pdf", Scope::Global, Some("abc123")),
            meta("unmanaged/legacy", Scope::Global, None),
        ];
        let table = render_list_table(&skills);
        assert!(table.contains("skills.sh/pdf"));
        assert!(table.contains("[global]"));
        assert!(table.contains("unmanaged/legacy"));
        assert!(table.contains("unmanaged")); // unmanaged 行有标识
    }

    /// --json schema 锁定：SkillMeta[] 字段名稳定。
    #[test]
    fn list_json_schema_locks_skillmeta_fields() {
        let skills = vec![meta("skills.sh/pdf", Scope::Local, Some("abc123"))];
        let json = render_list_json(&skills).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = &v[0];
        assert_eq!(obj["id"], "skills.sh/pdf");
        assert_eq!(obj["scope"], "local");            // lowercase（serde rename_all）
        assert_eq!(obj["computed_hash"], "abc123");
        assert_eq!(obj["source"], "skills.sh");
        assert!(obj["installed_at"].is_string());
        assert!(obj["canonical_path"].is_string());
    }
```

- [ ] **Step 2: 跑测试看失败**

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests 2>&1`
预期：编译失败——`ListCmd` / `render_list_table` / `render_list_json` 未定义。

- [ ] **Step 3: 实现 ListCmd + 渲染 + run_list**

在 `skill.rs` 顶部 `use` 区加 `Registry, Scope, SkillMeta`：

```rust
use clap::Args;
use skillkit_core::{npx, paths::Paths, Registry, Scope, SkillMeta};
```

在 `run_find` 之后追加：

```rust
/// list：skillkit list [--json]，列 registry 全部已装 skill。
#[derive(Args)]
pub struct ListCmd {
    /// JSON 输出：SkillMeta[]
    #[arg(long)]
    pub json: bool,
}

fn scope_str(s: Scope) -> &'static str {
    match s {
        Scope::Global => "global",
        Scope::Local => "local",
    }
}

/// 渲染 list 表格（人看）。unmanaged（computed_hash=None）行尾标 unmanaged。
fn render_list_table(skills: &[SkillMeta]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for s in skills {
        let hash = s.computed_hash.as_deref().unwrap_or("-");
        let unm = if s.computed_hash.is_none() { "  (unmanaged)" } else { "" };
        writeln!(
            out,
            "{id}  [{scope}]  {source}  {ver}  {hash}{unm}",
            id = s.id,
            scope = scope_str(s.scope),
            source = s.source,
            ver = s.version.as_deref().unwrap_or("-"),
            hash = hash,
            unm = unm,
        )
        .unwrap();
    }
    out
}

fn render_list_json(skills: &[SkillMeta]) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(skills)?)
}

pub fn run_list(cmd: ListCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    let reg = Registry::load(&paths)?;
    let mut skills: Vec<SkillMeta> = reg.skills.values().cloned().collect();
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    if cmd.json {
        println!("{}", render_list_json(&skills)?);
    } else if skills.is_empty() {
        println!("（registry 为空，尚无已装 skill）");
    } else {
        print!("{}", render_list_table(&skills));
    }
    Ok(())
}
```

- [ ] **Step 4: 注册到 main.rs**

`main.rs` `use` 改为：

```rust
use commands::skill::{FindCmd, ListCmd};
```

`Cmd` 枚举 `Find(FindCmd)` 后加：

```rust
    /// 列出全部已装 skill
    List(ListCmd),
```

`match` 加分支：

```rust
        Cmd::List(cmd) => commands::skill::run_list(cmd)?,
```

- [ ] **Step 5: 跑测试 + 编译**

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests 2>&1 && cargo build -p skillkit-cli 2>&1`
预期：全部 PASS；编译通过。

- [ ] **Step 6: 加 list e2e（非 ignore，用 import-existing 造数据，不需 npx）**

`e2e_cli.rs` 在 find 段后追加：

```rust
// ===========================================================================
// list（不依赖 npx）
// ===========================================================================

#[test]
fn list_marks_unmanaged_skill() {
    // Given：import-existing 登记一个 unmanaged skill
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-b");
    env.skillkit().args(["import-existing"]).assert().success();

    // When：list（人看输出）
    let out = env.skillkit().args(["list"]).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);

    // Then：输出含该 skill 且标 unmanaged
    assert!(stdout.contains("unmanaged/legacy-b"), "list 应列出 unmanaged skill");
    assert!(stdout.contains("unmanaged"), "unmanaged 行应有标识");

    // And：--json 输出含 id 与 computed_hash=null
    let outj = env.skillkit().args(["list", "--json"]).assert().success();
    let v: serde_json::Value = serde_json::from_slice(&outj.get_output().stdout).unwrap();
    assert_eq!(v[0]["id"], "unmanaged/legacy-b");
    assert!(v[0]["computed_hash"].is_null());
}
```

运行：`cargo test -p skillkit-cli list_ 2>&1`
预期：PASS（非 ignore，直接跑）。

- [ ] **Step 7: format + lint + commit**

```bash
make format && make lint
git add crates/cli/src/commands/skill.rs crates/cli/src/main.rs crates/cli/tests/e2e_cli.rs
git commit -m "feat(cli): 新增 skill list 命令——列已装 skill + unmanaged 标识 + --json"
```

---

### Task 3: remove 命令（与 Uninstall 暂时共存）

**Files:**
- Modify: `crates/cli/src/commands/skill.rs`
- Modify: `crates/cli/src/main.rs`

**Interfaces:**
- Consumes: `skillkit_core::Registry::get(&str) -> Result<&SkillMeta>`、`skillkit_core::uninstall(&Paths, id: &str) -> Result<()>`（`install.rs:58`，内部按 `computed_hash.is_some()` 决定删不删目录）。
- Produces: `pub struct RemoveCmd { pub id: String, pub yes: bool, pub json: bool }`、`pub fn run_remove(RemoveCmd) -> anyhow::Result<()>`。

- [ ] **Step 1: 写失败测试（clap 解析）**

`skill.rs` `mod tests` 追加：

```rust
    #[test]
    fn remove_parses_id_yes_json() {
        // 先给 TestCmd 枚举追加 Remove 变体：`Remove(RemoveCmd)`
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "remove", "skills.sh/pdf", "--yes", "--json"]);
        let TestCmd::Remove(RemoveCmd { id, yes, json }) = cmd else {
            panic!("expected Remove")
        };
        assert_eq!(id, "skills.sh/pdf");
        assert!(yes);
        assert!(json);
    }
```

- [ ] **Step 2: 跑测试看失败**

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests::remove_parses_id_yes_json 2>&1`
预期：编译失败——`RemoveCmd` 未定义。

- [ ] **Step 3: 实现 RemoveCmd + run_remove（含交互确认）**

`skill.rs` 顶部 `use` 加 `uninstall`：

```rust
use skillkit_core::{npx, paths::Paths, uninstall, Registry, Scope, SkillMeta};
```

文件末尾（`run_list` 之后、`mod tests` 之前）追加：

```rust
/// remove：skillkit remove <id> [--yes] [--json]，卸载 skill（完全替换 uninstall）。
/// 默认交互确认；--yes 跳过；--json 隐含跳过并输出 {id, removed_canonical}。
#[derive(Args)]
pub struct RemoveCmd {
    /// skill id，格式 <source>/<skill>
    pub id: String,
    /// 跳过交互确认
    #[arg(long)]
    pub yes: bool,
    /// JSON 输出（隐含 --yes）：{id, removed_canonical}
    #[arg(long)]
    pub json: bool,
}

pub fn run_remove(cmd: RemoveCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    // 先读 registry 判断 managed（决定 removed_canonical + 提示文案），与 uninstall 内部行为一致
    let managed = {
        let reg = Registry::load(&paths)?;
        reg.get(&cmd.id)?.computed_hash.is_some()
    };

    let skip_confirm = cmd.yes || cmd.json;
    if !skip_confirm {
        let note = if managed { "" } else { "（unmanaged：仅删登记，保留目录）" };
        println!("将删除 {id}{note}，确认？(y/n)", id = cmd.id, note = note);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "y" {
            println!("已取消");
            return Ok(());
        }
    }

    uninstall(&paths, &cmd.id)?;

    if cmd.json {
        println!("{}", serde_json::json!({ "id": cmd.id, "removed_canonical": managed }));
    } else {
        let note = if managed { "" } else { "（仅删登记）" };
        println!("✓ 已卸载 {id}{note}", id = cmd.id, note = note);
    }
    Ok(())
}
```

- [ ] **Step 4: 注册到 main.rs（Remove 与现有 Uninstall 暂时共存）**

`main.rs` `use` 改为：

```rust
use commands::skill::{FindCmd, ListCmd, RemoveCmd};
```

`Cmd` 枚举 `List(ListCmd)` 后加：

```rust
    /// 卸载 skill（替换 uninstall）
    Remove(RemoveCmd),
```

`match` 加分支：

```rust
        Cmd::Remove(cmd) => commands::skill::run_remove(cmd)?,
```

- [ ] **Step 5: 跑测试 + 编译**

运行：`cargo test -p skillkit-cli --bin skillkit skill::tests 2>&1 && cargo build -p skillkit-cli 2>&1`
预期：全 PASS；编译通过（`Uninstall` 仍在，两命令共存）。

- [ ] **Step 6: 加 remove 确认交互 e2e（unmanaged，非 ignore，不依赖 npx）**

`e2e_cli.rs` 在 list 段后追加：

```rust
// ===========================================================================
// remove 确认交互（unmanaged，不依赖 npx）
// ===========================================================================

#[test]
fn remove_unmanaged_default_confirm_with_stdin_y() {
    // Given
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-c");
    env.skillkit().args(["import-existing"]).assert().success();

    // When：默认确认，stdin 给 y
    env.skillkit()
        .args(["remove", "unmanaged/legacy-c"])
        .write_stdin("y\n")
        .assert()
        .success();

    // Then：目录保留（unmanaged 保护），registry 移除
    assert!(env.home_path().join(".agents/skills/legacy-c").exists());
    assert!(registry_ids(&env).is_empty());
}

#[test]
fn remove_cancel_with_stdin_n_keeps_registry() {
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-d");
    env.skillkit().args(["import-existing"]).assert().success();

    // stdin 给 n → 取消，registry 记录保留
    env.skillkit()
        .args(["remove", "unmanaged/legacy-d"])
        .write_stdin("n\n")
        .assert()
        .success();
    assert!(registry_ids(&env).contains(&"unmanaged/legacy-d".to_string()));
}

#[test]
fn remove_yes_skips_confirm_and_json_implies_yes() {
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-e");
    env.skillkit().args(["import-existing"]).assert().success();

    // --json 隐含跳过确认，输出 {id, removed_canonical:false}
    let out = env.skillkit()
        .args(["remove", "unmanaged/legacy-e", "--json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["id"], "unmanaged/legacy-e");
    assert_eq!(v["removed_canonical"], false);
    assert!(registry_ids(&env).is_empty());
}
```

运行：`cargo test -p skillkit-cli remove_ 2>&1`
预期：三个 remove 确认测试 PASS。

- [ ] **Step 7: format + lint + commit**

```bash
make format && make lint
git add crates/cli/src/commands/skill.rs crates/cli/src/main.rs crates/cli/tests/e2e_cli.rs
git commit -m "feat(cli): 新增 skill remove 命令——替换 uninstall + 交互确认/--yes/--json"
```

---

### Task 4: 切除 uninstall + 清理 install.rs + 迁移 e2e

**Files:**
- Modify: `crates/cli/src/main.rs`（删 `Cmd::Uninstall` + 分支 + `UninstallCmd` use）
- Modify: `crates/cli/src/commands/install.rs`（删 `UninstallCmd`/`run_uninstall`/`print_registry_candidates`；registry 源 `--json` 分支改调 `skill::print_candidates`）
- Modify: `crates/cli/tests/e2e_cli.rs`（`uninstall_unmanaged_keeps_directory` → `remove_*`，复用 Task 3 已有 e2e；本 task 删旧 uninstall 用例避免重复）

**Interfaces:**
- Consumes: Task 1 的 `commands::skill::print_candidates(&Paths, &str, bool)`。
- Produces: `install.rs` 只剩 `InstallCmd`/`InstallSub`/`run_install`/`resolve_registry_package`/`parse_scope`；`UninstallCmd`/`run_uninstall`/`print_registry_candidates` 删除。

- [ ] **Step 1: 删 main.rs 的 Uninstall**

`main.rs` `use` 区把：

```rust
use commands::install::{InstallCmd, UninstallCmd};
```

改为：

```rust
use commands::install::InstallCmd;
```

`Cmd` 枚举删去变体：

```rust
    /// 卸载 skill
    Uninstall(UninstallCmd),
```

`match` 删去分支：

```rust
        Cmd::Uninstall(cmd) => commands::install::run_uninstall(cmd)?,
```

- [ ] **Step 2: 清理 install.rs**

`crates/cli/src/commands/install.rs`：

(a) `use`（第 5 行）把：

```rust
use skillkit_core::{install, npx, paths::Paths, registry::Scope, source::SourcesStore, uninstall};
```

改为（删 `uninstall`，本文件不再用；`npx` 在 `resolve_registry_package` 仍用，保留）：

```rust
use skillkit_core::{install, npx, paths::Paths, registry::Scope, source::SourcesStore};
```

(b) 删除整个 `UninstallCmd` 结构（第 29-33 行）：

```rust
#[derive(Args)]
pub struct UninstallCmd {
    /// skill id，格式 <source>/<skill>
    pub id: String,
}
```

(c) 删除 `print_registry_candidates`（第 64-72 行，整块）：

```rust
/// registry 源 find 候选输出（--json）：解析 find → 序列化数组，不安装。
fn print_registry_candidates(paths: &Paths, skill: &str) -> anyhow::Result<()> {
    let candidates = npx::find(paths, skill)?;
    if candidates.is_empty() {
        anyhow::bail!("在 skills.sh 未找到 skill：{skill}");
    }
    println!("{}", serde_json::to_string_pretty(&candidates)?);
    Ok(())
}
```

(d) `run_install` 内 registry 源 `--json` 分支（原 `if json { print_registry_candidates(&paths, &skill)?; }`）改为复用 `skill::print_candidates`：

```rust
                None => {
                    if json {
                        crate::commands::skill::print_candidates(&paths, &skill, true)?;
                    } else {
```

(e) 删除 `run_uninstall`（第 117-122 行，整块）：

```rust
pub fn run_uninstall(cmd: UninstallCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    uninstall(&paths, &cmd.id)?;
    println!("✓ 已卸载 {}", cmd.id);
    Ok(())
}
```

- [ ] **Step 3: 编译验证 install.rs 清理无误**

运行：`cargo build -p skillkit-cli 2>&1`
预期：编译通过（无 `unused import`/`unresolved` 错误；`npx` 仍被 `resolve_registry_package` 使用故保留）。

- [ ] **Step 4: 迁移 e2e——删旧 uninstall 用例（已被 Task 3 的 remove e2e 覆盖）**

`e2e_cli.rs` 删除整个 `uninstall_unmanaged_keeps_directory` 测试及其上方分隔注释（第 188-211 行）：

```rust
// ===========================================================================
// uninstall 保护 unmanaged
// ============================================================================

#[test]
fn uninstall_unmanaged_keeps_directory() {
    // ... 整块删除
}
```

理由：Task 3 的 `remove_unmanaged_default_confirm_with_stdin_y` 已覆盖「unmanaged 目录保护 + registry 移除」，旧 uninstall 用例对应的命令已不存在。

文件顶部模块注释（第 1-6 行）把「uninstall 保护」改为「remove 保护」：

```rust
//! 覆盖 M3 手动验证的 CLI 场景：import-existing / upgrade 冲突交互 / remove 保护 / find / list。
```

- [ ] **Step 5: 加 managed remove e2e（真删目录，#[ignore] 需 npx install）**

`e2e_cli.rs` 在 remove 确认段后追加：

```rust
#[test]
#[ignore = "需真跑 npx skills 装 local source；cargo test -- --ignored 手动跑"]
fn remove_managed_deletes_canonical_directory() {
    // Given：装一个 local source managed skill
    let env = Env::new();
    install_local_skill(&env, "dc", "pdf");
    // When：--yes remove（跳过确认）
    let out = env.skillkit()
        .args(["remove", "dc/pdf", "--yes", "--json"])
        .assert()
        .success();
    // Then：--json removed_canonical=true；canonical 目录已删；registry 移除
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["removed_canonical"], true);
    assert!(!env.home_path().join(".skillkit/.agents/skills/pdf").exists());
    assert!(registry_ids(&env).is_empty());
}
```

运行：`cargo test -p skillkit-cli 2>&1`（常规，不含 ignore）
预期：全 PASS（managed 用例跳过）。

- [ ] **Step 6: format + lint + 全量 test**

```bash
make format && make check 2>&1
```
预期：fmt 应用、clippy 零 warning、全量测试通过（core + cli + server）。

- [ ] **Step 7: commit**

```bash
git add crates/cli/src/main.rs crates/cli/src/commands/install.rs crates/cli/tests/e2e_cli.rs
git commit -m "refactor(cli): remove 完全替换 uninstall——删 Uninstall 命令 + install 清理 + e2e 迁移"
```

---

### Task 5: 文档更新（README + 交接）

**Files:**
- Modify: `README.md`
- Modify: `docs/sessions/2026-07-29-skillkit-design.md`

- [ ] **Step 1: README 命令参考——uninstall → remove，补 find/list**

在 `README.md` 找到命令参考段（交接 §18 确认有「全部命令参考 source/install/project/profile/upgrade/import-existing/uninstall/serve」）：

把 `uninstall` 条目改为 `remove`，并在 `install` 段后补 `find` / `list` 两个条目。参考 CLI `--help` 真实输出（不编造）。示例条目格式（与现有条目同风格）：

```markdown
### skillkit find <query> [--json]
搜 skills.sh registry 中的 skill 候选，纯展示不安装。`--json` 输出候选数组 [{spec,url}]，供 agent 决策后自行 install。

### skillkit list [--json]
列出 registry 全部已装 skill。unmanaged（无源存量）行标 unmanaged。`--json` 输出 SkillMeta[]。

### skillkit remove <id> [--yes] [--json]
卸载 skill（完全替换 uninstall）。默认交互确认，`--yes` 跳过，`--json` 隐含跳过并输出 {id, removed_canonical}。unmanaged 只删登记不删目录。
```

原 `### skillkit uninstall <id>` 条目整段删除。

- [ ] **Step 2: 交接 §1.1 命令表面更新**

`docs/sessions/2026-07-29-skillkit-design.md` §1.1 命令表面代码块：

把：

```
skillkit uninstall <id>
```

改为：

```
skillkit find <query> [--json]                              # 搜 skills.sh 候选，纯展示不安装（--json 输出 [{spec,url}]）
skillkit list [--json]                                      # 列已装 skill（--json 输出 SkillMeta[]）
skillkit remove <id> [--yes] [--json]                       # 替换 uninstall；默认确认，--yes/--json 跳过（--json 输出 {id,removed_canonical}）
```

并在 §2 最近完成段顶部加一条（编号续 19）：

```markdown
19. **skill find/list/remove**（本会话）：顶层新增 find（搜 skills.sh，复用 npx::find）/ list（列 registry）/ remove（完全替换 uninstall + 补交互确认，修旧 uninstall 无确认的 gap）。新建 cli/commands/skill.rs，install.rs 删 Uninstall 回归单一职责、registry 源 --json 分支复用 skill::print_candidates（DRY）。GUI 原型 Skills 视图同步（find 搜索框/remove ×/unmanaged badge）。--json schema 锁定测试三件。
```

- [ ] **Step 3: format（md 不涉）+ 校验链接**

运行：`grep -n "uninstall" README.md docs/sessions/2026-07-29-skillkit-design.md 2>&1`
预期：除「替换 uninstall」「uninstall 无确认」这类说明性文字外，不再有把 `uninstall` 当作现存命令的引用（命令表面/help 引用应都已改 remove）。逐条确认残留合理。

- [ ] **Step 4: commit**

```bash
git add README.md docs/sessions/2026-07-29-skillkit-design.md
git commit -m "docs: find/list/remove 落地——README 命令参考 + 交接 §1.1 命令表面"
```

---

### Task 6: GUI 原型 Skills 视图同步

**Files:**
- Modify: `demo/index.html`

**Interfaces:**
- Consumes: server 真实 GUI `crates/server/templates/fragments/skills_main.html`（列 `id|scope|source|version|computed_hash|ops`；ops = install scope 下拉 + upgrade（仅 managed）+ ×）。
- Produces: 原型 Skills 视图按 server 形态补全 + 新增 find 搜索框；SKILLS mock 加 unmanaged 条目。

- [ ] **Step 1: SKILLS mock 加 unmanaged + 字段对齐**

`demo/index.html` 的 `SKILLS` 对象（约 331-340 行）追加一个 unmanaged 条目（`source:"unmanaged"`、`computed_hash:null`、`scope:"global"`）：

```javascript
  'unmanaged/legacy-thing': { name:'legacy-thing', source:'unmanaged', scope:'global', version:null, computed_hash:null, canonical_path:'~/.agents/skills/legacy-thing' },
```

- [ ] **Step 2: Skills 表格列对齐 server（加 ops 列，调列序）**

Skills 视图表头（约 307-309 行）改为：

```html
<thead><tr>
  <th>id</th><th>scope</th><th>source</th><th>version</th><th>computed_hash</th><th>ops</th>
</tr></thead>
```

- [ ] **Step 3: renderSkills 行——unmanaged badge / upgrade 仅 managed / install scope / remove ×**

`renderSkills`（约 426-443 行）重写渲染逻辑：

```javascript
function renderSkills(){
  renderSkillsFilter();
  const rows = Object.entries(SKILLS).filter(([id,s]) => {
    if (skillFilter==='all') return true;
    if (skillFilter==='global'||skillFilter==='local') return s.scope===skillFilter;
    return s.source===skillFilter;
  });
  $('#skills-body').innerHTML = rows.map(([id,s]) => {
    const managed = s.computed_hash !== null && s.computed_hash !== undefined;
    const badge = managed ? '' : ' <span class="tag" style="background:#fee2e2;color:#b91c1c">unmanaged</span>';
    const upgrade = managed
      ? `<button class="pill-btn u">upgrade</button>`
      : `<span style="color:var(--ink-3);font-size:11px">不可升级</span>`;
    const removeTitle = managed ? 'remove（删目录）' : 'remove（仅删登记）';
    return `
    <tr>
      <td>${idCell(id)}${badge}</td>
      <td>${sc(s)}</td>
      <td>${tag('src', s.source)}</td>
      <td class="mono">${s.version || '-'}</td>
      <td class="mono" style="color:var(--ink-2)">${s.computed_hash || '-'}</td>
      <td>
        <select class="select-mini" style="padding:3px 6px"><option>local</option><option>global</option></select>
        <button class="pill-btn">install</button>
        ${upgrade}
        <button class="pill-btn x" title="${removeTitle}" style="color:var(--danger)">×</button>
      </td>
    </tr>`;
  }).join('');
}
```

补一条 CSS（约 145-152 的 `.pill-btn` 段附近）让 `button.u` 有颜色（交接 §5 遗留项 3）：

```css
  .pill-btn.u { color: var(--ok); border-color: var(--ok); }
  .pill-btn.x { color: var(--danger); }
```

- [ ] **Step 4: Skills 视图顶部加 find 搜索框**

Skills 视图（约 300-312 行 section）在 `<div class="toolbar" id="skills-filter">` 上方加 find 表单：

```html
<form class="toolbar" onsubmit="event.preventDefault(); doFind();">
  <input class="filter" id="find-query" placeholder="find <query>：搜 skills.sh 候选" style="min-width:260px">
  <button type="submit" class="filter" style="background:var(--ink);color:var(--bg)">find</button>
  <span id="find-result" style="font-family:'IBM Plex Mono',monospace;font-size:11.5px;color:var(--ink-2)"></span>
</form>
```

在 `<script>` 内补 mock find（模拟 npx::find 返回候选，不联网）：

```javascript
const FIND_INDEX = ['pdf','frontend-design','dataviz','canvas-design','logseq-usage','tdd','code-review'];
function doFind(){
  const q = $('#find-query').value.trim();
  if (!q) { $('#find-result').textContent = ''; return; }
  // mock：query 命中 FIND_INDEX 即列候选 {spec,url}
  const hits = FIND_INDEX.filter(n => n.includes(q)).map(n => ({
    spec: `anthropics/skills@${n}`,
    url: `https://skills.sh/anthropics/skills/${n}`,
  }));
  $('#find-result').innerHTML = hits.length
    ? hits.map(h => `<div>[+] <b>${h.spec}</b> → ${h.url}</div>`).join('')
    : `<span style="color:var(--danger)">在 skills.sh 未找到 skill：${q}</span>`;
}
```

- [ ] **Step 5: 顶部 nav 旁命令提示（可选，反映新命令面）**

`demo/index.html` 页脚 `foot-note`（约 272 行样式 + 481 行内容）已是命令面说明位，无需改。Skills 视图本身已含 find/list（表格即 list）/remove（×），新命令面已体现。

- [ ] **Step 6: 浏览器手查**

运行：`open demo/index.html`（或 `make run ARGS="serve --port 7317"` 对照 server 真实 GUI）
预期（人眼走查）：
- Skills 表格列序 `id|scope|source|version|computed_hash|ops`，与 server 一致。
- unmanaged 行有红色 badge，upgrade 位显示「不可升级」，× 标「仅删登记」。
- managed 行有 upgrade 按钮（绿色）+ ×（红色）。
- find 框输入 `pdf` 回车 → 列出候选；输入 `zzz` → 「未找到」。
- install scope 下拉 + install 按钮每行可见。

- [ ] **Step 7: commit**

```bash
git add demo/index.html
git commit -m "demo: Skills 视图同步 find/list/remove——搜索框/×/unmanaged badge/列对齐 server"
```

---

## Verification（全计划跑完）

- [ ] `make check` 全绿（core + cli + server + clippy `-D warnings` 零 warning）。
- [ ] `cargo test -p skillkit-cli` 常规全过（find/list/remove 单测 + 非 ignore e2e）。
- [ ] `cargo test -p skillkit-cli -- --ignored`：`find_json_returns_candidate_array`（真跑 npx skills find）+ `remove_managed_deletes_canonical_directory`（真跑 npx install）过。
- [ ] CLI 手查：`make run ARGS="find pdf --json"` 输出候选数组；`make run ARGS="list --json"` 输出 SkillMeta[]；`make run ARGS="remove <id>"` 弹确认，`--yes` 跳过，`--json` 输出 `{id,removed_canonical}`。
- [ ] `skillkit uninstall` 在 `--help` 与分发中彻底消失：`make run ARGS="--help"` 输出无 uninstall，有 find/list/remove。
- [ ] `grep -rn "uninstall" crates/cli/src README.md` 仅剩说明性文字（如注释提及「替换 uninstall」），无现存命令引用。
- [ ] GUI 原型：`open demo/index.html` Skills 视图形态对齐 server 真实 GUI（§Task 6 Step 6 清单）。
