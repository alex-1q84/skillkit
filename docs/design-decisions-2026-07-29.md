# SkillKit 设计决策纪要

- 日期：2026-07-29
- 性质：设计讨论的决策推理记录，记录"为什么这么选"及被否定的备选方案。最终设计见 `2026-07-29-skillkit-design.md`。

## 决策 1：独立实现核心，npx skills 只做下载

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

**理由**：与现状（opencli-* 的 symlink 模式）一致，零迁移摩擦；单版本 + 元数据锁已能满足"记录和锁定版本"（记录 commit_sha，升级时校验）。skill 是指令集，版本兼容性问题比软件库轻，多版本并存是 YAGNI。

**否定的备选**：
- 多版本并存（canonical 按版本分目录 `~/.skm/skills/<skill>/<version>/`）：支持同 skill 多版本并行，但占空间、元数据和升级逻辑复杂，当前无此需求。预留升级路径，未来需要时不破坏现有抽象。

## 决策 5：`~/.agents/skills/` 只放全局公共 skill

**背景**：发现 `~/.agents/skills/` 是通用 AI agent 加载目录，Cursor、OpenCode、Codex、Gemini 等除 Claude 外的大部分 agent 都直接从此目录加载。

**决策**：`~/.agents/skills/` 专属全局公共 skill，绝不挪用为项目级暂存，元数据也不放进去。所有 skillkit 元数据统一收 `~/.skm/`。

**理由**：挪用通用加载目录会污染其他 agent 的 skill 视图，混淆 local/shared 边界。把全局公共 canonical 选在 `~/.agents/skills/` 本身就让 Cursor 等零配置可用（直接读），只有 Claude 需要 symlink 桥接（Claude 不直接读 .agents）。这一约束也促使项目 local canonical 从"每项目各放一份"改为集中到 `~/.skm/skills/`。

## 决策 6：项目 skill 分 local / shared 两类，shared 不由 skillkit 管

**背景**：项目里有的 skill 要入仓库随团队分发，有的要共享但不入仓库。

**决策**：
- local（不入库）：canonical 集中在 `~/.skm/skills/`，与 shared 同级平铺落地到 `<project>/<agent>/skills/<skill>/`（symlink 或 copy），git 忽略走 `<project>/.git/info/exclude`（本地不入库）。
- shared（入库）：真实文件直接在 `<project>/<agent>/skills/`，git 提交，skillkit 只做只读发现，不安装/升级/卸载。

**理由**：shared skill 既然在 git 里，项目自身（git + 团队约定）已经在管理它，skillkit 重复管是多余，违反最小改动和 YAGNI。skillkit 对 shared 只需能看到清单，方便与 local 对照展示。

**演进**：早期设想用 `.skm/shared.lock` 锁文件管理 shared 的版本，明确放弃——shared 由 git 管，不需要第二个版本管理器。

## 决策 7：registry 用 id 引用，profile 退成粗分类，project 做精确选择

**背景**：profile 和 project 的职责需要厘清，且 source/scope 等信息不该在多处重复。

**决策**：
- registry 给每个 skill 一个 id（`<source>/<skill-name>`），作为跨实体的唯一引用。
- profile 只存 id 列表，是"这类场景可能用到的 skill 候选集"（粗分类）。
- project 的 `installed_skills` 是 apply 的唯一事实依据，精确到每个 skill，是所应用 profile 候选集的子集选择。

**理由**：id 引用消除冗余（DRY）。profile 是粗分类 + 批量操作入口，project 是精确事实，职责分明。profile 新增 skill 不会自动装到项目，用户必须显式选择，符合"精确控制"。

## 决策 8：按 agent 能力选落地策略

**决策**：在 `~/.skm/config.toml` 声明每个 agent 的能力（是否支持 symlink、是否直读 `~/.agents/skills/`），apply 时据此选落地方式。Claude 用 symlink，Cursor 不支持 symlink 用 copy 兜底，OpenCode/Codex/Gemini 全局层面直读无需操作。

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
