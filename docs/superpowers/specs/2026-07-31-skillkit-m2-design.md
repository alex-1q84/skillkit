# SkillKit M2 GUI 设计（htmx 路线）

- 日期：2026-07-31
- 状态：待评审
- 依赖：M1 已完成（profile/project/apply 闭环，35 tests 全绿）
- 关系：本 spec 取代 `docs/2026-07-29-skillkit-design.md` §12 的 React+Vite 方案，改 htmx 路线。原 §12 的技术细节段在 M2 spec 评审通过后同步更新。

## 1. 目的与范围

M2 给 skillkit 加本地 web GUI（`skillkit serve`），让主人总览配置 + 可视化操作四大视图 + Projects apply 闭环。技术栈 htmx + Askama + SortableJS，不用 React/Vue，保持单二进制零运行时依赖。

在范围：
- `crates/server` 新 crate（Axum 薄壳调 core）。
- htmx 前端（Askama 模板 + 静态 JS/CSS，无独立前端工程）。
- SSE（跨进程文件变化刷新）。
- core 文件锁原语（CLI/server 并发写保护，spec §13 落地）。

不在范围（M3 或更后）：
- `import-existing` 扫描导入现有 skill。
- `install upgrade` / `install list`。
- 打包 mac-config Brewfile。
- apply 细粒度 SSE 进度（见 §7）。

## 2. 技术栈定调

- 后端：Axum + Askama（编译期类型安全模板）+ rust-embed（静态资源嵌入）+ notify（文件监听）+ tokio。
- 前端：htmx 2.x + SortableJS（拖拽，仅 profile 组装用）+ 自写 CSS。无 node、无构建步骤、无独立前端工程。
- 模板编译进二进制；静态资源（htmx.min.js / sortable.min.js / app.css）经 rust-embed 嵌入。不走 CDN（本地工具离线可用）。
- 视觉沿用 `demo/index.html` 的亮色风格，产品化到 app.css。
- 单二进制、零运行时依赖（与 M0/M1 定位一致）。

## 3. 架构（三层不变，core 业务逻辑零改动）

- core：复用现有全部业务逻辑。M2 唯一新增是文件锁原语（§8），属基础设施，不改业务语义。
- server：新 crate，Axum 薄壳。每个 HTTP handler = `load → core 方法 → save → 渲染 Askama 片段或全页`。
- 前端：htmx 驱动。浏览器只发 `hx-get` / `hx-post` / `hx-delete`，后端返回 HTML 片段，htmx 局部替换。不返回 JSON、不做前端状态同步。

core 已暴露 server 所需的全部方法（已核实 CLI 现有薄壳模式）：
- `Project::{load, save, register, add_skill, remove_skill, apply_profile}`
- `Profile::{load, save, add_skill, remove_skill, list_names}`
- `SourcesStore::{load, add, remove, list}`
- `Registry::{load, get, remove}`、`Scope`/`SkillMeta`
- `apply::{compute_diff, run_apply, build_status}`、`ApplyDiff`/`ApplyReport`/`StatusView`
- `install::{install, uninstall}`
- `Config::{load, find_agent}`、`Paths::production`

CLI 现在就是 `load → core 方法 → save → println/--json`；server 把「println」换成「渲染片段」即可。

## 4. server crate 结构

```
crates/server/
  Cargo.toml            # axum, askama, rust-embed, tokio, notify, serde, uuid, skillkit-core
  src/
    lib.rs              # serve()：bind 127.0.0.1 + 生成随机 token + router 装配
    routes/
      mod.rs            # router + token 校验中间件
      home.rs           # GET / → 重定向 Projects
      sources.rs        # 源视图 + 增删（返回片段）
      skills.rs         # registry 总览 + install/uninstall
      profiles.rs       # profile 列表 + add/remove/reorder
      projects.rs       # 项目列表 + 工作台 + apply 闭环 + status
      sse.rs            # GET /events SSE 流
    templates/          # Askama 模板（.html，编译期类型检查）
      layout.html       # 外壳：<head> + nav + htmx/sortable 引用 + 出参 block
      sources.html / skills.html / profiles.html / projects.html
      fragments/*.html  # hx 局部替换片段（源行、skill 行、status diff 等）
  static/               # rust-embed 嵌入
    htmx.min.js, sortable.min.js, app.css
```

