# skillkit 交接（2026-07-29 → 2026-08-01，M0-M3 + skill find/list/remove + GUI parity 设计完成）

> 用途：新会话读 §1（当前状态）+ §3（必读背景）+ §5（当前待办）三段够用；验证/路径/命令回查 §4/§6/§7；历史改动与前端坑归档在 §8，回查用。
>
> **当前阶段**：GUI parity 设计完成（spec `b3a5eeb` + plan `0e0c066`，已提交 main），**代码尚未动**。目标：把 CLI 已有、GUI 缺失的 8 条操作补到 web GUI（Skills 视图 find/install-candidate/import/upgrade-all + Projects 视图 add/scan/rebind/apply-profile），core 仅需下沉 `scan_projects`。plan 拆 10 个 TDD task，每 task 自带测试循环 + commit。下次接续 **直接执行 plan**（§7）。基建债（CI / Cargo.toml 元数据）退居次优。

## 1. 当前状态

### 1.1 命令表面

```
skillkit source add <package> [--name <别名>]        # 名称默认从 package 推导（repo 名/目录名），--name 覆盖
skillkit source list / remove <name>
skillkit install add <source> <skill> [--scope global|local] [--json]   # 固定源直接装；skills.sh 源走 npx skills find 交互选候选（--json 输出候选数组不安装）
skillkit find <query> [--json]                              # 搜 skills.sh 候选，纯展示不安装（--json 输出 [{spec,url}]）
skillkit list [--json]                                      # 列已装 skill（--json 输出 SkillMeta[]）
skillkit remove <id> [--yes] [--json]                       # 完全替换 uninstall；默认确认，--yes/--json 跳过（--json 输出 {id,removed_canonical}）
skillkit upgrade <id> | --all [--yes] [--json]                          # npx skills update + 重读 computed_hash；冲突列受影响项目，--yes 跳过
skillkit import-existing [--json] [--dry-run]                           # 扫描存量 skill 目录，可溯重装入池 + 无源 unmanaged 登记
skillkit serve [--port 7317] [--no-open] [--token <固定值>]   # 四视图 + apply 闭环 + SSE + 默认自动打开浏览器；--token 仅 e2e/localhost 用（默认随机）
```

### 1.2 结构性事实

- **Source 极简 `{name, package: Option<String>}`**：package 是 npx skills source format（github shorthand `owner/repo` / 完整 git url / local path）；`None` = registry 搜索入口（skills.sh）。
- **下载全委托 npx skills**：`crates/core/src/npx.rs` 封装 `add/find/update/remove` + skills-lock.json 读 computedHash。
- **canonical 池子 `~/.skillkit/.agents/skills/`**：npx skills project scope 在 `cwd=~/.skillkit/` 直接写（`-a universal --copy -y`）。install 统一落池子，不分 scope。global 双层 symlink：池子 → `~/.agents/skills/`（Cursor 等直读）→ `~/.claude/skills/`（Claude 桥接）。
- **版本 `computed_hash`**：源自 `~/.skillkit/skills-lock.json` 的 `computedHash`（内容 SHA-256）。registry 字段名 computed_hash，`locked_shas` 值同步。
- **skills.sh 默认源** = registry 搜索入口：CLI main / server serve 启动调 `SourcesStore::ensure_default`，缺失即自动补回（用户删了也会在下一次启动补回）。`install skills.sh/<skill>` 走 `npx skills find` 交互选候选（多同名候选不自动装）；`--json` 时直接输出候选数组。
- `SkillkitError::Git` → `Tool`。
- 测试：**make check 全绿**（core 45 + cli 8 + server 21 + m0_e2e 1 + m1_e2e 3 + m3_e2e 1）+ **CLI e2e 10 用例**（5 常规 + 5 `#[ignore]` 真跑 npx）+ **GUI e2e 6 用例**。clippy `-D warnings` 零 warning。计数会漂，用 `make check` / `make e2e-cli` / `make e2e` 复跑。
- **unmanaged skill**（M3）：存量目录无法溯源时以虚拟源 `unmanaged` 登记（`computed_hash=None`、scope=global），不可升级、remove 不删目录、GUI 角标标记。`import-existing` 扫描 `~/.agents/skills/` + `~/.claude/skills/`（跳 symlink）+ `~/.codex/skills/` + `~/.cursor/skills/`，可溯源（`.git`+remote）重装入池。
- **e2e 设施三层**（`make check` / `make e2e-cli` / `make e2e`）：core `#[ignore]` 端到端真跑 npx；CLI assert_cmd 驱动真实二进制 + 临时 HOME（`crates/cli/tests/e2e_cli.rs`，BDD 风格 Given/When/Then）；GUI playwright 真实 chromium。
- **GUI 现状（2026-08-01 盘点）**：CLI 18 条原子操作中 GUI 已覆盖 10 条，缺 8 条（find/install-candidate/import/upgrade-all/project add/scan/rebind/apply-profile）。8 条里 7 条 core 已具备能力（GUI 待接端点），仅 `scan_projects` 当前在 `cli/commands/project.rs` 待下沉 core。spec/plan 已就绪（§2-20），**代码未动**。

