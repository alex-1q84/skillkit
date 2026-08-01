# skill find / list / remove — 设计 spec

> 日期：2026-08-01
> 状态：待 writing-plans 落实现计划
> 关联：`docs/sessions/2026-07-29-skillkit-design.md` §1.1（命令表面）、CLAUDE.md §6（CLI 约定）

## 1. 背景与动机

现有 CLI 的 skill 操作命令面有两个缺口：

- **find 埋在 install 里**：`npx::find`（搜 skills.sh registry，`crates/core/src/npx.rs:50`）已封装好且有解析/剥 ANSI/单测，但只被 `install add` 对 skills.sh 源（`package=None`）的分支调用（`crates/cli/src/commands/install.rs:44/65`），没有独立命令。agent 想「先搜后决策」必须走 install 的 `--json` 副作用路径，不直观。
- **没有 list**：registry 已存全部已装 skill 的 `SkillMeta`，但 CLI 没有列出它们的命令，只能起 server GUI 看。
- **uninstall 命名不对称**：`source` 用 `remove`，skill 这边却用 `uninstall`；且 `run_uninstall`（`install.rs:117`）直接删无确认，违反 CLAUDE.md §6「uninstall/remove 默认交互确认，--yes 跳过」。

本 spec 增加三个顶层命令 `find`/`list`/`remove`，把已有底层能力暴露成独立命令，并用 `remove` 完全替换 `uninstall`（含补交互确认）。底层 `core` 能力基本就绪，新代码集中在 cli 薄壳 + GUI 原型同步。

## 2. 命令表面

```
skillkit find <query> [--json]            # 搜 skills.sh registry，纯展示候选，不安装
skillkit list [--json]                    # 列 registry 全部已装 skill
skillkit remove <id> [--yes] [--json]     # 卸载（完全替换 uninstall），默认交互确认
```

### 2.1 find

- 复用 `skillkit_core::npx::find(paths, query) -> Vec<Candidate>`（`Candidate = {spec, url}`）。
- 纯展示候选，**不安装**——安装仍走 `install add`。agent 用法：`find --json` 拿候选 → 决策 → `install add <source> <skill>`。
- 人看：编号列表 `[0] anthropics/skills@pdf  →  https://skills.sh/...`；空结果报「在 skills.sh 未找到 skill：\<query\>」。
- `--json`：输出 `Candidate[]`，与现在 `install add skills.sh <skill> --json` 的候选输出同 schema（统一公开契约）。
- DRY：抽公共函数 `print_candidates(paths, query, json) -> Result<()>` 到 `commands/skill.rs`，`install.rs` 对 registry 源的 `--json` 分支（现 `print_registry_candidates`）改为复用它；固定源 `--json` 分支（输出 SkillMeta）不变。

### 2.2 list

- 复用 `Registry::load`，展示 `reg.skills.values()` 全部 `SkillMeta`。
- 人看表格：`id | source | scope | version | computed_hash`；`computed_hash=null` 的行尾标 `unmanaged`（对齐 server `skills_main.html` 的 badge）。
- `--json`：输出 `SkillMeta[]`（见 §4）。
- **不加** `--source`/`--scope` 过滤参数（YAGNI）。

### 2.3 remove（完全替换 uninstall）

- 复用 `skillkit_core::uninstall(paths, id)`（删 canonical 池子 + registry 登记 + `npx::remove` 同步 lock；`computed_hash=None` 的 unmanaged 不删目录）。
- **补交互确认**（修现有 uninstall 的 gap）：默认 `将删除 <id>，确认？(y/n)`，读 stdin；`--yes` 跳过；`--json` 隐含跳过并输出结果（agent/CI 友好）。
- `--json` 输出 `{id, removed_canonical}`：cli 层据 `meta.computed_hash.is_some()` 推断（managed→true，unmanaged→false），与 `uninstall` 内部行为一致；不改 `core::uninstall` 签名（避免波及 server handler）。

## 3. uninstall 处理（破坏性，已获主人确认）

**完全删除 `skillkit uninstall` 命令，不保留别名。** 理由：项目未发布（交接 §18「未发布前不进 Brewfile」），无外部用户；保留隐藏别名徒增维护面。

波及面一并改：
- `crates/cli/tests/e2e_cli.rs` 的 uninstall 用例（unmanaged 目录保护）改 `remove`。
- `README.md` 命令参考 `uninstall` → `remove`。
- `docs/sessions/2026-07-29-skillkit-design.md` §1.1 命令表面更新。

## 4. --json schema（公开契约，AI agent 依赖稳定）

