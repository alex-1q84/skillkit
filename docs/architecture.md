# skillkit 架构（现状权威）

> 本文件描述**代码现状**的架构事实，是「现在是什么样」的唯一权威。设计推理存档在 `docs/2026-07-29-skillkit-design.md`（spec），取舍决策以 `docs/design-decisions-2026-07-29.md`（决策纪要，文中引用格式「决策 N」）为准。历史 spec 与评审文档不再回改；它们与现状的差异，以本文件 + 对应决策编号取代注记为准。代码层规范见 `CLAUDE.md`，前端规范见 `docs/frontend-rules.md`，发版见 `docs/release.md`。

## 1. 定位与三层架构

skillkit 是 AI agent skill 的统一管理工具：注册安装源、安装并锁版本、按 profile 组织候选集、按项目精确落地到各 agent 目录。Rust 单二进制，CLI（供 AI agent 高频调用）+ 本地 web GUI（供人总览配置）共享同一 core，前端产物嵌入二进制，零运行时依赖。Cargo workspace 三 crate（`Cargo.toml:2`）：

| crate | package | 职责 |
|-------|---------|------|
| `crates/core` | skillkit-core | **全部业务逻辑**：`install` / `install_local` / `upgrade` / `import` / `dupes` / `scope` / `apply` / `detect` / `registry` / `profile` / `project` / `source` / `npx` / `symlink` / `lock` / `migrate` / `paths`（模块清单见 `crates/core/src/lib.rs`） |
| `crates/cli` | skillkit-cli（bin `skillkit`） | 薄壳：clap 解析 → 调 core → 呈现（`--json`/表格），顶层 anyhow 聚合错误 |
| `crates/server` | skillkit-server | 薄壳：Axum handler → 调 core → Askama 渲染；`serve` 子命令经 `skillkit_server::run` 内建 Tokio runtime 启动 |

CLI 与 server 不允许出现重复业务逻辑（CLAUDE.md §3 不变量）；「load → core → save → 渲染」之外 handler/命令层不写推导。

**无常驻 daemon**（决策 3）：CLI 直调 core；`skillkit serve` 启 Axum 也直调 core，退出即止。跨进程实时性靠「core 写 `~/.skillkit/` 状态文件 + server 端 notify 监听 + SSE 推送」：每个被监听目录一个全局 watcher 线程，事件按文件分类（registry/sources/profiles/projects）广播（`crates/server/src/routes/sse.rs`）。SSE 唯一用途即此——CLI 在另一进程改状态后浏览器自动刷新；`apply` 等同步请求直接返回结果，不走 SSE（决策 24）。

数据流（写入全部经 core，原子写 = 临时文件 + rename）：

```
CLI 命令 ─┐
          ├→ skillkit-core ─→ ~/.skillkit/（registry.json / sources.toml / config.toml /
server   ─┘                  │             profiles/ / projects/ / .lock/ 状态文件）
(Axum+htmx)                  ├→ ~/.skillkit/.agents/skills/   canonical 物理池（npx skills 写入布局）
                             ├→ ~/.agents/skills/ + ~/.claude/skills/   global 两层 symlink 桥接
                             └→ <project>/.agents/skills/ 等   项目 local 落地（copy/symlink）
```

两个入口启动时都做初始化：`SourcesStore::ensure_default` 补回 skills.sh 默认源 + `migrate()` 升级 registry schema（失败 warn 不阻塞，`crates/cli/src/main.rs:52-58`、`crates/server/src/lib.rs:116-122`）。

### 架构级不变量（任何时候不能破坏）

来源 CLAUDE.md §5，架构含义在此重述：