### 1.3 验证 flow

```bash
cd /Users/mywo/lab/skillkit
make check                                    # 全绿（cli bin 15 + e2e 8 非 ignore + core 45 + server 21，clippy 零 warning）
make e2e                                      # GUI 端到端（真实 chromium，6 用例；需空闲端口 7417）
make e2e-cli                                  # CLI 全链路端到端（assert_cmd + 真跑 npx，5 用例；不进 check）
cargo test -p skillkit-core -- --ignored      # core 端到端真跑 npx skills（m0 2 + m3 1）
make run ARGS="serve --port 7317"             # 起 GUI 手动走查
```

## 2. 最近完成（GUI parity 设计 + skill find/list/remove + M3 全量 + e2e 固化 + README）

20. **GUI parity 设计**（本会话，spec `b3a5eeb` + plan `0e0c066`，已提交 main，**代码未动**）：brainstorming→writing-plans 全流程。对照 CLI 18 条原子操作盘点 GUI 缺口——已覆盖 10 条，缺 8 条（find/install-candidate/import/upgrade-all + project add/scan/rebind/apply-profile）。core 仅 `scan_projects` 需从 `cli/commands/project.rs` 下沉（其余 7 条 core 已具备）。决策：find 同步 + `hx-indicator` loading（不搞 async+SSE，YAGNI）；GUI 端点不加 `--json`（职责分离，`--json` 是 CLI 给 agent 的契约）；8 条全补齐含 scan（主人定）。plan 拆 10 TDD task（Task1 scan 下沉 / Task2-5 Skills 四端点 + fake_npx 测试基建 / Task6-9 Projects 四端点 / Task10 收尾），每 task 完整 handler+模板+测试代码。spec/plan 落 `docs/superpowers/{specs,plans}/2026-08-01-gui-parity*.md`。下次直接执行 plan（§7）。

19. **skill find/list/remove**（已 merge main `5885ee9`，plan 回填 `b73260d`）：brainstorming→writing-plans→executing-plans→finishing 全流程。顶层新增 find（搜 skills.sh，复用 `npx::find`）/ list（列 registry）/ remove（完全替换 uninstall + 补交互确认，修旧 uninstall 无确认的 gap）。新建 `cli/commands/skill.rs`，install.rs 删 `UninstallCmd`/`run_uninstall`/`print_registry_candidates` 回归单一职责、registry 源 --json 分支复用 `skill::print_candidates`（DRY）。--json schema 锁定测试三件（Candidate[]/SkillMeta[]/{id,removed_canonical}）。GUI 原型 `demo/index.html` Skills 视图同步（find 搜索框/remove ×/unmanaged badge/列对齐 server）。spec/plan 落 `docs/superpowers/{specs,plans}/`。**执行修正回填 plan 6 类**：Candidate 移 tests use、refutable pattern 改 let-else、scope_str 传值、render_list_table 用 writeln!、note 用 &str、验证命令 --lib→--bin skillkit。

18. **README 落地**（本会话，未提交）：新建 `README.md`——从交接 §1.1 命令表面 + CLI `--help` 真实输出提炼（不编造命令），覆盖安装（`cargo install --path crates/cli`）、快速开始、全部命令参考（source/install/project/profile/upgrade/import-existing/uninstall/serve）、支持的 agent 表格（Claude/Cursor/OpenCode/Codex，新增 agent 只改配置）、开发命令（make check/e2e/e2e-cli）、三层架构。README 顶部声明 MIT（license 字段待 Cargo.toml 补齐对齐）。基建债 README 项完成，剩 Cargo.toml 元数据 + CI。

