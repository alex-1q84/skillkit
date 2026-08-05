# SkillKit 设计决策纪要

- 日期：2026-07-29
- 性质：设计讨论的决策推理记录，记录"为什么这么选"及被否定的备选方案。最终设计见 `2026-07-29-skillkit-design.md`。

## 决策 1：独立实现核心，npx skills 只做下载

> ⚠️ 下载环节已被**决策 13**取代：从「skills.sh 走 npx skills + 私有/本地自研 git clone」收敛为「全部委托 npx skills」。核心逻辑（版本/profile/项目/落地）仍由 skillkit 独立实现，此点不变。

**背景**：需要 profile 隔离、项目级管理、跨 agent 同步，这些 npx skills 都不支持。

**决策**：skillkit 独立实现 skill 引擎的全部核心逻辑（版本管理、profile、项目、落地）。npx skills 只保留一个职责——从 skills.sh 源下载 skill 到 `~/.agents/skills/`。

**理由**：npx skills 的安装路径写死（`.claude/skills/` 或 `~/.claude/skills/`），无法做 profile 子目录隔离，也无法控制跨 agent 的落地位置。要真正掌控安装位置，必须自己实现核心。

**否定的备选**：
- 包装 npx skills 作为底层引擎：受限于固定路径，只能靠 enable/disable 软切换做 profile，做不到真正的目录隔离。
- 混合后端（公开 skill 走 npx skills、私有走自研）：两套存储位置并存，一致性和复杂度风险最高。

## 决策 2：技术栈选 Rust + Axum

**背景**：需要 CLI（AI agent 高频调用）和本地 web GUI 共享同一核心。

**决策**：Rust 写核心库，同一核心编译出 CLI 单二进制，Axum 起本地 web server，前端 React+Vite 产物用 rust-embed 嵌入二进制。

**理由**：CLI 会被 AI agent 在每次会话中多次调用，Rust 单二进制毫秒级启动、零运行时依赖，且能纳入 mac-config 的 Brewfile 统一分发，与现有 cx/rtk 工具链一致。前端确定用浏览器形态后，桌面打包框架（Tauri/Wails）被排除，后端就是一个本地 HTTP server 加内嵌核心。

**否定的备选**：
- TypeScript（Hono/Fastify）：与 npx skills 同生态、开发最快，但需要 Node 运行时、CLI 冷启动慢，对 agent 高频调用不友好，分发不如单二进制干净。
- Go（Wails/Echo）：单二进制且开发比 Rust 快，但与现有 npm 生态契合度一般。

## 决策 3：无常驻 daemon

**决策**：CLI 直接调用核心库，`serve` 启 web server 也直接调核心库，不引入常驻 daemon。

**理由**：状态实时性靠核心库写状态文件 + server 端 file watcher 监听变化经 SSE 推送即可保证（CLI 不必通知 server）。daemon 会引入生命周期管理、进程协调等复杂度，对这个工具不必要。CLI 与 web server 并发写配置用文件锁解决。

## 决策 4：物理存储用 Symlink 池 + 单版本

**背景**：需要控制落地位置、版本可锁定。

**决策**：canonical 集中存一份物理副本，agent 目录放 symlink（或对不支持 symlink 的 agent 用 copy）。版本锁作为元数据记录，不为多版本预先实现物理分目录。

**理由**：与现状（opencli-* 的 symlink 模式）一致，零迁移摩擦；单版本 + 元数据锁已能满足"记录和锁定版本"（记录 computed_hash，升级时校验）。skill 是指令集，版本兼容性问题比软件库轻，多版本并存是 YAGNI。

**否定的备选**：
- 多版本并存（canonical 按版本分目录 `~/.skillkit/skills/<skill>/<version>/`）：支持同 skill 多版本并行，但占空间、元数据和升级逻辑复杂，当前无此需求。预留升级路径，未来需要时不破坏现有抽象。

## 决策 5：`~/.agents/skills/` 只放全局公共 skill

