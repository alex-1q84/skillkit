# SkillKit 设计决策纪要

- 日期：2026-07-29
- 性质：设计讨论的决策推理记录，记录"为什么这么选"及被否定的备选方案。最终设计见 `2026-07-29-skillkit-design.md`。
- 溯源：决策 21+ 来源栏指向的 spec/review/sessions 过程文档已于 2026-10-31 清理（原文见 git 历史），来源路径仅供追溯，非现存文件。

## 决策 1：独立实现核心，npx skills 只做下载

> ⚠️ 下载环节已被**决策 13**取代：从「skills.sh 走 npx skills + 私有/本地自研 git clone」收敛为「全部委托 npx skills」。核心逻辑（版本/profile/项目/落地）仍由 skillkit 独立实现，此点不变。

**背景**：需要 profile 隔离、项目级管理、跨 agent 同步，这些 npx skills 都不支持。

**决策**：skillkit 独立实现 skill 引擎的全部核心逻辑（版本管理、profile、项目、落地）。npx skills 只保留一个职责——从 skills.sh 源下载 skill 到 `~/.agents/skills/`。

**理由**：npx skills 的安装路径写死（`.claude/skills/` 或 `~/.claude/skills/`），无法做 profile 子目录隔离，也无法控制跨 agent 的落地位置。要真正掌控安装位置，必须自己实现核心。

**否定的备选**：
- 包装 npx skills 作为底层引擎：受限于固定路径，只能靠 enable/disable 软切换做 profile，做不到真正的目录隔离。
- 混合后端（公开 skill 走 npx skills、私有走自研）：两套存储位置并存，一致性和复杂度风险最高。

## 决策 2：技术栈选 Rust + Axum

> ⚠️ 前端部分已被**决策 23**取代：React+Vite 独立前端工程改为 htmx 2.x + Askama 服务端渲染片段 + SortableJS（模板编译进二进制、静态资源 rust-embed 嵌入、无 node 构建链）；Rust + Axum 单二进制与 CLI/GUI 共享 core 不变。

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

> ⚠️ 「用户必须显式选择（精确到每个 skill）」半边已被**决策 30**修订：project 的 installed_skills 改由绑定 profiles 的并集自动推导（set_profiles 全量替换语义），手动勾选表单与 update/APPLY 端点已删除；id 引用与 profile 粗分类的职责分层不变。

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

> ⚠️ 第 3 条 sync-agents 语义已被**决策 19**修订（从「补全到 Config 全 agent」改为「重新探测」）；第 2 条项目级 `.agents/skills/` 的只读 shared 角色已被**决策 20**升级为「总是参与 apply 的 local 落地目录 + shared 只读发现」双重角色。

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

## 决策 19：项目 agents 精确探测（注册/绑定/同步时探测，未命中回退 .agents/）

> ⚠️ 第 4 条已被**决策 20**修订：`.agents/skills/` 从「探测未命中才参与 apply」升级为「总是参与 apply」（agents=[] 也保底落地，杜绝有绑定记录无落地）；探测逻辑本身（detect.rs 配置目录→指令文件→回退 .agents/）不变。

**背景**：决策 16 之后注册时默认把 `proj.agents` 填成 Config 全 agent（claude-code/cursor/codex），apply/绑定 profile 会给每个 agent 建 `<project>/.<agent>/skills/` 目录——实际只用 Claude Code 的项目也被建出 `.cursor/skills`、`.codex/skills`，污染项目目录、让 git 状态与 agent 加载目录失真。

**决策**：
- `proj.agents` 不再默认全量，注册/绑定 profile/`sync-agents` 时按项目根目录精确探测：配置目录（`.claude`/`.codex`/`.cursor`/`.agents`）命中任一即按目录判定（多个全收）；无配置目录时按指令文件（`CLAUDE.md`→claude-code、`AGENTS.md`→开源标准）；全部未命中回退开源标准 `.agents/`（`agents` agent，copy 落地）。
- 探测逻辑收敛在 `crates/core/src/detect.rs` 的 `detect_agents`，CLI/server 共用；`Project::refresh_agents()` 供绑定/同步时覆盖旧声明。
- `sync-agents` 语义从「补全到 Config 全 agent」改为「重新探测」（GUI 按钮改名「重新探测 agents」），修订决策 16 第 3 条。
- 项目级 `.agents/skills/` 从「只读 shared 发现」升级为双重角色：shared 真实目录仍只读发现；探测未命中具体 agent 时作为开源标准 local 落地目录（参与 apply）。此即决策 16「后续提醒」预留的另立决策。