17. **手动验证固化 e2e**（本会话，`04626fc`）：把 M3 手工验证的 case 固化为自动化。① `crates/cli/tests/e2e_cli.rs`——assert_cmd 驱动真实 skillkit 二进制 + 临时 HOME 隔离，BDD 风格（Given/When/Then 注释分段），10 用例：import-existing（登记/去重/dry-run/幂等/--json，5 常规）+ uninstall 保护 unmanaged 目录 + upgrade（冲突 y/n 交互/--yes/--json 走 stderr/--all 列出/UpgradeAllReport，5 `#[ignore]` 真跑 npx）。cli 加 `assert_cmd` + `tempfile` dev-dep。② `e2e/test_ui.py` 补 2 用例（unmanaged 角标、upgrade 按钮仅 managed + install 表单保留回归），TESTS 表加每用例目标页。③ Makefile 加 `e2e-cli` target；CLAUDE.md/交接文档补运行方式。**踩坑**：CLI 输出 hash 带中文括号「（hash: ...）」提取易错；upgrade 冲突的受影响项目在 stdout 非 stderr；unmanaged 行 td 里 badge 文本混入 id 提取（需 split()[0]）。

16. **M3 迁移打磨全量**（本会话，`93d9067`..`4ccba5c`，SDD 9 task + final review）：① `import-existing`——扫描 `~/.agents/skills/` + `~/.claude/skills/`（跳 symlink）+ `~/.codex/skills/` + `~/.cursor/skills/`，可溯源（`.git`+remote）重装入池、无源登记 unmanaged（虚拟源 `unmanaged`、`computed_hash=None` 不可升级、uninstall 不删目录）；`--dry-run`/`--json`。② `upgrade <id> | --all`——复用 `npx::update` + 重读 computed_hash；`locked_shas[id]==old_hash` 判冲突（单 skill 交互确认，`--all` 冲突列出不拦截——主人决策「列出不拦截」，`UpgradeAllReport { upgraded, blocked }`）。③ GUI Skills 视图 unmanaged 角标 + upgrade 按钮（managed 行）+ install 表单保留。④ mac-config justfile `install_skillkit`（未发布，不进 Brewfile）。**执行经验**：SDD 的 dry-run 去重发散（Task 3 fix）、计划模板误删 install 表单（Task 7 fix）、final review 修 RemoveFailed/--json 流/PATH 守卫/CSS。local source 的 upgrade 是 npx no-op 已知限制（§3.1）。

15. **SSE watcher 全局单例**（P4）：sse.rs 改每目录一个常驻 watcher 线程 + broadcast channel（`OnceLock<Mutex<HashMap<PathBuf, broadcast::Sender>>>`），多连接订阅同一 channel，连接断开只 drop receiver 不重建 watcher——修旧实现「每次连接 spawn 永不退出的 watcher 线程，多次刷新累积」。`BroadcastStream` 落后丢事件由 SSE 下次事件刷新兜底。Cargo.toml 补 `tokio sync`、`tokio-stream sync`。

14. **source 收尾三件**（P3）：① `install add` 加 `--json`——registry 源输出 find 候选数组 `[{spec,url}]`（不交互不安装，agent 决策用）、固定源输出 SkillMeta JSON；补 3 个 clap 解析测试（cli 测试 0→3）。② demo/index.html SKILLS mock 字段同步 `sha`/`path` → `computed_hash`/`canonical_path`（值改 `~/.skillkit/.agents/skills/<skill>`，清 `~/.skm` 残留），渲染/fallback/冲突文案「sha 漂移」→「hash 漂移」。③ `SourcesStore::load` 旧 schema 检测从 `contains("source_type")` 子串匹配改为解析驱动：`Source` 加 `deny_unknown_fields`，解析失败即备份 `.bak` + 重置默认（覆盖任意坏 TOML）。

> 更早的改动（M0/M1/M2、e2e 设施、source 模型收敛等）已归档到 §8.1。

## 3. 必读背景

### 3.1 npx skills 行为（source 重构 / M3 upgrade 必读）