## 5. 路由设计（htmx 视角）

所有路由在 `/t/<token>/` 前缀下（token 鉴权见 §9）。

| 方法 + 路径 | 作用 | 返回 |
|---|---|---|
| `GET /t/<token>/` | 重定向 Projects | 302 |
| `GET /t/<token>/sources` | Sources 视图全页 | HTML |
| `POST /t/<token>/sources` | 添加源 | 源列表片段 |
| `DELETE /t/<token>/sources/:name` | 删源 | 源列表片段 |
| `GET /t/<token>/skills` | registry 总览（按 scope/source 筛选） | HTML |
| `POST /t/<token>/skills/:id/install` | install | registry 片段 |
| `DELETE /t/<token>/skills/:id` | uninstall | registry 片段 |
| `GET /t/<token>/profiles` | profile 列表 | HTML |
| `POST /t/<token>/profiles` | 创建 profile | profile 片段 |
| `POST /t/<token>/profiles/:name/skills` | 加 skill | profile 片段 |
| `DELETE /t/<token>/profiles/:name/skills/:id` | 移除 skill | profile 片段 |
| `POST /t/<token>/profiles/:name/reorder` | SortableJS 拖拽排序（onEnd 触发 hx-post） | profile 片段 |
| `GET /t/<token>/projects` | 项目列表 | HTML |
| `GET /t/<token>/projects/:id` | 项目工作台（三栏） | HTML |
| `POST /t/<token>/projects/:id/skills` | 全量替换 installed_skills（对应工作台勾选） | installed + status 片段 |
| `POST /t/<token>/projects/:id/apply-profile` | 灌入 profile | installed 片段 |
| `POST /t/<token>/projects/:id/apply` | run_apply 落地 | status diff 片段 + warnings |
| `GET /t/<token>/projects/:id/status` | status diff 片段 | HTML 片段 |
| `GET /t/<token>/events` | SSE 流 | text/event-stream |

## 6. Projects apply 闭环（核心交互）

工作台三栏，对应 `demo/index.html` 的 Projects 视图：

- installed_skills（可勾选）：checkbox 决定落地集。勾选变化 `hx-post` → 后端更新 `installed_skills` → 返回 installed 列表 + status diff 片段。
- shared（只读）：项目 git 里的 shared skill，只展示（skillkit 不管，§5/决策 6）。
- status diff：expected / missing / extra / conflicts，对应 `project status --json`。
- APPLY 按钮：`hx-post` 调 `run_apply` → 返回落地结果片段（created / removed / recopied / warnings，来自 `ApplyReport`）+ 刷新后的 status diff（落地后 expected 应全部 in-sync）。

勾选算 diff 走后端（主人已确认），localhost 延迟可忽略，不做前端 mock 计算。

## 7. SSE（简化版）

apply 是 core 同步一次跑完，没有细粒度进度可推。结论：apply 触发即同步返回结果片段，不走 SSE。

SSE 唯一用途：**跨进程文件变化刷新**。`notify` 监听 `~/.skillkit/` 下状态文件（registry.json / sources.toml / config.toml / profiles/ / projects/），任一变化——通常是主人在终端跑了 CLI 改了状态——推送 `{event:"changed", scope:"registry"|"sources"|"profiles"|"projects"}`，前端按 scope 触发对应区域 `hx-get` 局部刷新。

不监听 `~/.agents/skills/`（通用加载目录，skillkit 不在其下写元数据，§5 硬约束）。

## 8. 文件锁（spec §13 落地，纳入 M2 硬范围）