- `~/.agents/skills/` 只放全局公共 skill（经桥接 symlink），绝不写元数据文件（会被各 agent 误扫描）；元数据统一收 `~/.skillkit/`。
- canonical 物理单份；跨实体只用 id 引用，source/scope/version 只在 registry 存一份。
- 项目 shared skill 只读发现（`scan_shared`），不安装/升级/卸载——由项目 git 自管。
- global 与 profile/project 归属互斥（决策 17）：`add_skill`/`set_profiles` 在 core 拦截 global。
- 项目 local skill 平铺落地 `<agent>/skills/<skill>/`，绝不建 `local/` 子目录（Claude Code 只发现一层目录）；git 忽略只写 `.git/info/exclude`。
- 不碰 Claude 原生插件（`~/.claude/plugins/`）与 `~/.claude/commands/`。
- 路径不硬编码：用户目录一律 `dirs::home_dir()` 解析（`Paths::production`），测试注入 tempdir。

## 2. 前端栈（web GUI）

- **htmx 2.x**（静态文件内嵌 version 2.0.4）+ **Askama 0.13** 服务端渲染片段 + **SortableJS**（profile 拖拽排序）+ **rust-embed** 嵌入静态资源（`crates/server/static/`）——离线可用、无 node 构建链、单二进制零运行时依赖（决策 23）。修正主 spec §12 与决策 2 早期「React + Vite」表述；禁止重型框架是硬约束（CLAUDE.md §2）。
- **UI 状态进 URL + 服务端重渲染**（决策 35）：Skills 页选中态/过滤（`?selected=&profiles=&scope=&unassigned=`，`crates/server/src/routes/mod.rs` 的 `SkillsQuery`）全部走 query 参数，SSE 刷新与过滤 chip 靠 URL 透传，前端不存独立状态。
- 页面模板薄壳 + `fragments/*_main.html` 承载 main 内容；交互约定：**写操作（POST/DELETE）返回完整页面**（`hx-target="body" hx-swap="outerHTML"`），**SSE 刷新请求 `?fragment=1` 纯片段**（`hx-target="main"`，响应不含 nav，防导航重复）。详细规则见 `docs/frontend-rules.md`。

## 3. 物理存储模型

- **canonical 单池**：`~/.skillkit/.agents/skills/`（`Paths::skillkit_skills_dir`，`crates/core/src/paths.rs:48`）。所有安装动作经 npx skills 以 project scope 写入此布局：cwd=`~/.skillkit`、`npx skills@latest add <package> -s <skill> -a universal --copy -y`（决策 13），npx 自己的 `skills-lock.json`（含 computedHash 与 source）也落 `~/.skillkit/`，core 只读它记版本。下载与升级全程委托 npx skills，skillkit 的角色到「调它 + 读它的 lock」为止（决策 1）。
- **单版本 + 元数据锁**（决策 4/9）：每个 skill 物理只有一份，不预分版本目录；版本信息记录在 registry 的 `computed_hash`（源自 skills-lock.json）与 project 的 `locked_shas`。
- **agent 目录桥接**（决策 8）：global skill 由 core 建两层 symlink——池子 → `~/.agents/skills/<name>`（Cursor/Codex 等直读）→ `~/.claude/skills/<name>`（Claude 桥接），幂等、真实目录占位报错不静默删；卸载/降级对称撤链（先 claude 后 agents，反序留悬空链，`crates/core/src/symlink.rs`）。
- **unmanaged 存量迁池**（决策 42）：`import` 把 `~/.agents/skills/`、`~/.claude/skills/`、`~/.codex/skills/`、`~/.cursor/skills/` 下的真实目录 adopt 入池（rename 原子、池子权威）+ 原位补 symlink 桥接；registry 既有 unmanaged 记录由 `relink_unmanaged` 按 registry 遍历幂等补迁（`crates/core/src/import.rs`）。
- **两伪源对称**（决策 27/37）：`unmanaged`（存量登记，`computed_hash=None`，不可升级）与 `local`（本地目录/zip 安装，id 形如 `local/<name>`，`crates/core/src/install_local.rs:295`）都只是 `SkillMeta.source` 的虚拟命名，不进 `sources.toml`。

## 4. 元数据布局与 schema