- 下载委托命令：`npx skills add <package> -s <skill> -a universal --copy -y`，在 `cwd=~/.skillkit/` 跑（project scope）→ skill 落 `.agents/skills/<skill>/`，`skills-lock.json` 落 cwd 根。用 `-s` 分开传 package 和 skill（`@skill` 合一格式对 local path 不通用）。
- **package 支持三种**：github shorthand（`owner/repo`）、完整 git url（`git@`/`https://`/`ssh://`，私有仓库走标准 git 认证——SSH key / git credential）、local path（相对或绝对）。实测 local + github 走通；私有 git 认证失败只是环境无 SSH key，npx 行为正确。
- **npx skills 没有指定输出目录的参数**——落点由 `-a <agent>` + scope 决定，agent 是固定枚举。`universal` agent 的 project 路径是 `.agents/skills/`。这就是 canonical 池子选 `~/.skillkit/.agents/skills/` 的原因（零搬运，npx 直接写）。
- `find` 输出：`<owner/repo@skill> + install 数 + skills.sh URL`，**多候选同名**（如 pdf 在多个 package），CLI 交互选不自动装。输出带 ANSI 色码（`npx.rs::strip_ansi` 手动剥，NO_COLOR 不一定生效）。`parse_find` 排除含 `<`/`>` 的占位 token。
- `skills-lock.json` 结构：`{version, skills: {<name>: {source, sourceType, skillPath, computedHash}}}`。github 有 `skillPath`，local 没有（source 直接是路径）。computedHash 是版本锁依据。
- npx skills 自带安全扫描（Socket/Snyk）和 source 解析，skillkit 白嫖。
- 其他命令：`npx skills find/update/remove/use`；`experimental_install`（从 skills-lock.json 恢复）是 M3 可用的重装原语。
- **⚠️ `npx skills update` 对 local source skill 静默 no-op（M3 实测）**：local path 源的 skill 无 `skillPath`（lock 里 source 直接是路径），`update` 的 `updatable` 过滤只认有 `skillPath` 的远程源，local 的被归入 legacy 打印「No installed skills found matching」后跳过。结果：`skillkit upgrade` 对 local 源 skill 能跑通流程但 hash 不变（`806cba88 → 806cba88`）。github source 正常。已知限制，不是 skillkit bug。

### 3.2 既有 M0/M1 背景（不变量）

- apply 闭环：`compute_diff` → `land_one` → `run_apply` → `build_status`；agent_dir 映射 `claude-code`→`.claude`。install 默认 local scope（global 需显式 `--scope global`，此时池子→agents→claude 双层 symlink 立即可用）。
- project-id = uuid v4 前 8 hex 大写；`locked_shas` 是上次 apply 的 computed_hash 基线快照（非版本锁）。
- **两目录职责分离**：池子 `~/.skillkit/.agents/skills/`（install 落点、npx 写）vs 落地点 `~/.agents/skills/`（apply 后 agent 直读）。不违反 CLAUDE.md §5（home 根 `~/.agents/skills/` 仍只放 apply 落地的全局公共 skill）。

### 3.3 GUI parity 设计要点（执行 plan 前必读）

- **8 缺口的 core 能力盘点**：`npx::find`（搜候选）/ `install(paths,"skills.sh",skill,spec,scope)`（registry 源装，package=find 候选的 spec）/ `import_existing` / `upgrade_all` / `Project::register` / `proj.rebind` / `proj.apply_profile`+`Profile::load`——core 全有，GUI 薄壳调。唯 `scan_projects` 需下沉（plan Task 1）。
- **registry 源 install 的 spec↔skill 语义**：candidate.spec（`owner/repo@skill`）是 `npx skills add` 的 package 参数；skill 名用 find 时的 query（如 `pdf`），决定 canonical 目录名 + registry id 后缀（`skills.sh/pdf`）。与 CLI `resolve_registry_package` 一致。
- **fake_npx 测试模式**（plan Task 2 引入 `crates/server/tests/common/mod.rs`）：PATH 前置假 npx 脚本响应 find/add/update，RAII guard drop 还原 PATH。假 npx 无状态统一响应，多测试并发覆盖 PATH 无害（行为一致 + 各自 cwd 独立）。范式源自 core `upgrade.rs` 的 `install_fake_npx`。

## 4. 验证清单（重载 / 切换后立即跑）

