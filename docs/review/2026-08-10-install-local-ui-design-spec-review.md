# Spec Review — install-local UI 重设计（Modal 浮层三合一）（2026-08-10 · 复评 v2）

> 审查对象：`docs/superpowers/specs/2026-08-10-install-local-ui-design.md`（v1 审查后已修改，本文件为复评结论）
> 审查基准：对代码逐一核对（`crates/core`、`crates/server`、templates、tests、Cargo.toml、Cargo.lock、axum 0.8.9 源码、demo/index.html）+ 交接记录 `docs/sessions/2026-08-10-skillkit.md`。
> 日期：2026-08-10
> 结论：**v1 全部 P0（2）/ P1（1）/ P2（4）已在修改中闭环，其余新增声明（取消按钮接线、`.install-actions .primary`、`m.id` 统一、依赖变更、§8 决策框架、§9 风险补录）逐一核对成立，spec 现可执行。** 残留 6 条 P3（不阻塞），其中「超上限测试怎么不真造 100MiB 也能触发」需 plan 阶段定实测方案。详见 §2。

## 1. 总体结论

- **v1 问题闭环情况**：P0-1（axum `multipart` feature）→ §5 依赖变更（L138）；P0-2（tempfile 移主依赖）→ §5（L139）；P1（multipart 2MB 默认限制）→ §8 重写为「plan 实测 multer Limits → 够则 DefaultBodyLimit、不够则降上限或手工 multer」决策框架（L179）+ §7 补超上限拦截测试（L161）；P2-1（主按钮样式）→ §4.2 明确不依赖 `.btn.primary` + §4.5 新增 `.install-actions .primary`（L119）；P2-2（取消按钮接线）→ §4.2 显式 `onclick="this.closest('.browse-overlay').remove()"`（L72）；P2-3（zip dev-dep）→ §5（L140）；P2-4（summary 标识）→ §4.3 三路径统一 `SkillMeta.id`（L101）+ §7 断言改 `local/<name>`（L156）。
- **新增声明逐一核对通过**：
  - L72 引用的 `layout.html:60` 关闭委托条件原文 `if (e.target.closest('.browse-close') || e.target === overlay)` ✅，取消按钮走显式 onclick 的判断成立（普通按钮两条件都不命中）。
  - L72/L119 的「`.btn.primary` app.css 从未移植、demo 照搬语言」✅：app.css grep 无 `.btn`，demo 有 `.btn.primary`（`demo/index.html:249`）。
  - L119 新规则引用的 `var(--accent)` / `var(--surface)` ✅ 已定义（`app.css:13` `--accent: #b45309`、L6 `--surface`）。
  - L101 的「path ① 现状用 `f.path`（`skills.rs:654`）」✅ 原文 `Some(&format!("✓ 已安装本地 skill：{}", f.path))`。
  - L138 的「axum 0.8 默认 features 不含 multipart」✅（axum 0.8.9 Cargo.toml default 列表无 multipart；L139 tempfile 现状只在 `[dev-dependencies]` ✅）。
  - L179 的「`Multipart` 默认受 `DefaultBodyLimit` 2MB 约束」✅（axum 0.8.9 `src/extract/multipart.rs` 文档原文）。
  - §9.7/§9.8（file input 清空限制、htmx indicator 不适用 htmx.ajax）与 L162 GET modal 渲染测试均合理。
- **残留下沉为 P3**：超上限测试的可行上限值、§8 选项 (a) 未覆盖 zip 的 multer per-field 约束、`mode=dir` 与按字段名分流的冗余、§4.3 路径①「行为完全不变」与 L101「统一 m.id」的措辞张力、ESC 关所有 overlay、`.primary` 与 demo 主按钮视觉差异。均不阻塞执行。
- **结论：spec 可执行，无 P0/P1/P2 残留。** 唯一需 plan 阶段现场定的是「超上限测试如何不造 100MiB body 也触发 4xx」（见 P3-1），其余按 §4 顺序执行即可。

## 2. 问题清单

> v1 的 P0×2 / P1×1 / P2×4 已在 spec 修改中闭环（见 §1），本节只列复评残留问题。严重度分级同前。

### ⚪ P3-1 — 超上限测试的「被测上限值」未定死，可能与「不真造 100MiB」矛盾