**理由**：agent 目录是项目的可见状态，建了用户就得维护（git exclude 多一份、目录多一层噪音）；精确探测从项目自身痕迹（配置目录/指令文件）推断，无需用户额外配置；`.agents/` 是各 agent 通用的开源标准目录，探测失败回退到它语义最稳（Codex/Gemini/OpenCode 都读）。

**否定的备选**：
- 注册时让用户手选 agents（GUI 表单曾有此输入，已去掉）：给用户加负担，且新项目往往还没建任何配置目录，无从选起。
- 探测表配置驱动（config.toml 给每个 agent 声明探测目录/文件）：灵活但过度设计，当前三大主流 + 开源标准已覆盖主流场景，其余 agent 用户可用 `--agents` 显式覆盖。
- 绑定 profile 时不刷新、只靠注册时探测：旧项目（已注册为全量）痛点不解，用户观察到的「绑个 profile 建一堆目录」仍在。

**后续提醒**：`detect_agents` 探测表当前固定（claude/codex/cursor/agents），若未来出现新的主流 agent 私有目录/指令文件，在 `detect.rs` 表里加一行即可。

## 决策 20：`.agents/skills/` 总是作为项目 local 落地目录（绑定即落地）

**背景**：决策 19 把落地目录收敛为「探测到的 agent 各自目录，未命中才回退 `.agents/`」，实际用出两类问题：① 项目只有 `.claude` 时 skill 只落 `.claude/skills/`，不直读 `.claude` 的 agent（cursor/codex/opencode 等）在 `.agents/skills/` 里看不到绑定结果；② 旧项目 toml `agents = []` 时 apply 静默零落地——有绑定记录但项目里没有任何 skill，也无警告。

**决策**：
- 项目 local skill 的落地目标固定为「开源标准 `agents` 总在列 + 探测到的 agent 中 `reads_agents_dir=false` 的额外落自己目录」。默认配置下后者仅 claude-code（Claude 不直读 `.agents`，决策 5）：`.agents/skills/<skill>` 必落（copy + `.skillkit-sha` 标记），探测到 claude-code 再加 `.claude/skills/<skill>`（symlink）；cursor/codex 直读 `.agents/skills/`，不再各自 copy 一份私有目录。
- 探测结果仍写入 `proj.agents`（展示与 shared 扫描用），落地集合由 `apply::landing_agents(config, proj.agents)` 派生；`agents = []` 也保底落 `.agents/`，结构性杜绝「有绑定记录无落地」。
- 落地前 `create_dir_all` 按需创建 `.<agent>/skills/`（含 `.agents/`）；旧版落在 `.cursor/`/`.codex/` 的 skillkit local 在下次 apply 按 extra 清理。
- 修订决策 19 第 4 条：`.agents/skills/` 从「探测未命中才参与 apply」升级为「总是参与 apply」。

**理由**：`.agents/skills/` 是各 agent 通用的开源标准目录（Codex/Gemini/OpenCode/Cursor 都直读），总是落它让「绑定 profile」的结果对全部 agent 立即可见；Claude 是唯一不直读它的主流 agent，用 `reads_agents_dir=false` 的能力声明桥接，保持「新增 agent 只改配置不改代码」的硬约束。

**否定的备选**：
- 探测到的 agent 各自目录 ∪ `.agents/` 全落：cursor/codex 项目会同时存 `.cursor/skills/` copy 和 `.agents/skills/` copy 两份，冗余且重新制造决策 19 要解决的目录噪音。
- 绑定时为每个 config 声明的 agent 建目录：回到决策 19 否定过的「给未使用的 agent 建目录」。

## 决策 21：npx skills update 对 local path 源 skill 静默 no-op（实测约束留档）

（2026-07-29 ｜ 来源：`docs/sessions/archive/2026-07-29-skillkit-design.md` §3.1）

**背景**：M3 实测发现 local path 源的 skill 无 `skillPath`（lock 里 source 直接是路径），`npx skills update` 的 `updatable` 过滤只认有 `skillPath` 的远程源，local 的被归入 legacy 打印「No installed skills found matching」后跳过。

**决策**：该行为留档为已知限制：skillkit upgrade 对 local 源 skill 流程跑通但 hash 不变（如 `806cba88 → 806cba88`），github 源正常——是 npx skills 上游行为而非 skillkit bug，用户侧表现为「升级无效」。

**理由**：上游能力边界，skillkit 侧不可修复，只能记录；不属 skillkit 的取舍，属实测约束留档（此前 TODO 与主 spec 均未记）。

## 决策 22：project-id 用 uuid v4 前 8 hex 随机短码，注册后冻结

（2026-07-30 ｜ 来源：`docs/superpowers/plans/2026-07-30-skillkit-m1.md` Task 3 设计决策段）