**背景**：发现 `~/.agents/skills/` 是通用 AI agent 加载目录，Cursor、OpenCode、Codex、Gemini 等除 Claude 外的大部分 agent 都直接从此目录加载。

**决策**：`~/.agents/skills/` 专属全局公共 skill，绝不挪用为项目级暂存，元数据也不放进去。所有 skillkit 元数据统一收 `~/.skillkit/`。

**理由**：挪用通用加载目录会污染其他 agent 的 skill 视图，混淆 local/shared 边界。把全局公共 canonical 选在 `~/.agents/skills/` 本身就让 Cursor 等零配置可用（直接读），只有 Claude 需要 symlink 桥接（Claude 不直接读 .agents）。这一约束也促使项目 local canonical 从"每项目各放一份"改为集中到 `~/.skillkit/.agents/skills/`（npx skills 直接写入的标准布局）。

## 决策 6：项目 skill 分 local / shared 两类，shared 不由 skillkit 管

**背景**：项目里有的 skill 要入仓库随团队分发，有的要共享但不入仓库。

**决策**：
- local（不入库）：canonical 集中在 `~/.skillkit/.agents/skills/`，与 shared 同级平铺落地到 `<project>/<agent>/skills/<skill>/`（symlink 或 copy），git 忽略走 `<project>/.git/info/exclude`（本地不入库）。
- shared（入库）：真实文件直接在 `<project>/<agent>/skills/`，git 提交，skillkit 只做只读发现，不安装/升级/卸载。

**理由**：shared skill 既然在 git 里，项目自身（git + 团队约定）已经在管理它，skillkit 重复管是多余，违反最小改动和 YAGNI。skillkit 对 shared 只需能看到清单，方便与 local 对照展示。

**演进**：早期设想用 `.skillkit/shared.lock` 锁文件管理 shared 的版本，明确放弃——shared 由 git 管，不需要第二个版本管理器。

## 决策 7：registry 用 id 引用，profile 退成粗分类，project 做精确选择

**背景**：profile 和 project 的职责需要厘清，且 source/scope 等信息不该在多处重复。

**决策**：
- registry 给每个 skill 一个 id（`<source>/<skill-name>`），作为跨实体的唯一引用。
- profile 只存 id 列表，是"这类场景可能用到的 skill 候选集"（粗分类）。
- project 的 `installed_skills` 是 apply 的唯一事实依据，精确到每个 skill，是所应用 profile 候选集的子集选择。

**理由**：id 引用消除冗余（DRY）。profile 是粗分类 + 批量操作入口，project 是精确事实，职责分明。profile 新增 skill 不会自动装到项目，用户必须显式选择，符合"精确控制"。

## 决策 8：按 agent 能力选落地策略

**决策**：在 `~/.skillkit/config.toml` 声明每个 agent 的能力（是否支持 symlink、是否直读 `~/.agents/skills/`），apply 时据此选落地方式。Claude 用 symlink，Cursor 不支持 symlink 用 copy 兜底，OpenCode/Codex/Gemini 全局层面直读无需操作。

**理由**：Cursor 不支持 symlink 是硬约束，但它直读 `.agents/skills/`，所以全局公共 skill 对它零配置；只有项目 local skill 需要用 copy 兜底。把能力声明放配置，新增 agent 不改代码。

## 决策 9：版本策略为单版本 + 元数据锁 + 冲突检测

**决策**：canonical 物理存储只有一份，版本锁记录在 registry 和 project 的 `locked_shas`。升级时扫描所有 project 的锁，发现冲突则警告并列出受影响项目。

**理由**：团队一致性靠源仓库锁（团队都从私有 skill 仓库的同一个 tag 安装），不靠每项目本地存不同物理版本。单版本模型下 `locked_shas` 并非"锁死版本"——canonical 只有一份，升级即全局物理更新；locked_shas 记录的是上次 apply 的基线，作用是让 canonical 变更被感知（apply/upgrade 时比对、提示受影响项目），而非让项目停在旧版。