```
~/.skillkit/
  config.toml          # agent 能力声明（core/config.rs）
  sources.toml         # 安装源注册表 {name, package}（core/source.rs；package=None 即 skills.sh 搜索入口）
  registry.json        # 已装 skill 元数据（core/registry.rs）
  skills-lock.json     # npx skills 写的 lock（core 只读）
  profiles/<name>.toml # profile：只存 skill id 列表（DRY）
  projects/<id>.toml   # project：agents/applied_profiles/installed_skills/locked_shas
  .lock/<key>.lock     # 并发锁文件（key 见 §9）
  .agents/skills/      # canonical 物理池
```

- `registry.json` 顶层 `{version, skills}` 版本化（`crates/core/src/registry.rs:48`）。**启动自动迁移**（决策 48）：CLI main 与 server serve 启动时调 `migrate()`，按 version 逐级升级；原则是确定性回填（数据只来自本地 skills-lock.json，不猜不联网，查不到留 None）、幂等、失败仅 warn 不阻塞（`crates/core/src/migrate.rs`、`crates/cli/src/main.rs:56`）。修正主 spec §8.3 的无版本直出 map 旧示例。
- `SkillMeta` 含 `spec`（registry 源原始安装名 `owner/repo@skill`，仅来源记录，npx update 只认短名，此字段不参与升级调用）与 `version` 字段（`crates/core/src/registry.rs:24`）。完整字段：`id` / `name` / `source` / `spec?` / `scope` / `version?` / `computed_hash?` / `installed_at` / `canonical_path`。
- `Project`（`~/.skillkit/projects/<id>.toml`）：`installed_skills` 是 apply 的唯一依据，`locked_shas` 是上次 apply 的基线快照（值为 computed_hash，非版本锁）；`applied_profiles` + `set_profiles` 替换语义重算并集（global 一律不进 project，决策 17 硬约束）。
- `Profile`（`~/.skillkit/profiles/<name>.toml`）：`{name, description, skills}`，只存 id 列表。

## 5. 标识与引用

- **skill id** = `<source>/<skill-name>`（`Registry::skill_id`），profile/project/URL 全用它引用；source、scope、版本只在 registry 存一份（DRY，决策 7）。进 web URL 单段路径参数时 `/` 编码为 `%2F`。
- **project-id** = uuid v4 前 8 hex 大写随机短码，注册时生成、此后冻结；`rebind` 改 path/name 不改 id（`crates/core/src/project.rs:30`）。修正主 spec §6.4「目录名-路径 SHA256」路径派生旧设想（决策 22）——id 独立于路径，项目移动/改名后 rebind 即可。

## 6. CLI 命令全景

git-style 分组（`crates/cli/src/main.rs`）：`source` / `install add|local` / `find` / `list` / `remove` / `rescope` / `import-existing` / `dupes list|trash|adopt` / `profile` / `project` / `upgrade` / `serve`。各组子命令现状：

- `source add|list|remove`：源注册表管理（`package` 是 npx skills source format 串；name 可自动推导，决策 15；skills.sh 缺失自动补回，决策 14）。
- `install add <source> <skill>`：固定源直接装；skills.sh（registry）源先 `npx skills find` 列候选交互选（`--json` 时只输出候选数组不安装，供 agent 决策）。
- `find <query>` / `list [--unassigned]`：纯查询。
- `remove <id>` **完全替换** uninstall、无别名（决策 29，`crates/cli/src/commands/skill.rs:116`）。
- `rescope <id> --scope global|local`、`import-existing [--dry-run]`。
- `dupes list|trash <names...>|adopt <name>`：处理 `~/.agents/skills/` 同名冗余副本（列出含与池子正主的一致性对比 / 删至回收站 / 以副本覆盖正主，正主先送回收站，managed 正主拒绝 adopt 防破坏升级语义，决策 51，`crates/core/src/dupes.rs`）。
- `install local <目录|zip> [--name] [--scope] [--force]`（决策 37）；`profile create|add-skill|remove-skill|list|show|delete`；`project add|rebind|remove|scan|apply-profile|add-skill|remove-skill|apply|status|list`。
- 修正主 spec §11 命令清单。
- 全命令支持 `--json`，其输出 schema 视为**公开契约**（AI agent 依赖其稳定）；危险操作（remove / project remove / profile delete）默认交互确认，`--yes` 跳过（`remove --json` 隐含跳过）；`dupes trash/adopt` 是例外——直接执行不确认（删除进系统回收站可逆，是它不加确认的依据，`crates/cli/src/commands/dupes.rs:63`）。`project status` 是感知接口（expected/missing/extra/conflicts 四清单）、`project apply` 是执行接口，二者共用 `scan_extras` 判定闭环不漂移。