- [ ] `cd /Users/mywo/lab/skillkit && make check` 全绿（core 45 + cli bin 15 + e2e 8 非 ignore + server 21 + clippy `-D warnings` 零 warning）。
- [ ] `cargo test -p skillkit-core -- --ignored`：m0 两个端到端过（真跑 npx skills local fixture → 池子落地 + registry + 双层 symlink；重复 install 报错）+ m3 一个（install → upgrade 更新 hash）。
- [ ] `make e2e-cli`：CLI 全链路 e2e 过（import-existing / remove 确认交互 + upgrade 冲突交互 + managed remove 真跑 npx）。
- [ ] `make e2e`：GUI e2e 6 用例过（导航不重复回归 / 实时预览 / 默认源 / 增删闭环 / unmanaged 角标 / upgrade 按钮仅 managed）。
- [ ] `make run ARGS="serve --port 7317"` 走查：Sources 显示 skills.sh 默认源（不再空白）、package 输入实时预览推导名（git url → repo 名）、name 框可覆盖且手动编辑后不再被覆盖、Skills install skills.sh 源走 find 交互选候选、apply 闭环到 `~/.agents/skills/`。
- [ ] `install add` 的 `--json` 行为：固定源输出 SkillMeta JSON；skills.sh 源输出候选数组（不交互不安装）。
- [ ] `git status` 干净度：工作树应干净（GUI parity spec+plan + skill find/list/remove + README + plan 修正均已提交 main）。
- [ ] **GUI parity 就绪信号**：`ls docs/superpowers/{specs,plans}/2026-08-01-gui-parity*.md` 两文件都在；`git log --oneline -3` 见 `0e0c066`(plan) + `b3a5eeb`(spec)。代码尚未动——`make check` 仍为旧基线全绿（core 45 + cli + server 21）。
- [ ] **回归信号**：若 `git log` 不见 `0e0c066`/`b3a5eeb`，spec/plan 丢了——从 `docs/superpowers/{specs,plans}/` 重查；若 `make check` 已有 server 新端点测试（find/scan/install-candidate 等）说明 GUI parity 已开始执行，改读 plan 对应 task 续上。
- [ ] **回归信号**：install 后 canonical 落 `~/.skillkit/.agents/skills/`（不是 `~/.agents/skills/`）；registry.json 字段是 `computed_hash` 不是 `commit_sha`；`crates/core/src/git.rs` 不存在；无 `~/.skillkit/.lock/*.lock` 残留。GUI Skills 页若 unmanaged 行没有「install 表单」= M3 计划误删 install 的回归（Task 7 fix 曾修复）。

## 5. 已知遗留 / 待办

1. ~~**M3 迁移打磨**~~ ✅ 全部完成（commit `fbbedb8` + `04626fc`）：
   - ~~`skillkit import-existing`~~ ✅ 完成——扫描存量 skill 目录 → 可溯重装入池 + 无源登记 unmanaged；`--dry-run` 只输出不写，`--json` 输出 ImportReport。
   - ~~`skillkit upgrade <id>`~~ ✅ 完成——`npx skills update` + 重读 computed_hash 更新 registry；`--all` 批量（冲突列出不拦截，blocked 列受影响项目）；单 skill 冲突需 `--yes` 或交互确认。
   - ~~打包进 mac-config Brewfile~~ ✅ 完成（`just install_skillkit` 构建 + 装进 PATH；未发布前不进 Brewfile）。
2. **基建债**（下次焦点）：CI（GitHub Actions `make check`）、Cargo.toml `[package]` 元数据（description/license/repository）。~~README~~ ✅ 完成（`b7a5a40`）。
3. ~~**server GUI 对齐 remove + 加 find 端点**~~ ✅ find 端点已规划进 GUI parity plan（Task 2，GET /skills/find）；list 端点 GUI 早有（GET /skills = registry 总览）。剩 uninstall→remove 命名清理（server handler 仍叫 `uninstall`，功能已是 remove）——Minor，可在 GUI parity 执行时顺手清或单开。
4. **demo 走查 + ignored e2e 真跑**（本次遗留）：`demo/index.html` Skills 视图 Task 6 Step 6 浏览器手查未自动验；`find_json_returns_candidate_array` + `remove_managed_deletes_canonical_directory` 两个 `#[ignore]` 真跑 npx 用例需 `make e2e-cli` 跑过验证。
5. **`button.u` 无 CSS 规则**（server）：server `skills_main.html` 的 `class="u"` 升级按钮无 CSS（demo 原型已加 `.pill-btn.u`，server 未对齐，Minor）。
6. **GUI Skills 页 install 表单无回归测试**：e2e 已断言 install 表单存在（`test_skills_upgrade_button_only_managed`），后续若改模板需留意（Minor）。
7. **e2e 三层不统一入口**：`make check`（无 e2e）、`make e2e-cli`、`make e2e` 分开跑；未来可加 `make e2e-all` 聚合（Minor）。
8. **GUI parity 执行**（下次焦点，plan 已就绪 `0e0c066`）：跑 `docs/superpowers/plans/2026-08-01-gui-parity.md` 10 task。core 下沉 `scan_projects` + Skills/Projects 视图各 4 端点 + 模板 + fake_npx 测试基建。推荐 subagent-driven 逐 task 执行；实现时按 §8.2 的 13 条 htmx/askama 坑。

## 6. 关键文件路径速查