- 现象：§7（L161）要求「构造超过所设 DefaultBodyLimit 的 multipart body…用假超大字段，不真造 100MiB」；但若 §8 决策结果是 `DefaultBodyLimit::max(100MiB)`，超过它就需要 >100MiB body，无法「不真造」。
- 证据：`crates/server/Cargo.toml` 无 DefaultBodyLimit 配置；axum 默认 2MB（`src/extract/multipart.rs` 文档）。测试基建 `tests/routes.rs` 用 `app.oneshot(...)` 走完整 router。
- 影响：不解决则此测试要么写不出来，要么写成假断言（验不到 413 路径）。
- 修正建议（plan 阶段选一）：① 测试用自定义 router 挂小 `DefaultBodyLimit`（如 1MiB）造一个略超的 body 验 413 拒绝路径（快速、确定）；② 若走 §8 选项 (a) 降上限，就按降后值造「超上限一个量级」的假字段；③ 若走 multer 手工 Constraints，改测 multer field 上限（10MiB 级），body 只须 11MiB。
- 测试盲区：无（新增测试）；但要防「上限 100MiB 而测试只验 2MB 默认值」这种错位。

### ⚪ P3-2 — §8 选项 (a) 只降目录上限，未提 zip 的 multer per-field 约束

- 现象：§8（L179）选项 (a) 写「zip 100MiB、目录总 50MiB/2000 文件」——若 plan 实测发现 multer 默认 per-field `file_size`（疑似 10MiB 级）是绑定约束，zip 是**单个 100MiB 文件**，同样会被拒，不能只降目录。
- 证据：multer 未进 Cargo.lock（feature 未开），默认 `Limits` 数值需 plan 实测；axum `Multipart` 提取器不暴露 constraints 配置（`src/extract/multipart.rs` 直接 `multer::Multipart::new`）。
- 影响：若只按 spec 措辞降目录上限，zip 大文件仍 4xx。
- 修正建议：§8 选项 (a) 补一句「zip 上限同样受 multer per-field file_size 约束，需同步调整或降 zip 上限」；plan 第一任务实测后按实际数字定。
- 测试盲区：同 P3-1——只测小 zip 盖不住。

### ⚪ P3-3 — `mode=dir` 字段与「按字段名分流」冗余

- 现象：§4.3 目录路径 append `mode=dir`（L94），但 L96 明确「按字段名分流（archive vs file），省一个字段」。
- 证据：L94 与 L96 并存。
- 影响：`mode` 字段成了死重；server 端要么忽略它，要么分流逻辑实际仍靠字段名，留一个没用的字段容易让执行者困惑。
- 修正建议：删掉 `mode=dir` append（与 L96 决策一致），或在 §8/计划里注明「server 忽略 mode」。选前者最省。
- 测试盲区：无。

### ⚪ P3-4 — §4.3 路径①「行为完全不变」与 L101「统一 m.id」措辞张力

- 现象：L84 写「handler 调 install_local(path, ...)，行为完全不变」，L101 又写「三路径统一改成 m.id」。
- 证据：L84 vs L101。
- 影响：执行者可能误解为 path ① 不动 summary，或误解为 handler 全改。实际是：**安装流程（path/name/scope/force → install_local → 冲突/覆盖语义）不变，仅成功 summary 的标识从 `f.path` 换成 `m.id`**。
- 修正建议：L84 措辞改为「安装流程行为不变（成功 summary 标识统一改 `m.id`，见端点设计结论）」。
- 测试盲区：现有 3 个 POST 测试（`tests/routes.rs:1782/1820/1869`）均不验响应体 summary 文本，改标识无回归风险——但 L156 新断言要验的是 `local/<name>`。

### ⚪ P3-5 — ESC 仍会关掉所有 `.browse-overlay`

- 现象：v1 P3-3 未被 spec 提及；`layout.html:65-69` 对全部 overlay 调 `closeBrowseOverlay`。
- 影响：正常场景同屏只有一个浮层，可接受；留档提醒即可。
- 修正建议：不修。

### ⚪ P3-6 — `.install-actions .primary` 与 demo 主按钮视觉不完全一致

- 现象：spec L119 新规则是 `background: var(--accent)`（琥珀底），demo 的 `.btn.primary` 是 `background: var(--ink); color: var(--bg)`（深底，hover 才转 accent）（`demo/index.html:249-251`）。
- 证据：`demo/index.html:249-250` vs spec L119。
- 影响：「对齐 demo 暖色强调」措辞与 demo 实际主按钮视觉有出入（demo 主按钮是深色）。纯视觉偏好，不阻塞。
- 修正建议：执行时定调——用 spec 的琥珀底（与 `.pill-btn` 过渡一致）或 demo 的深底都可，别照抄 demo class 名即可（spec 已明确）。

## 3. 核对通过明细（供执行时对照，逐项已验证）