## 7. Web GUI

- `serve` 只绑 `127.0.0.1`（默认端口 7317），随机 uuid token 入 URL 路径首段；token 不匹配一律 404，不泄露路由存在性（决策 26，`crates/server/src/lib.rs:100-129`）。本机单用户模型，`/ping` 与 `/static/*` 公开。
- **四大视图 + 项目 workspace 详情页**：nav 顺序 Projects / Sources / Skills / Profiles（`crates/server/templates/layout.html`）——Projects（注册/扫描发现/toggle 勾选注册）、Sources（源注册表 + add 预览推导 name）、Skills（registry 总览 + 安装向导）、Profiles（候选集，SortableJS 拖拽排序）；`/projects/{id}` workspace 页（`project_workspace.html`）提供 rebind、sync-agents（重探测 agent）、browse、绑定 profiles 面板（全量替换 `installed_skills` 并触发 apply）与 status 片段（`crates/server/src/routes/projects.rs`）。修正主 spec §12 四视图旧范围与 08-07 spec「不做上传」旧边界。
- **Skills 视图 = 安装向导**：find 搜 skills.sh 渲染候选，每条带本地**占用高亮**（同名 registry 记录或未登记池目录，`occupied_of`）与一键/`force=1` 覆盖安装（决策 49）；点 skill 名弹浮层查看池内 SKILL.md 原文（等宽呈现，`skill_md` handler + `fragments/skill_md_modal.html`）；dupes 入口同页。
- **安装本地 skill Modal 三合一 multipart 上传**（决策 41）：zip（`archive` 字段）/目录（多 `file` part）/表单（`name`/`scope`/`force`）一个端点分流，body 上限 100MiB（`MAX_UPLOAD_BYTES`，`crates/server/src/routes/skills.rs:787`）；handler 落临时目录后调 core `install_local`，**core 零改动**。
- **长阻塞 handler 一律 spawn_blocking**（决策 31）：npx 网络调用（find）、import 扫描迁池、upgrade 批量、Skills/Dupes 整页渲染均卸到 blocking 线程池，不占 Tokio 工作线程（`crates/server/src/routes/skills.rs:236` 等 8 处）；轻量单文件读写的 handler 直接同步调用。
- 写操作错误统一 422 + `{"error"}`，htmx 收 4xx 不 swap、前端 JS 弹 toast 不刷页（`crates/server/src/routes/mod.rs:17`）。

## 8. 关键生命周期

