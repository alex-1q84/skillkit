# 2026-07-29 → 2026-07-31 skillkit（设计 → M0 → M1 → M2 GUI → source 模型收敛）

> 用途：skillkit 会话关键事实/决策/遗留沉淀。新会话读 §1 + §4 + §7 三段够用；细节回查 §2/§3/§5/§6。
>
> **当前阶段**：source 模型收敛完成（统一走 npx skills，删 SourceType/git.rs，Source 极简 {name, package}，computedHash 版本）。下次接续 **M3 迁移打磨**。

## 1. 当前状态（2026-07-31，source 模型收敛完成）

### 1.1 命令表面

```
skillkit source add <package> [--name <别名>]        # 名称默认从 package 推导（repo 名/目录名），--name 覆盖
skillkit source list / remove <name>
skillkit install add <source> <skill> [--scope global|local]   # 固定源直接装；skills.sh 源走 npx skills find 交互选候选
skillkit uninstall <id>
skillkit serve [--port 7317] [--no-open]              # 四视图 + apply 闭环 + SSE + 默认自动打开浏览器
```

### 1.2 结构性事实

- **Source 极简 `{name, package: Option<String>}`**：package 是 npx skills source format（github shorthand `owner/repo` / 完整 git url / local path）；`None` = registry 搜索入口（skills.sh）。删 `SourceType` 枚举。
- **下载全委托 npx skills**：`crates/core/src/npx.rs` 封装 `add/find/update/remove` + skills-lock.json 读 computedHash。`git.rs` 整个删除。
- **canonical 池子 `~/.skillkit/.agents/skills/`**（原 `~/.skillkit/skills/`）：npx skills project scope 在 `cwd=~/.skillkit/` 直接写（`-a universal --copy -y`）。install 统一落池子，不分 scope。global 双层 symlink：池子 → `~/.agents/skills/`（Cursor 等直读）→ `~/.claude/skills/`（Claude 桥接）。
- **版本 `computed_hash`**：源自 `~/.skillkit/skills-lock.json` 的 `computedHash`（内容 SHA-256），取代 `commit_sha`。`locked_shas` 值同步。registry 字段改名 computed_hash。
- **skills.sh 默认源** = registry 搜索入口：CLI main / server serve 启动调 `SourcesStore::ensure_default`，缺失即自动补回（用户删了也会在下一次启动补回）。`install skills.sh/<skill>` 走 `npx skills find` 交互选候选（多同名候选不自动装）。
- `SkillkitError::Git` → `Tool`。`Cargo.lock` 有未提交改动（会话开始即存在）。
- 测试：45 全绿（core 31 + cli 3 + server 15）+ m0 两个端到端 `#[ignore]` 真跑 npx skills（手动跑）。clippy `pedantic -D warnings` 零 warning。

### 1.3 验证 flow

```bash
cd /Users/mywo/lab/skillkit
make check                                    # 全绿
cargo test -p skillkit-core -- --ignored      # m0 端到端真跑 npx skills（local fixture）
make run ARGS="serve --port 7317"             # 起 GUI 手动走查
```

## 2. 本会话累积的改动（newest first）

12. **Sources GUI 改进**（本会话）：① `ensure_default` 语义改为「skills.sh 缺失即补回」（覆盖「删了不加回」，修空文件 `sources = []` 时 GUI 空白 bug，决策 14）；② 新增 `derive_source_name`（shorthand/scp-style/url/local path 四形态取末段 + 剥 .git/尾斜杠），CLI `source add <package> [--name]`（**有意的破坏性参数序变更**）、GUI 表单 package 输入实时预览推导名（htmx 调 `/sources/preview` 服务端推导，前端零规则副本；name 手动编辑后停用预览）。决策 15。测试 51 全绿（core 33 + cli + server 18）。

11. **source 模型收敛——统一走 npx skills**（前会话，`e918bc0`）：主人 5 轮反馈驱动。删 `SourceType`/`git.rs`；`Source`→`{name, package}`；下载全委托 npx skills（私有 git / local / github 统一 source format）；canonical 池子改 `~/.skillkit/.agents/skills/`；版本 `commit_sha`→`computed_hash`（读 skills-lock.json）；skills.sh 默认源=registry 搜索入口（find 交互选）；`SkillkitError::Git`→`Tool`。CLI `source add <name> <package>`（去类型参数）、`install add <source> <skill> [--scope]`（默认 local）。server SourceForm 单 package 输入框。文档同步（spec §4-§8.5 + §10/§11/§13 ripple、决策纪要追加**决策 13**、CLAUDE.md §5）。决策推理见 `docs/design-decisions-2026-07-29.md` 决策 13。