**决策**：project-id = uuid v4 前 8 hex 大写随机短码，独立于 path，注册后冻结；`rebind` 只更新 path/name 而 id 不变。取代主 spec §6.4「目录名-绝对路径 SHA256 前 6 位」的路径派生设想。

**理由**：id 独立于 path，项目移动/改名后 rebind 才能保 id 续用。

**否定的备选**：路径派生稳定 id（spec §6.4 原案）——路径一变 id 即变，幂等注册的价值不如 rebind 显式语义。

## 决策 23：前端改 htmx 2.x + Askama + SortableJS，砍独立前端工程

（2026-07-31 ｜ 来源：`docs/superpowers/specs/2026-07-31-skillkit-m2-design.md` §2/§12，自述取代主 spec §12 的 React+Vite 方案）

**决策**：前端技术栈从 React+Vite 改为 htmx 2.x + Askama 服务端渲染片段 + SortableJS：模板编译进二进制、静态资源 rust-embed 嵌入不走 CDN（离线可用）、无 node 无构建步骤，砍掉独立前端工程。修订决策 2 的前端部分。

**理由**：保持单二进制零运行时依赖。

**否定的备选**：React+Vite 独立前端工程（决策 2 原案）——node 构建链与分发复杂度不划算。

## 决策 24：SSE 唯一用途 = 跨进程文件变化刷新，apply 同步返回不走 SSE

（2026-07-31 ｜ 来源：`docs/superpowers/specs/2026-07-31-skillkit-m2-design.md` §7/§12.2）

**决策**：SSE 唯一用途定为跨进程文件变化刷新（notify 递归监听 `~/.skillkit/` 状态文件，只认 registry/sources/profiles/projects 四类）；apply 触发即同步返回结果片段，不走 SSE。细化决策 3 的用途边界。

**理由**：apply 是 core 同步一次跑完，无细粒度进度可推；async 任务 + 推送需任务表/状态机，属过度设计；不监听 `~/.agents/skills/`（决策 5 硬约束）。

**否定的备选**：apply 异步任务 + SSE 推进度。

## 决策 25：文件锁原语——fs2 + per-key 锁文件 + 5s 超时

（2026-07-31 ｜ 来源：`docs/superpowers/specs/2026-07-31-skillkit-m2-design.md` §8）

**决策**：fs2 advisory flock；per-key 粒度（每状态文件一把锁，`~/.skillkit/.lock/<key>.lock`）；写锁默认 5s 超时，冲突报错不死等；读操作不抢锁；CLI/server 共用 core 同一锁。落实决策 3「并发写配置用文件锁」的总则。

**理由**：本地单用户工具，5s 冲突即报错比长等待重试更诚实。

**否定的备选**：
- 锁目标状态文件本身——改为独立 `.lock/<key>.lock` 文件，避免写坏状态文件。
- 长等待重试至成功——对本地单用户工具不如直接报错。

## 决策 26：serve 安全模型——只绑 127.0.0.1 + 随机 token 入 URL 路径

（2026-07-31 ｜ 来源：`docs/superpowers/specs/2026-07-31-skillkit-m2-design.md` §9/§13）

**决策**：只绑 127.0.0.1，随机 token 入 URL 路径前缀 `/{token}/`（token 不符返回 404，不泄露路由存在性），不做登录/多用户/远程访问。

**理由**：本地单用户工具，token 防本机其他进程误访问即够。

**否定的备选**：登录体系/多用户/远程访问——均属过度设计。

## 决策 27：unmanaged skill 数据模型——虚拟源、不可升级、单一 registry

（2026-07-31 ｜ 来源：`docs/superpowers/specs/2026-07-31-skillkit-m3-design.md` §2/§3）

**决策**：存量无源 skill 用虚拟源 `unmanaged`、`computed_hash=None`、scope=global、不可升级（upgrade 报「无版本锁」跳过），与 managed 混在同一 registry 单存储；import 溯源只认 `.git` remote url 一种形态（可溯源必须重装才成 managed），不猜其他。

**理由**：存量目录无法溯源时「可见但不可升级」是诚实的中间态；现实中绝大部分走 unmanaged（全局无 skills-lock.json）；深度溯源 YAGNI。

## 决策 28：upgrade 冲突检测定位为「人类交互确认」而非锁版本

（2026-07-31 ｜ 来源：`docs/superpowers/specs/2026-07-31-skillkit-m3-design.md` §4.2）

**背景**：决策 9 已定「升级扫描 project 锁、冲突警告并列受影响项目」；本条细化 core/CLI 分层——core 不读 stdin、不自动改 `locked_shas`。