- **install**：解析 package → `npx skills add` 写池（锁外，网络秒级）→ 读 computed_hash → 持锁登记 registry（锁内重读，防旧快照覆盖）。同名占用防护两层：目录已占非 force 报 `SkillAlreadyInstalled`；stale 登记或池内同 name 双登记报 `SkillPoolOccupied` 引导清理。`force` 覆盖 = 复用 uninstall 清场（managed 撤桥接/unmanaged 摘记录）+ 孤儿目录补删，**清场硬删不走回收站**（覆盖语义本就紧跟替换，`crates/core/src/install.rs:88`）。
- **import**：同名 dedupe 幂等收敛（保守规则：全组 canonical 同池同路且恰一条 managed 才摘多余 unmanaged，否则点名留人工）→ relink 存量迁池 → 扫描四目录 adopt 入池 → 持锁登记 → 桥接；桥接撞真实目录占位**降级 skipped 点名**不中止（清占位后重跑收敛）。
- **remove（uninstall）**：一律系统回收站可逆（决策 50，`system_trash` 经 `trash::delete`）；managed 先撤两层桥接防 dangling，unmanaged 池内 canonical 同送回收站，**池外手工目录保留仅摘记录**（防误删用户资产，warn 点名路径）。
- **rescope**（决策 18）：local→global 先改 scope → 建桥接 → 落盘 → 清 profile/project 引用（跨文件非原子，失败给可恢复文案）；global→local 探测全局位置真实目录迁池 → 撤桥接 → 落盘。全程持 registry 锁。
- **upgrade**：升级前算受影响项目（locked_shas == 旧 hash），非空且未确认返回 `UpgradeBlocked`；`--all` 把 blocked 汇总进报告不中断（`crates/core/src/upgrade.rs`）。单 skill 走 `--yes`，GUI 批量按 yes=false 拦截报告（决策 28/32）。
- **apply**（幂等）：`compute_diff`（纯逻辑）→ global ensure 桥接 → local 落地 → extra 清理 → exclude 重写 → 重建 `locked_shas`。`--frozen` 遇 sha 漂移直接报错（CI 语义）。落地目标由 `landing_agents` 派生（决策 20）——开源标准 `agents` **总在列**（`.agents/skills/` copy + `.skillkit-sha` 标记，`agents=[]` 也保底），探测到的 agent 中 `reads_agents_dir=false` 的（默认配置即 claude-code）额外落私有目录 symlink 桥接；cursor/codex 直读不落私目录。copy 模式靠 `.skillkit-sha` 标记判断副本是否过期（漂移触发重 copy）。extra 清理与 status 共用 `scan_extras`（「感知-执行」闭环一致），**别名目录豁免按 canonicalize 物理路径判定**（`.claude/skills`、`.cursor/skills` symlink 同指共享池时不互删、不误报，决策 45，`crates/core/src/apply.rs:244`）。local 落地清单写 `<project>/.git/info/exclude` 的 skillkit 管理段（`# >>> skillkit managed >>>` 块），不动 `.gitignore`（决策 12）。结尾重建 `locked_shas` 为 expected 快照，孤儿锁 apply 一次自愈。`compute_status` 是 CLI status 与 server workspace 片段共用的组装单点（`crates/core/src/apply.rs:413`）。

## 9. 并发模型

- fs2 advisory flock（`try_lock_exclusive` 轮询，50ms 间隔），**per-key** 锁文件 `~/.skillkit/.lock/<key>.lock`（key：`registry` / `sources` / `config` / `profile-<name>` / `project-<id>`），写锁 5s 超时报 `LockTimeout`，读不锁（决策 25，`crates/core/src/lock.rs`）。guard drop 自动释放。
- `with_registry` 是 registry 写事务单点：acquire → load → 闭包 → `save_raw`（不重取锁，防同进程 flock 自死锁；`crates/core/src/registry.rs:114`）。`set_scope` 有意全程直接持锁（物理迁移窗口完整）；`install_local` 的 force 清场在锁外调 `clear_occupied`（内部各自持短锁），就位与 registry 写持同一把锁（决策 52）。同进程 flock 不可重入是所有「save_raw / 先 drop 再取下一把锁」写法的根因（决策 46）。
- **声明缺口**（`docs/TODO.md` §二，9ffd194 后现状）：registry 写回段已全部持锁、覆盖回滚根治；但**物理段**（npx 写池、删目录/回收站、rename 迁池）有意留在锁外（防 5s 锁超时），物理并发互踩仍是非目标取舍；池级共享锁未建。不要引用主 spec §3.7「不持任何锁」旧表述。
- 测试基建同款互斥：凡测试改进程环境变量（PATH 注入 fake npx、回收站重定向 `SKILLKIT_TEST_TRASH_DIR`）都持进程级 `ENV_LOCK` 串行，堵 macOS 并发 setenv 竞态（core 与 server 测试各自实现）。

## 10. 安全与信任边界