10. **serve 自动打开浏览器**（前会话，`450a7fd`）：serve 默认启动后用 `open`(macOS)/`xdg-open`(Linux)/`start`(Windows) 打开默认浏览器（标准库 `Command` 手写，无新依赖）；加 `--no-open` flag 供脚本/CI/测试跳过。listener 绑好后才 open，浏览器请求立即连上；open 失败只 warn 不挡 serve。

9. **M2 T10-T15**（前会话）：Projects 视图 + Sources CRUD + Skills install/uninstall + 声明编辑/apply 闭环 + SSE + app.css 产品化。执行中踩的坑见 §3.1。

8. **M2 T1-T9**（前会话）：server 骨架 → token → 静态 → layout → 文件锁 → Sources/Skills/Profiles 视图。

7. **`.skm` → `.skillkit` 改名 + M2 spec/plan**（前会话）。

6-0. M1/M0/工程化/设计（前会话，见 `git log`）。

## 3. 关键背景知识

### 3.4 npx skills 行为（本会话实测，source 重构 / M3 upgrade 必读）

- 下载委托命令：`npx skills add <package> -s <skill> -a universal --copy -y`，在 `cwd=~/.skillkit/` 跑（project scope）→ skill 落 `.agents/skills/<skill>/`，`skills-lock.json` 落 cwd 根。用 `-s` 分开传 package 和 skill（`@skill` 合一格式对 local path 不通用）。
- **package 支持三种**：github shorthand（`owner/repo`）、完整 git url（`git@`/`https://`/`ssh://`，私有仓库走标准 git 认证——SSH key / git credential）、local path（相对或绝对）。实测 local + github 走通；私有 git 认证失败只是环境无 SSH key，npx 行为正确。
- **npx skills 没有指定输出目录的参数**——落点由 `-a <agent>` + scope 决定，agent 是固定枚举。`universal` agent 的 project 路径是 `.agents/skills/`。这就是 canonical 池子选 `~/.skillkit/.agents/skills/` 的原因（零搬运，npx 直接写）。
- `find` 输出：`<owner/repo@skill> + install 数 + skills.sh URL`，**多候选同名**（如 pdf 在多个 package），CLI 交互选不自动装。输出带 ANSI 色码（`npx.rs::strip_ansi` 手动剥，NO_COLOR 不一定生效）。`parse_find` 排除含 `<`/`>` 的占位 token。
- `skills-lock.json` 结构：`{version, skills: {<name>: {source, sourceType, skillPath, computedHash}}}`。github 有 `skillPath`，local 没有（source 直接是路径）。computedHash 是版本锁依据。
- npx skills 自带安全扫描（Socket/Snyk）和 source 解析，skillkit 白嫖。
- 其他命令：`npx skills find/update/remove/use`；`experimental_install`（从 skills-lock.json 恢复）是 M3 可用的重装原语。

### 3.1 M2 执行经验（前会话，扩展 GUI 或 htmx+askama 工作必读）

plan 的代码有几处写法执行时会卡，修正模式（T1-T15 验证过）：