```
/Users/mywo/lab/skillkit/
├── CLAUDE.md / README.md / Cargo.toml / Makefile / rustfmt.toml
├── crates/
│   ├── core/                  # skillkit-core（lib）—— 业务逻辑
│   │   ├── src/{lib,paths,error,config,source,registry,npx,install,import,upgrade,symlink,profile,project,apply,lock}.rs
│   │   └── tests/{m0_e2e,m1_e2e,m3_e2e}.rs       # 端到端 #[ignore] 真跑 npx skills（m0 2 + m3 1）
│   ├── cli/                   # skillkit-cli（bin）
│   │   ├── src/{main, commands/{source,install,skill,import,upgrade,profile,project,serve}}.rs
│   │   └── tests/e2e_cli.rs                # CLI 全链路 e2e（assert_cmd + 临时 HOME，BDD 风格）
│   └── server/                # skillkit-server（lib）—— Axum + Askama + rust-embed
│       ├── src/{lib.rs, routes/{mod,sources,skills,profiles,projects,sse}.rs}
│       ├── templates/{layout,home,sources,skills,profiles,projects,project_workspace}.html + fragments/
│       ├── static/{htmx.min.js, sortable.min.js, app.css}
│       └── tests/{common/mod.rs, routes.rs}
├── demo/index.html            # GUI 设计原型（Skills 视图已对齐 server：find 搜索框/remove ×/unmanaged badge/列序）
├── e2e/                       # GUI 端到端（python playwright + 真实 chromium）
│   ├── test_ui.py             # 6 用例（导航回归/预览/默认源/增删/unmanaged 角标/upgrade 按钮）
│   └── fixtures.py            # wait_for_serve / assert_nav_single / open_page / seed_registry
└── docs/
    ├── 2026-07-29-skillkit-design.md          # spec（source 模型收敛后的权威）
    ├── design-decisions-2026-07-29.md         # 决策 13/14/15（source 收敛/默认源/名称推导）
    ├── frontend-rules.md                      # 前端 AI 约束（htmx/askama 坑）
    ├── superpowers/{specs,plans}/             # M2/M3 + skill find/list/remove + GUI parity 的 spec+plan
    │   ├── specs/2026-08-01-gui-parity-design.md   # GUI 对齐 CLI 全功能设计（8 缺口 + scan 下沉）
    │   └── plans/2026-08-01-gui-parity.md          # 10 task TDD 实现计划（下次直接执行）
    └── sessions/2026-07-29-skillkit-design.md # 本交接
```

运行时目录（user-visible 状态）：
- `~/.skillkit/`：元数据（config/sources/registry/skills-lock.json/profiles/projects/.lock）。
- `~/.skillkit/.agents/skills/<skill>/`：canonical 池子（npx skills 直接写，内部）。
- `~/.agents/skills/<skill>/`：global apply 落地点（agent 直读；symlink 自池子）。
- `~/.claude/skills/<skill>/`：Claude 桥接（symlink → ~/.agents/skills/）。

## 7. 下次接续工作的最短路径（GUI parity 执行）

### 7.1 冷启动（新会话第一件事）

```bash
cd /Users/mywo/lab/skillkit
git status                                # 工作树应干净（spec+plan 已提交 main）
make check                                # 旧基线全绿（GUI parity 未动代码，应仍全绿）
ls docs/superpowers/{specs,plans}/2026-08-01-gui-parity*.md   # 确认 spec+plan 在
```

**必读**：`docs/superpowers/plans/2026-08-01-gui-parity.md`（10 task，每 task 含完整 handler/模板/测试代码）+ §3.1（npx skills 行为）+ §3.3（GUI parity 设计要点）+ §8.2（13 条 htmx/askama 坑，Task 2-9 实现时逐条遵守）。

### 7.2 焦点：执行 GUI parity plan（10 task）

1. **直接跑 plan**：用 superpowers:subagent-driven-development（推荐，每 task 派 fresh subagent + task 间 review）或 executing-plans（本会话内联 + 检查点）。task 顺序 1→10（Task1 scan 下沉是 Task7 前置；Task2 引入 fake_npx 是 Task3/5 前置；Task3 改 render_skills.summary 签名，Task4/5 依赖）。
2. **每 task 末尾**：`make check` 双绿 + 中文 Conventional Commits（`feat(gui): ...` / `refactor(core): ...`）。
3. **关键实现约束**：写操作（POST）返回 body outerHTML；find/scan 的 GET 返回局部片段；片段外层固定 id（`#find-results`/`#scan-results`）；`SkillsMainTpl`/`SkillsTpl`/`WorkspaceTpl` 加字段后两个 render 分支都要传；fake_npx 假脚本无状态故多测试并发覆盖 PATH 无害。