## 决策 10：工具命名 skillkit

**背景**：占位名 skm 需要验证重名。

**排查结果**：
- `skm`：brew 上是 TimothyYe/skm（SSH key manager，命令直接冲突）；crates.io 上 pyrex41/skill-manager 已占且功能几乎一样；reorx/skm 也是同类。排除。
- `knack`：crates.io 已存在，描述就是 Agent Skills CLI，同类撞车。排除。
- `skiff`：crates.io 是一门编程语言。排除。

**决策**：skillkit。brew + crates.io 双干净，语义贴（skill + kit 工具箱），与主人环境里的 skvm 区分足够。

## 决策 11：分阶段 M0-M3，GUI 在 CLI 闭环之后

**决策**：M0 骨架（source/install/全局 symlink）→ M1 闭环（profile/project/apply）→ M2 GUI → M3 迁移打磨。

**理由**：M1 完成即达成核心目标（CLI 闭环可用），GUI 是锦上添花放在 M2，迁移现有 skill 放 M3。每段独立可验证、可交付。

## 决策 12：local skill 平铺落地，git 忽略用 .git/info/exclude

**背景**：原设计把项目 local skill 落到 `<agent>/skills/local/<skill>/` 子目录，便于一条 `.gitignore` 忽略。但 review 发现 Claude Code 只发现 `.claude/skills/<skill>/SKILL.md` 一层，子目录（含 `local/`）完全不发现（issue #39138），且 Claude Code 不支持自定义 skill 路径（issue #22902 未实现），local 必须平铺。

**决策**：local 与 shared 同级平铺在 `<agent>/skills/<skill>/`；区分靠 skillkit 的落地清单（`installed_skills` 里 scope=local 的）；git 忽略改用 `<project>/.git/info/exclude`（git 天然本地、不入库），apply 动态维护。

**理由**：平铺满足 Claude 发现约束；`.git/info/exclude` 不污染团队 `.gitignore`、不需"忽略自己"的别扭写法、团队成员各自本地维护互不冲突；落地清单本就由 skillkit 管，生成 exclude 是 apply 的自然副产品。

**否定的备选**：
- 维持 `local/` 子目录：Claude 不发现，直接失效。
- 平铺 + 项目 `.gitignore` 动态清单：清单入库，团队成员 local 不同导致提交冲突。
- 取消 local 物理隔离、全部装全局池（方案 B）：消除整套落地复杂度，但放弃 per-project 精确控制，与 spec §2"精确到逐个 skill 指定"目标相悖，未采纳。

## 决策 13：source 模型收敛——统一走 npx skills

**背景**：M0 实现 skills.sh 源时偷懒用 git clone 顶替 npx skills（`install.rs` 把 `SkillsSh` 和 `Git` 合并走 `fetch_git`），`SourceType` 三分（skills-sh/git/local）实际是死区分。自研 git clone 要维护 `git.rs`，而 npx skills 本就支持 github shorthand / git url / local path 三种 source format。

**决策**：所有 source 的下载统一委托 `npx skills add <package>`（package 用 npx skills 的 source format）。skillkit 不再自己 git clone/复制，删 `git.rs`。`Source` 极简成 `{name, package}`，skills.sh 降级为默认预置源（registry 搜索入口，无固定 package）。canonical 池子从 `~/.skillkit/skills/` 改到 `~/.skillkit/.agents/skills/`（npx skills project scope 在 `cwd=~/.skillkit/` 直接写入），`skills-lock.json` 的 `computedHash` 取代 `commit_sha` 做版本锁。

**理由**：消除死区分（SourceType）、删自研下载层（`git.rs` + `fetch_git`/`fetch_local`）、复用 npx skills 的安全扫描和 source 解析。两个目录职责分离：池子（install 落点）vs 落地点（apply 后 agent 直读），不污染 home 根的 `~/.agents/skills/`。代价：删 `ref`/`skills_dir` 字段，不能锁分支/tag，版本纯 lock-based（computedHash + `npx skills update`）——可接受，npx skills 本就不支持指定 ref。