1. **handler 渲染模式**：不写 `match Tpl{...}.render() {...}`（结构体字面量在 match 头致花括号歧义，rustfmt 报错）。改：`let rendered = Tpl{...}.render();` 再 match，或拆同步 `fn render_xxx(state, token) -> Response` + `render_str` helper。
2. **写操作调视图**：page handler 第一个参数是 `State<AppState>` extractor，不能从写操作传裸 state 调。拆同步 `fn render_xxx(state: AppState, token)`，page 和写操作都调它。
3. **重复 key 表单**：`Form<ReorderForm{order: Vec<String>}>` 不行（serde_urlencoded 不支持重复 key→Vec）。改 `body: Bytes` + `form_urlencoded::parse(&body).filter(...)`。
4. **askama include**：不支持 `{% include "x" with var %}`（共享外层模板上下文）。改外层 for 变量名对齐被 include 模板字段名。
5. **私有 async fn**：clippy `unused_async` 对私有 async fn 无 await 报错。`render_xxx` 改同步 fn（pub handler 保持 async）。
6. **re-export 不混用**：core 公开类型在 `lib.rs` 统一 re-export，handler 全用 `skillkit_core::X`。新增公开类型补进 lib.rs（本会话顺带修了 `Source` 漏 re-export 的混用反例）。
7. **clippy pedantic 坑**：`default_trait_access`（`BTreeMap::new()` 取代 `Default::default()`）、`ignored_unit_patterns`（`Ok(())` 取代 `Ok(_)`）、`case_sensitive_file_extension_comparisons`、`single_match_else`（两臂 match 改 `if matches!`）、`empty_line_after_doc_comments`（doc 注释后不空行）。
8. **Axum 0.8 路径参数**：`{token}`（非 0.7 的 `:token`）；State/Path 在前，body extractor 最后；State 要 Clone。`{id}` 单段接受 %2F 解码，前端按钮 URL 里 id 含 / 须 handler 预编码（`m.id.replace('/', "%2F")`）。
9. **尾斜杠 404**：axum 0.8 `/{token}` 严格匹配，`/TOKEN/` 404。额外注册 `/{token}/`。
10. **askama 不支持方法借用参数**：`contains(&meta.id)` 编译失败。handler 预计算 `Vec<(SkillMeta, bool)>`。
11. **htmx 片段替换保 id**：写操作返回片段 outerHTML 替换时，片段外层用固定 id（如 `id="status-panel"`），否则替换后 id 丢失。
12. **SSE 前端**：htmx 2.x 的 sse 扩展拆到独立包，改用浏览器原生 `EventSource` + `htmx.ajax`，零额外依赖。
13. **Scope serde**：`#[serde(rename_all = "lowercase")]`，json 里 `"global"`/`"local"`。

### 3.2 Axum 0.8 要点

见 3.1-8。

### 3.3 既有 M0/M1 背景（不变量）

- apply 闭环：`compute_diff` → `land_one` → `run_apply` → `build_status`；agent_dir 映射 `claude-code`→`.claude`。install 默认 local scope（global 需显式 `--scope global`，此时池子→agents→claude 双层 symlink 立即可用）。
- project-id = uuid v4 前 8 hex 大写；`locked_shas` 是上次 apply 的 computed_hash 基线快照（非版本锁）。
- **两目录职责分离**：池子 `~/.skillkit/.agents/skills/`（install 落点、npx 写）vs 落地点 `~/.agents/skills/`（apply 后 agent 直读）。不违反 CLAUDE.md §5（home 根 `~/.agents/skills/` 仍只放 apply 落地的全局公共 skill）。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（core 31 + cli + server 15 + clippy `-D warnings` 零 warning）。
- [ ] `cargo test -p skillkit-core -- --ignored`：m0 两个端到端过（真跑 npx skills local fixture → 池子落地 + registry + 双层 symlink；重复 install 报错）。
- [ ] `make run ARGS="serve --port 7317"` 走查：Sources 显示 skills.sh 默认源（不再空白）、package 输入实时预览推导名（git url → repo 名）、name 框可覆盖且手动编辑后不再被覆盖、Skills install skills.sh 源走 find 交互选候选、apply 闭环到 `~/.agents/skills/`。
- [ ] `git status` 干净度：npx.rs 新增、git.rs 删除。
- [ ] **回归信号**：install 后 canonical 落 `~/.skillkit/.agents/skills/`（不是 `~/.agents/skills/`）；registry.json 字段是 `computed_hash` 不是 `commit_sha`；`crates/core/src/git.rs` 不存在；无 `~/.skillkit/.lock/*.lock` 残留。

## 5. 已知遗留 / 待办

1. **M3 迁移打磨**（下次接续，spec §15）：
   - `skillkit import-existing`：扫描 `~/.codex/skills/`、`~/.cursor/skills/` 等存量 → 识别 + 登记（package 语义：source add 或直接安装）。
   - `skillkit upgrade <id>`：`npx skills update <skill>` + 重读 skills-lock.json 的 computed_hash 更新 registry；扫描 `locked_shas` 冲突列受影响项目需 `--yes`（spec §10 line 326）。**语义变了：不再 git pull**。
   - 打包进 mac-config Brewfile（与 cx/rtk 一致）。