| Spec 声明（含复评新增） | 验证结果（文件:行号） |
|---|---|
| 取消按钮接线判断：现有关闭委托只认 `.browse-close` 和 `e.target === overlay`（L72 引 `layout.html:60`） | ✅ `layout.html:60` 原文 `if (e.target.closest('.browse-close') || e.target === overlay) {`；普通取消按钮两条件均不命中 |
| `.btn.primary` 是 demo 语言、app.css 从未移植（L72/L119） | ✅ app.css grep `.btn` 无命中；`demo/index.html:242` `.btn`、`:249` `.btn.primary` |
| `.install-actions .primary` 引用的 `var(--accent)`/`var(--surface)` 已定义 | ✅ `app.css:13` `--accent: #b45309`、`app.css:6` `--surface` |
| path ① 现状 summary 用 `f.path`（L101 引 `skills.rs:654`） | ✅ `skills.rs:654` `Some(&format!("✓ 已安装本地 skill：{}", f.path))` |
| 三路径统一 `m.id` 无回归风险 | ✅ 现有 3 个 POST 测试只验状态码+registry（`tests/routes.rs:1782/1820/1869`），不验 summary 文本；`render_skills` 完整页（`skills.rs:110-127`） |
| axum 0.8 默认 features 不含 multipart（L138） | ✅ axum 0.8.9 Cargo.toml `default` 列表；`src/extract/multipart.rs` `cfg(feature = "multipart")`；Cargo.lock 无 multer |
| tempfile 当前仅在 `[dev-dependencies]`（L139） | ✅ `crates/server/Cargo.toml:25`；core 主依赖 `tempfile = "3"`（对齐做法成立） |
| `Multipart` 默认受 `DefaultBodyLimit` 2MB 约束（L179） | ✅ axum 0.8.9 `src/extract/multipart.rs` 文档「by default limits the request body size to 2MB」+ `req.with_limited_body()` |
| `--accent-soft`/`--line`/`--mono` 等 §4.5 用到的变量 | ✅ `app.css:15/11/22` 等 |
| toolbar 三按钮样式差异（`.pill-btn` 11px/4px 10px vs 泛化 `button` 12px/6px 12px） | ✅ `app.css:121`（button）、`app.css:439-446`（.pill-btn）；统一改泛化 button 可行 |
| modal 骨架照抄 `browse.html`（overlay > modal > header ✕ > body） | ✅ `browse.html:1-36` |
| 现有 3 个 server 测试只验落库不验响应体（GET 无覆盖，L162 补） | ✅ `tests/routes.rs:1782/1820/1869`（POST，仅状态码+registry） |
| 写操作完整页 / 失败 4xx toast / SSE 纯片段 / summary 4s 淡出 均在现状代码 | ✅ `skills.rs:110-127`、`mod.rs:17-26`、`layout.html:31-46/183-200` |
| core `install_local` 签名与 100MiB/10000 上限不变 | ✅ `install_local.rs:238-244`、`:9-10` |

## 4. 修正建议的执行顺序

1. **plan 阶段第一任务定为「实测 multer 默认 `Limits` + 定 DefaultBodyLimit 上限」**（对应 P1 决策 + P3-1/P3-2）：
   - 起一个临时 axum 服务或单测，逐字段压 multer 默认 per-field file_size / parts / fields 上限数值；
   - 按结果选 §8 的 (a) 降上限或 (b) 手工 multer + Constraints；定死 zip 与目录各自上限；
   - 定「超上限 4xx」测试怎么触发（P3-1 三选一）。
2. **执行 spec §5 文件清单**：Cargo.toml 三处依赖（axum multipart / tempfile 移主 / zip dev-dep）→ 新建 `install_local_modal.html`（含取消按钮 onclick、`.install-actions .primary`）→ 改 `skills_main.html`（按钮 class + `#modal-mount` + 删 span）→ `layout.html`（`#modal-mount` + JS）→ `skills.rs`（GET 换模板 + summary 改 `m.id` + 新增 upload handler）→ `mod.rs`（注册 upload）→ `app.css`（§4.5）。
3. **顺手消化 P3**：删 `mode=dir`（P3-3）；L84 措辞改「安装流程不变、summary 统一 m.id」（P3-4）；`.primary` 视觉定调（P3-6）。
4. **测试按 §7**（含 GET modal 渲染、超上限、summary 断言 `local/<name>`），收尾按交接 §3.3 生效链路（build cli 二进制 → 重启 serve → 强刷）；GUI 走查用 DOM 轮询，不用 `expect_navigation`（交接 §3.4）。

## 5. 结论

- **v1 全部阻塞项已闭环**：P0×2（依赖）、P1×1（multipart 上限决策框架 + 测试）、P2×4（主按钮样式、取消接线、zip dev-dep、summary 标识）均有落点，新增声明逐一核对通过。
- **spec 可执行**，残留 6 条 P3 不阻塞，其中 P3-1/P3-2（超上限测试的实测路径）须在 plan 第一任务消化，避免测试写不出或假断言。
- 核心架构决策（复用 `.browse-overlay` + 现有关闭委托、三路径汇入同一 core 调用、`m.id` 统一标识）与代码现状一致，无推翻项。