**决策**：core 返回 `UpgradeBlocked{id, affected}` 不读 stdin，CLI 层 y/n 确认（`--yes` 跳过）；升级后 `locked_shas` 不自动改、不自动 apply 受影响项目（单一职责：基线更新归 project apply）。

**理由**：单版本模型下 `locked_shas` 锁不住物理版本，冲突检测的价值是让用户知道升级连带影响 N 个项目基线。

**否定的备选**：
- core 读 stdin 交互——core 层保持无交互。
- 升级自动改 locked_shas / 自动 apply——基线更新职责归 project apply。

## 决策 29：CLI uninstall 完全替换为 remove，不保留别名

（2026-08-01 ｜ 来源：`docs/superpowers/specs/2026-08-01-skill-find-list-remove-design.md` §3/§2.3）

**决策**：uninstall 完全替换为 remove，不保留别名（有意的破坏性变更）；remove 补交互确认（默认 y/n，`--yes`/`--json` 隐含跳过）。

**理由**：项目未发布无外部用户，保留隐藏别名徒增维护面；原 uninstall 直接删无确认违反 CLI 约定；与 source 的 remove 命令对称。

**否定的备选**：保留 uninstall 作为别名。

## 决策 30：Projects 交互重构三不变量（installed_skills 自动推导）

（2026-08-01 ｜ 来源：`docs/superpowers/specs/2026-08-01-projects-ui-redesign-design.md` §2）

**决策**：
- installed_skills 由绑定 profiles 的并集自动推导（`set_profiles` 全量替换语义 + 清孤儿 locked_shas 跳 global），删除手动勾选表单与 update/APPLY 端点。
- 「应用」一步完成保存绑定 + 重算 + run_apply 落地。
- 删除项目只注销（删 `~/.skillkit/projects/<id>.toml`），不碰项目目录任何文件。

修订决策 7「用户必须显式选择」的半边。

**理由**：①消除 applied_profiles 与 installed_skills 脱节的根因；②声明与落地是技术分层，不应让用户感知；③项目目录是用户领地，已落地 symlink 指向 canonical 无害。

**否定的备选**：
- 保留手动勾选——双事实源必然脱节。
- 删除项目连带清理落地文件——越界进用户领地。

## 决策 31：server 同步阻塞调用一律 spawn_blocking；find 同步请求 + loading 指示

（2026-08-01 ｜ 来源：`docs/review/2026-08-01-gui-parity-design-spec-review.md` §2-P2，修正 gui-parity spec §4 并扩为全路由惯例；spec 原文 `docs/superpowers/specs/2026-08-01-gui-parity-design.md` §4）

**决策**：handler 调同步阻塞 core（Command::output、扫描迁池、批量 npx）一律 spawn_blocking 卸载到 blocking 线程池；GUI find 选同步 htmx 请求 + hx-indicator loading。

**理由**：async 里直调同步 Command 阻塞的是 tokio 工作线程（默认 = CPU 核数），spawn_blocking 才走 blocking 池；npx find 典型 2-5 秒，非交互场景不会卡死，任务表/状态机是过度设计。

**否定的备选**：
- 异步任务 + SSE 推送。
- async 直调同步 Command——占用 tokio 工作线程。

## 决策 32：upgrade_all 返回 UpgradeAllReport，冲突进 blocked 不中断

（2026-08-01 ｜ 来源：`docs/superpowers/plans/2026-08-01-gui-parity.md` Task 5；`docs/review/2026-08-01-gui-parity-design-spec-review.md` §2-P1）

**决策**：upgrade_all 返回 `UpgradeAllReport{upgraded, blocked}`——冲突 skill 显式列进 blocked，不升级也不中断，而非原设计的 `Vec<UpgradeReport>` + 冲突 warn 静默跳过；GUI 批量升级调 `upgrade_all(paths, false)`（冲突进 blocked 列出），单 skill 升级才传 yes=true（用户已显式点击）。

**理由**：yes=true 时 core 永不返回 UpgradeBlocked，冲突 skill 会被静默自动升级，锁定 oldhash 的项目基线漂移而零反馈，违背「列出不拦截」；与 CLI `--all` 默认语义对齐。

**否定的备选**：
- GUI 批量也传 yes=true——冲突静默升级。
- 冲突 warn 静默跳过。

## 决策 33：路径输入交互选混合形态 + server 端目录浏览/补全

（2026-08-01 ｜ 来源：`docs/superpowers/specs/2026-08-01-project-path-picker-design.md` §1/§4；`docs/superpowers/specs/2026-08-03-projects-browse-floating-panel-design.md` §3/§4）

**决策**：输入框 + 浏览按钮的混合形态，配 server 端目录浏览（browse 端点）与 Tab 前缀补全（complete 端点），不用浏览器原生文件选择/跨平台 native 对话框。