2. **基建债**：CI（GitHub Actions `make check`）、README、Cargo.toml `[package]` 元数据（description/license/repository）。
3. **source 重构收尾**：CLI registry 源 install 的 `--json` 候选输出未做（现仅交互选）；demo SKILLS mock 的 sha/path 字段未全同步（原型低优先）；sources.toml 旧 schema 检测用 `contains("source_type")` 较糙（项目未发布，可接受）。
4. **SSE 线程保活优化**（可选）：notify watcher 在 SSE 连接断开后不退出，多次刷新累积线程。本地短时 serve 可接受，长跑需绑定 stream 生命周期。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/
├── CLAUDE.md / Cargo.toml / Makefile / rustfmt.toml
├── crates/
│   ├── core/                  # skillkit-core（lib）—— 业务逻辑
│   │   ├── src/{lib,paths,error,config,source,registry,npx,install,symlink,profile,project,apply,lock}.rs   # git.rs 已删，npx.rs 新增
│   │   └── tests/{m0_e2e,m1_e2e}.rs            # m0 端到端 #[ignore] 真跑 npx skills
│   ├── cli/                   # skillkit-cli（bin）
│   │   └── src/{main, commands/{source,install,profile,project,serve}}.rs
│   └── server/                # skillkit-server（lib）—— Axum + Askama + rust-embed
│       ├── src/{lib.rs, routes/{mod,sources,skills,profiles,projects,sse}.rs}
│       ├── templates/{layout,home,sources,skills,profiles,projects,project_workspace}.html + fragments/
│       ├── static/{htmx.min.js, sortable.min.js, app.css}
│       └── tests/{common/mod.rs, routes.rs}
├── demo/index.html            # GUI 设计原型（user-visible，SOURCES mock 已改 package 语义）
└── docs/
    ├── 2026-07-29-skillkit-design.md          # spec（source 模型收敛后的权威）
    ├── design-decisions-2026-07-29.md         # 决策 13 = source 收敛推理
    ├── superpowers/...                        # M2 spec/plan
    └── sessions/2026-07-29-skillkit-design.md # 本交接
```

运行时目录（user-visible 状态）：
- `~/.skillkit/`：元数据（config/sources/registry/skills-lock.json/profiles/projects/.lock）。
- `~/.skillkit/.agents/skills/<skill>/`：canonical 池子（npx skills 直接写，内部）。
- `~/.agents/skills/<skill>/`：global apply 落地点（agent 直读；symlink 自池子）。
- `~/.claude/skills/<skill>/`：Claude 桥接（symlink → ~/.agents/skills/）。

## 7. 下次接续工作的最短路径（M3 迁移打磨）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
make check                                     # 全绿
cargo test -p skillkit-core -- --ignored       # m0 端到端真跑 npx skills
make run ARGS="serve --port 7317"              # 起 GUI 走查四视图 + Sources 单输入框
```

**必读**：§3.4（npx skills 行为）+ `docs/design-decisions-2026-07-29.md` 决策 13。若涉及 GUI 扩展或新 htmx 端点，按 §3.1 的 13 条坑实现。

### 7.2 焦点：M3 三件事

1. **import-existing**：扫描 `~/.codex/skills/`、`~/.cursor/skills/` 等存量 skill 目录，识别 + 登记进 registry（package 语义：source add 或直接安装）。
2. **upgrade**：`skillkit upgrade <id>` 走 `npx skills update` + 重读 skills-lock.json 更新 registry.computed_hash；扫描 project `locked_shas` 列出受影响项目并警告，需 `--yes`（spec §10 line 326）。
3. **Brewfile 打包**：纳入 mac-config Brewfile（build + install）。

### 7.3 优先级

1. M3 三件（import-existing → upgrade → Brewfile）→ 2. 基建债（CI/README/元数据）→ 3. source 收尾（--json 候选）→ 4. SSE 线程优化（可选）。

## 7.x (archive) 历史接续路径

- **M2 完成 → M3**（前会话，source 收敛前）：M2 T1-T15 全完成，四视图 + apply 闭环 + SSE + 视觉。次接 M3 迁移打磨（import-existing / upgrade / Brewfile）。
- **M2 T10-T15 阶段（已完成）**：inline 执行 plan Task 10-15，Projects 视图 → 写操作 → apply 闭环 → SSE → 视觉。
- **M2 T1-T9 阶段（已完成）**：inline 执行 plan Task 1-9，server 骨架 → Profiles 写操作（含 SortableJS）。
- **M2 设计阶段（已完成）**：brainstorming htmx（取代 React）→ spec → writing-plans 15 task。
- **M1 阶段（已完成）**：profile/project/apply 幂等落地闭环（10 TDD task）。
- **M0 阶段（已完成）**：core 骨架 + install + 全局 Claude symlink。
- **设计阶段（已完成）**：CLAUDE.md → spec review（P0/P1/P2）→ writing-plans M0。