- serve 本机单用户模型：只绑回环地址 + 随机 token 路径前缀，无多用户/鉴权体系；静态资源 `/static/*` 与 `/ping` 公开（localhost 无泄露风险）。
- `install local` 对不可信输入（GitHub zip、外部目录）五件套（决策 38，`crates/core/src/install_local.rs`）：① name 校验防路径逃逸（拒 `..`/分隔符/非法字符）；② zip 逐条目 `enclosed_name` 拒穿越 + 拒 symlink 条目；③ 复制/哈希跳过 symlink（防池外内容混入）；④ 体积 100MiB / 条目 10000 上限防 zip bomb；⑤ tempdir → 校验 → staging 单段 rename 落池（持锁，registry 写失败回滚删 target；force 清场语义见决策 52）。
- 删除可逆原则（决策 50）：用户交互路径的删除一律送系统回收站；仅 force 覆盖清场硬删。

## 11. agent 适配模型

- **能力配置驱动**（决策 8）：`config.toml` 声明每个 agent 的 `supports_symlink` 与 `reads_agents_dir`，落地策略由此派生；新增 agent 只改配置不改代码。`Config::default()` 预置 claude-code（symlink、不直读）/ cursor / codex（copy、直读）。
- **注册/绑定/同步时精确探测**（决策 19，`crates/core/src/detect.rs`）：对项目根按三阶段判定——①配置目录优先（`.claude`→claude-code、`.codex`→codex、`.cursor`→cursor、`.agents`→开源标准，命中多个全返回）；②无配置目录按指令文件（`CLAUDE.md`→claude-code、`AGENTS.md`→开源标准）；③全部未命中回退开源标准 `.agents/`——绝不默认给全 agent 建目录；旧项目 `sync-agents` 一键校正。
- **落地目标与探测解耦**（决策 20）：`proj.agents` 只作展示与 shared 扫描依据，落地集合由 `landing_agents(config, proj.agents)` 派生（见 §8 apply）。`scan_shared` 只读发现项目各 agent 目录与 `.agents/skills/` 共享池的真实目录（项目 git 自管，skillkit 不安装/升级/卸载）。

## 12. 测试与分发

- 测试约定见 CLAUDE.md §8（验证业务结果而非实现细节；tempdir 模拟全目录树；`--json` schema 锁定测试；git 操作真跑不 mock；集成测试放对应 crate 的 `tests/`）。日常验证统一走 Makefile：`make format`（rustfmt 应用）、`make lint`（fmt --check + clippy `-D warnings`）、`make test`、`make check`（toolchain + format + lint + test 一站式，CI 与本地同规则）。
- e2e 三层：`make e2e` 浏览器级（python playwright + chromium 真实浏览器，固定 token + 临时 HOME 隔离 + 独立端口 7417，`e2e/test_ui.py`）；`make e2e-cli` CLI 全链路（assert_cmd 驱动真实二进制，`cargo test -p skillkit-cli --test e2e_cli -- --ignored`）；联网用例标 `#[ignore]` 手动跑，现状 11 处（cli e2e 8 + core m0 2 + m3 1），均不进 `make check`。
- 分发：GitHub Release 预编译二进制（aarch64 tarball + sha256）+ 公开 tap `alex-1q84/homebrew-tap` 由 release workflow 自动更新 formula；当前已发布 v0.1.8（workspace 三 crate 同版本）。不走 crates.io（缓行）。流程与坑位见 `docs/release.md`。

## 13. 文档地图

| 文档 | 边界 |
|------|------|
| `docs/2026-07-29-skillkit-design.md` | spec：设计推理存档（历史快照，不再回改） |
| `docs/design-decisions-2026-07-29.md` | 决策纪要：取舍权威（「为什么这样选」） |
| `docs/architecture.md`（本文件） | 现状架构权威（「现在是什么样」） |
| `CLAUDE.md` | 代码层规范（怎么写代码） |
| `docs/frontend-rules.md` | 前端权威（server crate 前端部分细化） |
| `docs/release.md` | 发版权威 |
| `docs/TODO.md` | 全项目待办唯一清单 |

冲突裁决顺序：现状以本文件为准，理由以决策纪要编号为准，规范以 CLAUDE.md / frontend-rules.md 为准；spec 与本文件冲突处，一律视为已被对应决策取代。