**理由**：浏览器安全沙箱不暴露完整本地绝对路径（webkitdirectory 只给目录名）是硬约束，skillkit 需要绝对路径只能 server 端列目录；纯逐级浏览不能快速粘贴已知路径，纯补全仍以输入为主，混合两者兼得。

**否定的备选**：
- 浏览器原生文件选择——拿不到绝对路径。
- 跨平台 native 对话框——依赖重。

## 决策 34：skill→profile 反向索引现算不缓存，批量归入走 load-循环-save

（2026-08-04 ｜ 来源：`docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md` §2.2/§3.2/§9；`docs/review/2026-08-04-skill-scope-profile-design-spec-review.md` §2-P0）

**决策**：
- skill→profile 反向索引（skill_profiles）扫描现算不缓存，不在 SkillMeta 加归属字段，保持 profile→skill 单向持有。
- `profile.add_skill` 保持非幂等（重复报 SkillAlreadyInstalled，CLI 既有报错语义不变）。
- 批量归入 profile 的原子模式 = handler load 一次 + 内存循环 add_skill（SkillAlreadyInstalled 静默跳过、scope 错误抛出不落盘）+ 一次 save，不新增 core 批量函数。

**理由**：profile 数量小，现算够用；把 add_skill 改真幂等会静默改变 CLI `profile add-skill` 既有报错语义；原子性来自 load/save 边界而非 add_skill 幂等。

**否定的备选**：
- 反向索引缓存——失效逻辑复杂。
- add_skill 改真幂等——静默改变 CLI 报错语义。
- 批量循环每轮独立落盘——部分失败不可接受。

## 决策 35：Skills 视图状态全走 URL query + 服务端重渲染

（2026-08-04 ｜ 来源：`docs/superpowers/specs/2026-08-04-skill-scope-profile-design.md` §5.2/§5.3）

**决策**：选中态与 profile 过滤全走 URL query + 服务端重渲染（SSE 刷新带当前 query），不走客户端 JS 状态；Profiles 视图退化为创建/查看/排序（删手填 source/skill 表单，加 skill 移到 Skills 视图批量归入）。机制后经 75db3da 演进为：chip 点击在 htmx:configRequest 里 pushState 写地址栏，写操作经 HX-Current-URL 头还原过滤视图。

**理由**：与 SSE 重渲染主模式统一——所有 UI 状态在 URL + 服务端，SSE/写操作重渲染无损。

**否定的备选**：
- 客户端 JS 过滤——状态双份会漂移。
- 每行调 skill_profiles 现扫——handler 一次遍历建反向 map 即可。

## 决策 36：存量 global 引用不做自动迁移

（2026-08-04 ｜ 来源：`docs/review/2026-08-04-skill-scope-profile-design-spec-review.md` §8.1 P2-3，落地为 spec「存量 global 引用策略」小节）

**决策**：存量 profile/project 里的 global 引用数据保留、不做自动迁移——GUI 惰性过滤、set_profiles 按 registry 跳过、apply 幂等忽略 global，legacy 引用不清。

**理由**：决策 17 的互斥是加在 add_skill/set_profiles 的未来硬约束；存量 global 引用清退成本高，且 apply 本就幂等忽略。

**否定的备选**：自动迁移工具——YAGNI。

## 决策 37：install local 用伪 source `local`

（2026-08-07 ｜ 来源：`docs/superpowers/specs/2026-08-07-install-local-skill-design.md` §3.1/§9）

**决策**：本地安装用伪 source `local`（不进 SourcesStore），id = `local/<name>`，与 unmanaged 伪源对称；`local` 作为保留名，未来自定义 source 名需避让。

**否定的备选**：
- 注册成 SourcesStore 真 source——带 package 字段语义别扭。
- 用路径/文件名做 source——id 不稳定。

## 决策 38：install local 把外部输入当不可信信任边界，主动设防

（2026-08-07 ｜ 来源：`docs/superpowers/specs/2026-08-07-install-local-skill-design.md` §3.4-3.6/§9 审查加固；`docs/superpowers/reviews/2026-08-07-install-local-skill-design-review.md` F1/F3/F4）

**决策**：GitHub zip / 外部目录视为不可信输入，安全五件套：name 校验（拒 `../`、纯点、分隔符）；containment 断言（target 必在池内，防 remove_dir_all 删光池）；zip 逐条目 enclosed_name 防 ZipSlip、拒 symlink 条目、体积 100MiB/条目 10000 上限防 zip bomb；computed_hash 用 sha256 长度前缀框架（len‖path‖len‖content）防树结构碰撞。