M1 还没有锁原语。M2 在 core 新增文件锁，粒度到单文件（spec §13）：

- 每个状态文件各一把锁：`registry.json`、`sources.toml`、`config.toml`、每个 `projects/<id>.toml`、每个 `profiles/<name>.toml`。
- 读操作（status / list / get）不抢锁。
- 写操作（install / apply / 声明编辑 / 源 CRUD）抢对应文件的写锁，带超时（默认 5s），超时按冲突报错（反馈引导行动），不死等。
- CLI 和 server 走 core 同一锁原语（core 内部封装，调用方无感）。
- 实现：优先用 `fs2` advisory flock（锁目标文件本身或 `~/.skillkit/.lock/<key>.lock`）。

## 9. token 鉴权

- `serve` 启动生成随机 token（uuid 或 hex），路由前缀 `/t/<token>/`。
- 绑定 `127.0.0.1`（不对外），token 防本机其他进程误访问，无需登录。
- 终端打印 `http://127.0.0.1:<port>/t/<token>/`，用户点开。
- SSE 走同前缀，无需额外鉴权。
- `--port` 可指定（默认 7317，spec §11）。

## 10. 测试策略（spec §14）

测试验证业务结果，不验证实现细节。

- core 文件锁原语：单元测试（并发写互斥、读不阻塞、超时报错）。
- server 集成测试（tempdir 注入 Paths + 起 Axum）：
  - GET 视图返回 200 且含关键业务内容（不测 HTML 标签细节）。
  - POST/DELETE 操作后 core 状态正确（断言 `~/.skillkit/` 状态文件 + apply 落地的 symlink/copy）。
  - apply 闭环：勾选 → apply → status diff 正确（复用 `m1_e2e` 的断言模式）。
  - token 鉴权：错 token 返回 404/403。
  - SortableJS reorder：POST 新顺序后 profile 的 skills 顺序持久化。
- SSE：notify 触发后推送正确 scope，可选手动验证（SSE 自动化测试成本高，权衡后不强测）。

## 11. M2 内部阶段拆分（供 writing-plans）

每阶段 TDD 红绿 + commit，沿用 M0/M1 节奏：

1. server crate 骨架：Cargo + Axum 起 + token 中间件 + rust-embed 静态首页 + layout 模板（能 serve，浏览器看到 htmx 加载）。
2. core 文件锁原语 + CLI/server 写操作接入。
3. 四视图只读 GET（Sources/Skills/Profiles/Projects 列表渲染，调 core load）。
4. 写操作片段（POST/DELETE → core 方法 → 返回片段）：源 CRUD + profile add/remove/reorder + project 声明编辑。
5. Projects apply 闭环（勾选 + apply + status diff 片段）。
6. SSE（notify watcher → 推送 → 前端局部刷新）。
7. 视觉打磨（demo/index.html 亮色风格 → app.css）。

## 12. 相对原 spec §12 的调整

1. 前端 React+Vite → **htmx + Askama + SortableJS**（主人定）。砍掉 `web/` 独立前端工程，模板与静态资源全进 server crate。
2. SSE 砍掉 apply 进度推送，只保留跨进程文件变化刷新（§7）。
3. 文件锁从「M2 要做」明确为 M2 硬范围（core 补锁原语，§8）。
4. 默认视图定为 Projects（原 spec 未定，主人最常用 apply 闭环）。

另：`~/.skm/` → `~/.skillkit/`（已全仓库改名，含代码方法名 `skillkit_dir`/`skillkit_skills_dir` 与全部文档）。

## 13. 不在范围（YAGNI）

- apply 细粒度 SSE 进度（同步返回够用）。
- 多用户 / 远程访问（本地单用户，token + localhost 足够）。
- profile 继承（spec §16 预留）。
- 拖拽以外的复杂客户端交互（多步向导、乐观更新）。
- M3 的 import-existing / install upgrade / Brewfile 打包。