**否定的备选**：
- 保留三分类型 + 自研 git clone：死区分 + 维护两套下载逻辑，spec 设想的 npx skills 路径从没真用。
- 隔离收割（临时目录跑 npx skills 再搬到 `~/.skillkit/skills/`）：搬运依赖 npx skills 输出结构，脆弱；npx skills 状态丢失，升级要重走临时流程。
- skills.sh 默认源指向固定仓库（如 vercel-labs/agent-skills）：skills.sh 是 registry 不是单仓库，固定仓库漏掉生态里其他 package 的 skill。

## 决策 14：skills.sh 默认源缺失即补回

**背景**：`SourcesStore::ensure_default` 原实现只在 sources.toml 文件不存在时种入 skills.sh，但空文件（`sources = []`，用户手动删后保存）会让 GUI/CLI 看不到默认源。

**决策**：`ensure_default` 语义改为「缺失即补回」——CLI main / server serve 每次启动时 load 现有 sources.toml，若列表里没有 `name="skills.sh"` 就补回 `{name: "skills.sh", package: None}`。覆盖原设计「用户可删、删了不加回」的语义。

**理由**：skills.sh 是产品的默认搜索入口，不该因用户删过一次就永久消失；GUI 必须有默认源可展示。用户若改过 skills.sh 的 package，仍尊重（只按 name 判断，不动已存在的条目）。

**代价与竞态**：并发冷启动（CLI 与 serve 同时）可能各 push 一条 skills.sh，但终态收敛为一条（内容相同，后写覆盖），与既有 add 路径的 read-modify-write 竞态同类，不额外处理。

## 决策 15：source name 自动推导（新增只填 package）

**背景**：GUI sources 表单要求用户填 name，但 name 完全可以从 package 推导（git url 取仓库名、本地路径取目录名），徒增心智负担。

**决策**：新增 `derive_source_name(package)` 纯函数（core）：按「shorthand / scp-style / 其它 url / 本地路径」四形态取末段，统一剥尾斜杠 + 一个 `.git` 后缀；空串返回 `None`。CLI `source add` 签名改为 `<package> [--name <别名>]`（**有意的破坏性变更**，位置参数从 name 换成 package）；GUI 表单 package 输入时经 htmx 调 `/sources/preview` 端点实时预填推导名（`input changed delay:300ms`，服务端推导，前端零规则副本），name 框留空则提交时后端推导，用户手动编辑 name 后停用预览（一行 `oninput` 互斥）。

**理由**：对齐「系统承担复杂性」——用户只填 URL，名字自动来，撞名时 `--name`/name 框覆盖。`install.rs` 的 id 契约 `<source-name>/<skill-name>` 不受影响：name 仍存在、仍唯一，只是新增时允许推导默认值。实时预览是主人明确要求（「输入 url 立即显示推导名以便确认」），htmx 走服务端推导不复制规则。

**否定的备选**：
- 前端实时推导（JS 复制规则）：推导逻辑双份实现会漂移，违背「业务逻辑只在 core」。改用 htmx 调服务端 `/sources/preview`。
- 提交后刷新才显示真名：用户确认推导名太晚，已改为实时预览。
- 去掉 name 字段、纯自动推导：撞名场景无法覆盖（两个仓库都叫 skills 时只能换包名绕开），保留可选覆盖成本极低。

**后续提醒**：本次只做新增时推导，不做已有 source 的改名编辑；若未来加「编辑 source name」，必须连带处理 registry 里 `<旧name>/<skill>` 的引用。

## 决策 16：默认声明主流 agent + scan_shared 认项目级 .agents 共享池

**背景**：sea-office-workspace 项目里有 `.cursor/skills` 下的 shared skill，skillkit 完全没认出。根因有二：① `Config::default()` 只声明 claude-code，用户不手写 config.toml 就不启用 cursor/codex；② `scan_shared` 只扫 `proj.agents` 里的 agent 目录，把「shared 只读发现」耦合到了「skillkit 声明管哪些 agent」。而 spec §7 的能力矩阵和 §6.2 的 config 示例本就设想要支持 cursor/codex，实现漏了。