### 7.3 优先级

1. GUI parity 10 task（plan 已就绪，直接执行）→ 2. 基建债（Cargo.toml 元数据 / CI，退居次优）。

## 7.1 (archive) 之前接续的最短路径（基建债）

基建债曾作为上次焦点，现因 GUI parity 优先而退居次优。两件：① Cargo.toml `[package]` 元数据（description/license/repository，core/cli/server 三 crate 都补，README 已声明 MIT）；② CI（GitHub Actions 跑 `make check`，e2e 三层不进 check，是否额外跑 `e2e-cli` 需 npx 由主人定）。

## 7.2 (archive) 之前接续的最短路径（M3 迁移打磨）

### 7.1a 冷启动

```bash
cd /Users/mywo/lab/skillkit
make check                                     # 全绿
cargo test -p skillkit-core -- --ignored       # m0 端到端真跑 npx skills
make run ARGS="serve --port 7317"              # 起 GUI 走查四视图 + Sources 单输入框
```

**必读**：§3.1（npx skills 行为）+ `docs/design-decisions-2026-07-29.md` 决策 13。若涉及 GUI 扩展或新 htmx 端点，按 §8.2 的 13 条坑实现。

### 7.1b 焦点：M3 三件事

1. **import-existing**：扫描 `~/.codex/skills/`、`~/.cursor/skills/` 等存量 skill 目录，识别 + 登记进 registry（package 语义：source add 或直接安装）。
2. **upgrade**：`skillkit upgrade <id>` 走 `npx skills update` + 重读 skills-lock.json 更新 registry.computed_hash；扫描 project `locked_shas` 列出受影响项目并警告，需 `--yes`（spec §10 line 326）。
3. **Brewfile 打包**：纳入 mac-config Brewfile（build + install）。

### 7.1c 优先级

1. M3 三件（import-existing → upgrade → Brewfile）→ 2. 基建债（CI/README/元数据）。

## 8. (archive) 历史归档

以下为已完成的历史改动与背景，只回查用，不在主文展开。当前状态见 §1-§7。

### 8.1 历史改动清单（原 §2 第 13-0 条）

13. **前端 e2e 测试设施**：`make e2e`——python playwright（pipx 1.55.0）驱动真实 chromium，4 用例覆盖导航重复回归（删除 source 双通道）/实时预览/默认源/增删闭环。serve 加 `--token` 参数（固定 token 供 e2e，默认随机）。e2e 用 `HOME=$TMP` 隔离 + 固定端口 7417（避 7317），不进 `make check`。`e2e/test_ui.py` + `e2e/fixtures.py`（无 pytest 纯脚本）。**踩坑**：`networkidle` 会被 SSE 长连接拖死（改 `wait_until="load"` + `expect` 轮询）；`/ping` 是公开路由不能拼 token base。

12. **Sources GUI 改进 + SSE 片段化**：① `ensure_default` 语义改为「skills.sh 缺失即补回」（修空文件 `sources = []` 时 GUI 空白 bug，决策 14）；② 新增 `derive_source_name`（shorthand/scp-style/url/local path 四形态取末段 + 剥 .git/尾斜杠），CLI `source add <package> [--name]`（**有意的破坏性参数序变更**）、GUI 表单 package 输入实时预览推导名（htmx 调 `/sources/preview` 服务端推导；name 手动编辑后停用预览）；③ **SSE 刷新片段化**：修删除 source 后导航重复 bug——各视图 main 内容提成 `fragments/*_main.html`，页面模板薄壳 include，page handler 支持 `?fragment=1` 返回纯 main 内容，SSE 刷新请求它（响应不含 nav）+ 契约测试；④ **前端 AI 约束**：`docs/frontend-rules.md`（Non-Negotiables/htmx 模式/Askama 坑/Red Flags），CLAUDE.md §7.5。决策 15。

11. **source 模型收敛——统一走 npx skills**：主人 5 轮反馈驱动。删 `SourceType`/`git.rs`；`Source`→`{name, package}`；下载全委托 npx skills；canonical 池子改 `~/.skillkit/.agents/skills/`；版本 `commit_sha`→`computed_hash`；skills.sh 默认源=registry 搜索入口（find 交互选）；`SkillkitError::Git`→`Tool`。CLI `source add <name> <package>`（去类型参数）、`install add <source> <skill> [--scope]`（默认 local）。决策 13。