**理由**：codex 对抗审查驱动——原设计正则会放行 name='..'（--force 可删掉整个 canonical 池）；无定界拼接可构造 `{a:"bc"}` vs `{ab:"c"}` 碰撞（hash 是漂移检测基础，碰撞即静默用旧内容）；zip symlink 可把 ~/.ssh 拷入池外泄。

## 决策 39：install_local 全程持锁 + 暂存目录 + 三段原子 + 冲突键 = registry id

（2026-08-07 ｜ 来源：`docs/superpowers/specs/2026-08-07-install-local-skill-design.md` §3.2/§3.7；`docs/superpowers/reviews/2026-08-07-install-local-skill-design-review.md` F2/F5 及二轮 F2/F3/F4）

**决策**：全程持 registry 锁（save 用不重复加锁变体 save_raw，防同进程 flock 自死锁）；复制走暂存目录；force 用三段原子 move-aside（target→.old→staging→target→rm .old，删除永不先于新内容就位）；冲突键 = registry id 而非目录名（force 前按 canonical_path 归属反查，不跨 source 误删；占位无主时 SkillPoolOccupied.owner_id=None 给手动清理指引）。

**理由**：复审发现首版「先删后 rename」非原子（窗口内崩溃留半删 canonical，致全量漂移）；池按 name 扁平，`skills.sh/foo` 与 `local/foo` 共用目录，按目录名判冲突 + force 删会误删他源 canonical；stale 记录可绕过 target.exists()。

**否定的备选**：
- 先删后 rename——非原子。
- 仅按池目录名判冲突——跨 source 误删。

## 决策 40：写操作 summary 反馈与 SSE 刷新的时序取舍

（2026-08-10 ｜ 来源：`docs/sessions/2026-08-10-skillkit.md` §2.5/§3.5，落地 commit f4e7dec 2026-08-07）

**决策**：SSE 刷新延迟 300ms + summary 展示期跳过刷新 + summary 4s 自动淡出；代价是写操作后 4s 内 CLI 侧数据变化不实时同步到浏览器。

**理由**：既有 bug（SSE 刷新冲掉写操作 summary 反馈）的修法选型，4s 盲区对单用户本地工具可接受。

**否定的备选**：不修——牺牲 summary 反馈，违背反馈引导行动。

## 决策 41：GUI 安装本地 skill = Modal 三合一 + multipart 上传

（2026-08-10 ｜ 来源：`docs/superpowers/specs/2026-08-10-install-local-ui-design.md` §3/§4）

**决策**：Modal 浮层三合一（拖放 .zip/目录 + 原生文件选择 + 路径输入兜底），引入 multipart 上传端点（axum multipart feature + 逐文件重建目录树 + tempfile 临时区 + 100MiB body limit），core/CLI 零改动——推翻 08-07 spec「不做 HTTP 文件上传」的 YAGNI 边界。

**理由**：拖放/文件向导是明确诉求；浏览器拖放拿不到绝对路径是根本前提，multipart 是目录上传唯一途径（不引前端 zip 库）。

**否定的备选**：
- 行内展开——层次弱，推挤表格。
- 极简拖放——失去路径兜底。
- 维持 08-07 的路径输入单形态。

## 决策 42：import 的 unmanaged 默认迁入 canonical 池 + relink 幂等补迁

（2026-08-14 ｜ 来源：`docs/superpowers/specs/2026-08-14-import-relocate-design.md` §3/§9）

**决策**：unmanaged 从「只登记」改为默认迁入 canonical 池 + 原位 symlink 桥接；历史存量靠 relink_unmanaged 按 registry 遍历幂等补迁；不新增独立 adopt 命令；迁移用同文件系统 rename（原子零拷贝），不走 install_local 暂存三段；池子已有同名时删原位置副本（池子权威），不引入 force 开关、不报错。

**理由**：统一 managed/unmanaged 物理模型（单池子 + 双层桥接）。

**否定的备选**：
- 独立 skill adopt 命令——多一步，且存量仍需批量入口。
- 要求用户删 registry 记录重 import——破坏性丢元数据。
- copy + delete——迁的是可信文件，无需暂存。
- 报错让用户选——副本本是冗余，报错是无谓摩擦。

## 决策 43：桥接遇真实目录占位——降级为 skipped 点名，不整批中断

（2026-08-14 ｜ 来源：`docs/superpowers/specs/2026-08-14-import-relocate-design.md` §6/§9（2026-08-14 修订）；`docs/review/2026-08-14-import-relocate-design-spec-review-r2.md` §2-P2-3）

**决策**：不放松 ensure_link 承重墙（不静默删占位），但从「import 整批中断报错」降级为该 skill 进 skipped 点名（含原因与清理引导），其余继续——推翻 r2/r3 评审当时确认的「CanonicalCreate 整批中断」设计。