**决策**：
- `Config::default()` 默认声明三大主流 agent：claude-code（symlink 桥接）、cursor、codex（均 copy 落地、直读池子）。其余 agent（OpenCode/Gemini 等）用户按需在 config.toml 追加，新增 agent 只改配置不改代码的原则不变。
- `scan_shared` 在遍历 `proj.agents` 各 agent 目录之外，额外扫项目级 `.agents/skills/`——它是 cursor/codex 直读的跨 agent 共享池，与 proj.agents 声明无关，发现的 skill 以 `agents/<name>` 归属展示。
- GUI 详情页加 `POST /projects/{id}/sync-agents` 端点 + 「同步默认 agents」按钮：把 `proj.agents` 设为 Config 当前全 agent，用于旧项目（只绑 claude-code）一键补全。

**理由**：开箱即用覆盖 .claude/.cursor/.codex 三大主流目录，符合 spec §7 已有意图（补齐实现而非新设计）；shared 只读发现的语义本就该反映项目里实际有什么，不该被 skillkit 是否声明管某 agent 限制；旧项目用显式按钮同步，可见可控，不偷偷改用户配置。

**否定的备选**：
- 启动时自动迁移旧项目 agents：省事但静默改用户配置，违背「不静默跳过」。
- scan_shared 完全解耦 proj.agents、扫所有 `.<name>/skills`：通用但有误扫风险（如 .git/skills），且需维护已知 agent 目录集合；当前需求（主流三件套 + .agents 共享池）用 Config 驱动 + 显式 .agents 已够。
- 不加 sync-agents、只改 default 治新项目：旧项目（多为 claude-code）永远漏，当前痛点不解。

**后续提醒**：项目级 `.agents/skills/` 是 spec 原本没有的新概念（spec 的 .agents 全是全局池子），本次新增，仅只读发现；若未来要让它参与 apply 落地，需另立决策。

## 决策 17：global skill 与 profile/project 归属互斥（core 硬约束）

**背景**：原 §8.4 允许 profile 引用 global skill、§10.1 允许 global 进 installed_skills，两层语义都不纯（global 是全局基座却混进场景组合/项目声明）。

**决策**：global skill 不属任何 profile、不进任何 project.installed_skills；core 在 `profile.add_skill`/`project.add_skill`/`project.set_profiles` 加 `&Registry` 参数做 scope 校验（global 拒绝/跳过），文案引导先 `rescope` 到 local。

**理由**：心智模型纯粹（global=全局基座独立、local=场景/项目组合成员），职责一刀切；apply 简化成只管 local 落地。`add_skill`/`set_profiles` 加 registry 参数是必要代价（scope 只存 registry）。

**否定的备选**：仅 GUI 引导不校验——CLI/外部调用能绕过，profile/project 留脏数据。

## 决策 18：scope 转移副作用模型 + 风险对齐确认

**背景**：需要 local↔global 互转，且转移伴随物理落地变更 + 归属清理。

**决策**：
- 转移 = 改 scope + 立即同步物理落地（local→global 建 `ensure_global_claude`；global→local 撤，新增 `remove_global_claude` 不加 scope 守卫避免改 scope 后 no-op 留悬空链）。
- local→global 自动从所有 profile/project 移除引用（不可逆，但可重新归入恢复）；global→local 可逆（rescope global 恢复）。
- 风险对齐：两方向 CLI 都默认交互确认；GUI 直接执行 + 横幅明示影响（去 hx-confirm 方向，commit b15d13e）。
- 原子回滚范围 = scope + registry + symlink；profile/project 多文件移除失败给可恢复文案，不声称全量原子。

**理由**：转移即生效（跟 install/remove 一致）；`remove_global_claude` 不加守卫是规避 set_scope 先改 scope 再撤链的顺序陷阱（spec review P2-A）。