```
find   → Candidate[]                                  // { "spec": String, "url": Option<String> }
list   → SkillMeta[]                                  // { id, name, source, scope, version,
                                                       //   computed_hash, installed_at, canonical_path }
remove → { "id": String, "removed_canonical": bool }
```

`Scope` 序列化沿用现有 `#[serde(rename_all = "lowercase")]`（`"global"`/`"local"`，交接 §8.2-13）。

三者的 `--json` 输出均加 schema 锁定测试（CLAUDE.md §8），防结构被无意改动。

## 5. 实现落点

### 5.1 cli

- 新建 `crates/cli/src/commands/skill.rs`：定义 `FindCmd`/`ListCmd`/`RemoveCmd` + `run_find`/`run_list`/`run_remove` + 公共 `print_candidates`。
- `crates/cli/src/commands/install.rs`：删 `UninstallCmd` + `run_uninstall` + `print_registry_candidates`（其逻辑移入 `skill.rs` 的 `print_candidates`，install 对 registry 源的 `--json` 分支改调它，固定源 `--json` 分支输出 SkillMeta 不变）；保留 `resolve_registry_package`（install 交互选候选仍用）。
- `crates/cli/src/commands/mod.rs`：加 `pub mod skill;` 与 `use`。
- `crates/cli/src/main.rs` `Cmd` 枚举：加 `Find(FindCmd)`/`List(ListCmd)`，`Uninstall(UninstallCmd)` → `Remove(RemoveCmd)`；`commands!` 分发对应改。

clap 结构（derive）：

```rust
Find { query: String, #[arg(long)] json: bool }
List { #[arg(long)] json: bool }
Remove { id: String, #[arg(long)] yes: bool, #[arg(long)] json: bool }
```

### 5.2 core

不动。`npx::find`、`Registry`、`uninstall` 均已就绪并在 `lib.rs` re-export（`Candidate`/`Registry`/`SkillMeta`/`uninstall` 都已导出，见 `lib.rs:23-29`）。

### 5.3 GUI 原型 demo/index.html

与此前「补全原型」差异分析合流，全部集中在 Skills 视图（find/list/remove 都映射到这里）：
- 加 **find 搜索框**：输入 query → 模拟展示 skills.sh 候选列表（`{spec,url}`，mock 几条）。
- 加 **remove × 按钮**：每行一个；unmanaged 行的 × 标注「仅删登记」（mock 状态）。
- Skills 视图列对齐 server 真实 GUI：`id|scope|source|version|computed_hash|ops`；补 **unmanaged badge**、**upgrade 仅 managed 行**、**install 切 scope 表单**（此前差异分析已指出）。
- mock 数据加 1-2 个 unmanaged skill（`source:"unmanaged"`、`computed_hash:null`），让 badge/条件渲染有数据。
- 其他视图（Sources/Profiles/Projects）本次不动。

## 6. 测试策略

测业务结果，不测内部调用（CLAUDE.md §8）。

- **find**：clap 解析（query 必填、`--json`）；`--json` schema 锁定（`Candidate[]`）；`#[ignore]` e2e 真跑 `npx skills find`，放 `crates/cli/tests/e2e_cli.rs`（与现有 import-existing/upgrade 的真跑 npx 用例同类，走 `make e2e-cli`）。
- **list**：registry 有数据 / 空 registry 两种输出；`--json` schema 锁定（`SkillMeta[]` 结构防漂移）。
- **remove**：clap 解析（id 必填、`--yes`/`--json`）；确认交互三条路径（默认问 y/n、`--yes` 跳过、`--json` 隐含跳过）；复用现有 uninstall 的 unmanaged 目录保护 e2e（`e2e_cli.rs` 用例改 `remove`）；`--json` 输出 `{id, removed_canonical}` 对 managed/unmanaged 各一。
- **迁移回归**：确认 `skillkit uninstall` 在 help/分发中彻底消失；`skillkit remove` 走通原 uninstall 全部 e2e。

验证命令：`make check`（含新单测 + clippy `-D warnings`）、`make e2e-cli`（remove 的 e2e）、`make run ARGS="find pdf --json"` / `list --json` / `remove <id>` 手动走查。

## 7. 不做（YAGNI 边界）

- find 不引导安装（纯展示）；安装仍走 `install add`。
- list 不加 `--source`/`--scope` 过滤参数。
- 不改 `core::uninstall` 签名（cli 层推断 `removed_canonical`）。
- 不保留 `uninstall` 别名（完全删除）。
- GUI 原型只改 Skills 视图；Sources/Profiles/Projects 不动。
- 不动 server 真实 GUI（本次作用域 = CLI + 原型；server 的 Skills 视图已有 install/uninstall/upgrade，后续若要对齐 remove 再单开）。