10. **serve 自动打开浏览器**：serve 默认启动后 `open`/`xdg-open`/`start` 打开默认浏览器；`--no-open` flag 供脚本/CI/测试跳过。listener 绑好后才 open；open 失败只 warn 不挡 serve。

9. **M2 T10-T15**：Projects 视图 + Sources CRUD + Skills install/uninstall + 声明编辑/apply 闭环 + SSE + app.css 产品化。
8. **M2 T1-T9**：server 骨架 → token → 静态 → layout → 文件锁 → Sources/Skills/Profiles 视图。
7. **`.skm` → `.skillkit` 改名 + M2 spec/plan**。
6-0. **M1/M0/工程化/设计**（见 `git log`）。

### 8.2 M2 执行经验（htmx+askama 前端工作回查用，13 条坑）

1. **handler 渲染模式**：不写 `match Tpl{...}.render() {...}`（结构体字面量在 match 头致花括号歧义，rustfmt 报错）。改：`let rendered = Tpl{...}.render();` 再 match，或拆同步 `fn render_xxx(state, token) -> Response` + `render_str` helper。
2. **写操作调视图**：page handler 第一个参数是 `State<AppState>` extractor，不能从写操作传裸 state 调。拆同步 `fn render_xxx(state: AppState, token)`，page 和写操作都调它。
3. **重复 key 表单**：`Form<ReorderForm{order: Vec<String>}>` 不行（serde_urlencoded 不支持重复 key→Vec）。改 `body: Bytes` + `form_urlencoded::parse(&body).filter(...)`。
4. **askama include**：不支持 `{% include "x" with var %}`（共享外层模板上下文）。改外层 for 变量名对齐被 include 模板字段名。
5. **私有 async fn**：clippy `unused_async` 对私有 async fn 无 await 报错。`render_xxx` 改同步 fn（pub handler 保持 async）。
6. **re-export 不混用**：core 公开类型在 `lib.rs` 统一 re-export，handler 全用 `skillkit_core::X`。新增公开类型补进 lib.rs。
7. **clippy pedantic 坑**：`default_trait_access`（`BTreeMap::new()` 取代 `Default::default()`）、`ignored_unit_patterns`（`Ok(())` 取代 `Ok(_)`）、`case_sensitive_file_extension_comparisons`、`single_match_else`（两臂 match 改 `if matches!`）、`empty_line_after_doc_comments`（doc 注释后不空行）。
8. **Axum 0.8 路径参数**：`{token}`（非 0.7 的 `:token`）；State/Path 在前，body extractor 最后；State 要 Clone。`{id}` 单段接受 %2F 解码，前端按钮 URL 里 id 含 / 须 handler 预编码（`m.id.replace('/', "%2F")`）。
9. **尾斜杠 404**：axum 0.8 `/{token}` 严格匹配，`/TOKEN/` 404。额外注册 `/{token}/`。
10. **askama 不支持方法借用参数**：`contains(&meta.id)` 编译失败。handler 预计算 `Vec<(SkillMeta, bool)>`。
11. **htmx 片段替换保 id**：写操作返回片段 outerHTML 替换时，片段外层用固定 id（如 `id="status-panel"`），否则替换后 id 丢失。
12. **SSE 前端**：htmx 2.x 的 sse 扩展拆到独立包，改用浏览器原生 `EventSource` + `htmx.ajax`，零额外依赖。
13. **Scope serde**：`#[serde(rename_all = "lowercase")]`，json 里 `"global"`/`"local"`。

### 8.3 Axum 0.8 要点

见 8.2-8。

### 8.4 历史接续路径

- **M2 完成 → M3**（source 收敛前）：M2 T1-T15 全完成，四视图 + apply 闭环 + SSE + 视觉。次接 M3 迁移打磨（import-existing / upgrade / Brewfile）。
- **M2 T10-T15 阶段（已完成）**：inline 执行 plan Task 10-15，Projects 视图 → 写操作 → apply 闭环 → SSE → 视觉。
- **M2 T1-T9 阶段（已完成）**：inline 执行 plan Task 1-9，server 骨架 → Profiles 写操作（含 SortableJS）。
- **M2 设计阶段（已完成）**：brainstorming htmx（取代 React）→ spec → writing-plans 15 task。
- **M1 阶段（已完成）**：profile/project/apply 幂等落地闭环（10 TDD task）。
- **M0 阶段（已完成）**：core 骨架 + install + 全局 Claude symlink。
- **设计阶段（已完成）**：CLAUDE.md → spec review（P0/P1/P2）→ writing-plans M0。