**理由**：实测 Claude CLI 自装 skill 即可触发占位，一个占位废掉整次导入且 GUI 只显示「导入失败」无原因，代价过高；降级后 registry 已一致（canonical 指池），清占位重跑自动收敛，中断不提供额外安全性；仍否决 force 绕过承重墙静默删占位目录。

## 决策 44：relink 有意信任 registry.canonical_path 为遍历源

（2026-08-14 ｜ 来源：`docs/superpowers/specs/2026-08-14-import-relocate-design.md` §3.3/§6；`docs/review/2026-08-14-import-relocate-design-spec-review-r2.md` §2-P3）

**决策**：relink 有意信任 registry.canonical_path 为遍历源（与 scope.rs 不信任 canonical、扫物理位置找回构成有意差异），换取「canonical 被挪到 4 个扫描目录之外也能覆盖」的全面性。代价：dangling 只能 warn 跳过；canonical 指向存在真实目录时按当前指向做破坏性 rename（不可逆，源目录消失）。风险声明后 YAGNI，不额外防护。

**理由**：canonical_path 由 skillkit 自管（import/adopt/rescope 写入），漂移到无关真实目录需用户手编 registry.json，风险低、防护成本大于收益；漂移 orphan 留手工清理。

## 决策 45：agent 别名目录豁免——apply/status 清理按物理路径判定

（2026-08-20 ｜ 来源：PR #2（ea64933 2026-08-20，merge 08a1e03 2026-08-21，外部贡献，无 spec））

**决策**：项目内 `.claude/skills`、`.cursor/skills` 等互为 symlink 指向同一物理目录时，apply/status 清理按 canonicalize 后物理路径判定豁免，别名不互相清理；status 与 apply 共用 scan_extras 保证判定一致。spec §10.2 未覆盖的能力边界，外部贡献发现。

**理由**：同一物理目录经多个 agent 别名挂载时，按路径字符串清理会误删 expected 目标——外部 PR #2 发现的真实边界，无内部备选之争（修法即按物理路径判定）。

## 决策 46：registry 写方锁 with_registry 单点收敛（两处有意例外）

（2026-08-26 ｜ 来源：`docs/sessions/2026-08-26-code-cleanup.md` §2.2；`docs/superpowers/reviews/2026-08-07-install-local-skill-design-review.md` 二轮 F2，commits 9ffd194/8385638）

**决策**：registry 写方锁覆盖 load→save 全程：提取 with_registry 单点收敛持锁写 registry 事务（acquire→load→闭包→save_raw；闭包内禁止再 Registry::load / FileLock::acquire，防同进程 flock 自死锁）。install_local 与 set_scope 两处有意不收敛——install_local 需锁窗口覆盖解压/复制/原子就位全流程；set_scope 有 same-scope 锁内早退，锁内 match 分支收敛反而复杂化（且有 rescope_survives_concurrent_registry_writer 并发回归测试守护）。

**理由**：二轮评审指出锁外 check-then-act 会 lost-update（并发 save 用陈旧快照覆盖，回滚并发写方写入）；对两处特殊锁窗口强行全收敛反而复杂化。

**否定的备选**：全部统一收敛到 with_registry。

## 决策 47：CLI project list 容错——跳过坏 toml，不整命令报错

（2026-08-26 ｜ 来源：`docs/sessions/2026-08-26-code-cleanup.md` §3，commit c7acdc5）

**决策**：从「单个坏 toml 使整命令报错退出」改为「跳过该文件 + tracing warn」（load_all_projects 收敛进 core，与 server 行为对齐）；若要 CLI 恢复严格报错，把 load_all_projects 换回手动循环即可。

**理由**：列表不应因一个坏文件全挂；与 server 侧行为对齐，避免两端口径不一。代价：CLI 失去严格报错能力（可一行换回）。

## 决策 48：registry schema 版本化 + 启动自动迁移（确定性回填）

（2026-09-10 ｜ 来源：commit a4ece85，无独立 spec 文档，决策仅存于代码注释）

**决策**：顶层 version 字段逐级升级；回填只用本地 skills-lock.json 的确定性证据（查不到留空，不猜不联网）；失败 warn 不阻塞（旧 schema 仍可读）。新增 spec 字段记录 skills.sh 原始安装名，但实测 `npx skills update` 只认短名，spec 不参与升级调用。

**理由**：spec 安装名与升级短名二分的实测约束；schema 演进需要确定性原则。

**否定的备选**：
- 迁移失败阻塞启动。
- 联网/猜测回填。

## 决策 49：安装向导占用冲突可视化 + 覆盖安装 force 语义

（2026-09-10 ｜ 来源：commits ec17791/4e2efb6/2aaa609，无独立 spec，交互决策存于 commit 与代码注释）

**决策**：find 候选行标出同短名占用者（registry 记录 / 未登记孤儿目录两类），占用时按钮切「覆盖 local/global」（表单 force=1）；core install() 增 force 参数——先复用 uninstall 清场（managed 撤桥接/摘记录/同步 lock）再正常安装，孤儿目录补删，覆盖清场不走回收站；安装失败错误透传具体原因，不笼统提示网络；候选行改 local/global 一键安装。

**理由**：占用冲突从「报错让用户自己清」升级为「界面内可见占用者 + 一键覆盖」；覆盖清场硬删是因覆盖紧跟替换，回收站无意义且避免测试误触真实废纸篓。

**否定的备选**：
- SkillAlreadyInstalled 报错让用户手动清理。
- 笼统网络错误提示。

## 决策 50：卸载一律进系统回收站（含 unmanaged）

（2026-09-11 ｜ 来源：`docs/superpowers/specs/2026-08-14-import-relocate-design.md` §6 原取舍被推翻，commit cb458a3；`docs/review/2026-08-14-import-relocate-design-spec-review.md` §3 对照现状）

**决策**：卸载一律进系统回收站（含 unmanaged），推翻 import-relocate spec §6「unmanaged 只摘登记不删 canonical」的原取舍；canonical 在池外的手工目录仍保留仅摘记录（防误删）；force 覆盖安装的清场例外硬删（覆盖语义紧跟替换，无需回收站）。

**理由**：原取舍实际用出「孤儿目录 + dangling 桥接残留」痛点（TODO 四-1 记录）；系统回收站可逆，误删可从废纸篓找回；池外手工目录非 skillkit 所有，不删。

**否定的备选**：
- 硬删——不可逆。
- 留孤儿——残留桥接。

## 决策 51：registry 同名重复登记双向收敛 + dupes 人工裁决三件套

（2026-09-11 ｜ 来源：`docs/superpowers/specs/2026-08-14-import-relocate-design.md` §6（2026-09-11 修订，r2/r3 评审后）；commits d0288e2/4b74511）

**决策**：
- install 堵源：with_registry upsert 前查同名且 canonical 在池的他人记录，命中报 SkillPoolOccupied 点名占用方。
- import dedupe_same_name 幂等收敛：仅「全组同池同路径且恰一条 managed」才摘多余 unmanaged 记录，物理歧义留人工点名。
- `skillkit dupes list/trash/adopt` 人工裁决，不自动删副本：trash 删至系统回收站可逆，并幂等补建 global 桥接；adopt 仅限全 unmanaged 且 canonical 一致在池，managed 正主拒绝（覆盖会破坏升级语义）；owner canonical 不一致时保守返回 None，不自动对比。

**理由**：副本与正主内容可能有实质差异（外部工具写回/手工拷贝产物），自动 adopt 意味着盲删丢数据；收敛只做零物理风险的「摘记录」，物理歧义留给人工。

**否定的备选**：
- import 按名自动去重。
- 自动 adopt。

## 决策 52：install_local force 对齐源安装清场语义（clear_occupied 共用）

（2026-11-03 ｜ 来源：GUI「覆盖已存在」装本地 skill 被 `unmanaged/<name>` 占用拒绝的实机反馈；局部修订决策 39）

**决策**：install_local 的 force=true 在取 registry 锁**之前**调 clear_occupied 清场（摘全部同名记录 + 删池内 target，跨 source 占用与孤儿目录一并处理），再持锁做冲突判定（清场后仍命中即并发写入抢先，拒绝不二次清场）与单段 rename 就位。局部修订决策 39：撤销「force 也不跨 source 删」与三段原子 move-aside（清场后 target 必不存在，三段退化为死代码删除）；「全程持锁」收窄为「就位与 registry 写持锁」，src 解析与清场在锁外（clear_occupied 内部经 with_registry 持短锁，锁内调用会同进程 flock 自死锁）。

**理由**：决策 49 只给源安装做了 force 清场，本地安装的 force 仅绕过同 id 重复检查，跨 source 占用仍无条件拒绝——GUI「覆盖已存在」与错误文案「或 --force 覆盖」的承诺双双落空，用户被迫去 CLI 清记录，体验断层；错误文案对本地路径不成立属文案与行为不一致缺陷。

**否定的备选**：
- 维持拒绝 + 文案改为引导 remove——保留两条路径 force 语义分裂。
- 锁内调 clear_occupied——flock 同进程不重入，必然 LockTimeout。
