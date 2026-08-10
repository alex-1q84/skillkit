# 安装本地 skill · 前端交互重设计（Modal 浮层三合一）

> 关联：本 spec 重做 `docs/sessions/2026-08-10-skillkit.md` §2.4 已实现的 install-local **前端层**（core/CLI 不动）。原设计文档 `docs/superpowers/specs/2026-08-07-install-local-skill-design.md` 的功能逻辑仍有效，本 spec 只覆盖 GUI 交互与样式。
>
> 日期：2026-08-10。作者：浮浮酱（brainstorming 产出）。

## 1. 背景与问题

install-local 功能链路（core → CLI → server handler → 模板 → htmx）已实现完成并测试通过（180 tests 全绿），但**前端样式是半成品**，交接文档因走查脚本只断言"元素存在"未验 `getComputedStyle` 而误判为"已完成"。实际病灶（根因已定位到行）：

1. **`app.css` 完全没有 `.install-local-form` / `.install-local-panel` 专属规则**——表单靠泛化 `label`/`input`/`button` 样式裸奔。
2. **挂载点是 `<span id="install-local-panel">`（inline 元素）塞在 nowrap flex 容器 `.head-actions` 里**（`fragments/skills_main.html:16` + `app.css:425`）。表单片段一注入，span 被撑成超大 flex item，把右侧「导入存量」「全部升级」按钮挤变形溢出——这是"点击后按钮变形、文字折行"的直接根因。
3. **表单内 4 个 `inline-flex` label + 2 个按钮在默认 block 流里按 whitespace 折行**，无标题、无分组容器、无卡片背景——这是"安装说明和文本框混在一堆、无层次感"的根因。
4. **toolbar 三个动作按钮风格本就不统一**：「安装本地」用小号 `.pill-btn`，「导入存量 / 全部升级」用泛化 `button`（`form.inline`）。

主人诉求：设计全新交互与样式，支持**拖放文件（目录）安装**和**文件向导（原生文件选择器）安装**，并根治上述样式问题。

## 2. 目标 / 非目标

**目标：**
- 入口形态改为 **Modal 浮层**，复用项目现有 `.browse-overlay` / `.browse-modal` 浮层模式（`app.css:329-377`，Projects 页目录浏览在用），遮罩 + 居中卡片天然提供层次感。
- modal 内提供三种安装方式合一：**拖放区**（支持 .zip 文件与目录）、**文件选择按钮**（选 .zip / 选目录）、**路径直接输入**（兜底，开发者粘路径最稳）。
- 高级选项（name / scope / force）默认折叠，符合渐进式展示。
- 根治按钮变形、文字折行、无层次——新增专属 CSS，删掉撑坏 toolbar 的 span 挂载点。
- core 与 CLI **零改动**（`install_local` 已吃路径，复杂度全收敛到 server 端临时区）。

**非目标（YAGNI）：**
- 不做多步骤向导（装一个 skill 是单一动作，多步流程过重）。
- 不引入 React / Vue / npm 构建链 / 前端打包工具（项目硬约束）。
- 不引入前端 zip 库（目录上传走 multipart 多文件 + server 端重建，见 §4.3）。
- 不改 core `install_local` 的签名或行为。
- 不动 SSE 刷新、summary 反馈、toast 错误反馈等已稳定的写操作反馈链路。

## 3. 方案选型

brainstorming 阶段对比三种入口形态：

| 方案 | 形态 | 结论 |
|------|------|------|
| **A. Modal 浮层 · 三合一**（选定） | 遮罩 + 居中卡片，复用 `.browse-modal` | 层次感最强；复用现成浮层模式成本最低；关闭机制（✕/点遮罩/ESC）免费复用；彻底脱离 toolbar 根治按钮变形 |
| B. 行内展开 · 独立卡片 | toolbar 下方展开卡片 | 不离开视图，但层次感弱于 modal，且展开会推挤表格 |
| C. 极简拖放 · Modal | 只做拖放+选择 | 失去路径兜底，本地开发者想粘路径时无路可走 |

主人选定方案 A。

## 4. 详细设计

### 4.1 入口与触发（复用 `.browse-overlay`）

**toolbar 按钮：**
- `fragments/skills_main.html:17-20` 的「安装本地」按钮 class 从 `.pill-btn` 改为与「导入存量 / 全部升级」一致的泛化 `button` 样式（统一三个动作按钮，解决风格不统一）。
- 按钮 `hx-get="/{token}/skills/install-local"` 不变，但 `hx-target` 改为新的固定挂载点 `#modal-mount`，`hx-swap="innerHTML"`。
- **删掉** `skills_main.html:16` 的 `<span class="install-local-panel" id="install-local-panel"></span>`（撑坏 nowrap flex 的真凶）。

**挂载点：**
- 在 `layout.html` 的 `<body>` 末尾新增固定容器 `<div id="modal-mount"></div>`。modal 片段渲染到此（modal 是 `position:fixed` 全屏遮罩，挂载位置不影响视觉）。

**新模板 `fragments/install_local_modal.html`：**
- 骨架照抄 `fragments/browse.html`：`.browse-overlay` > `.browse-modal`（加修饰 class `.install-modal`，宽度调到 600px）> `.browse-header`（标题"安装本地 skill" + `.browse-close` ✕）+ body。
- 复用 `.browse-overlay` class 意味着 `layout.html:57-69` 现有的关闭委托（点 `.browse-close` / 点遮罩本身 / 按 ESC → `closeBrowseOverlay` → `overlay.remove()`）**自动命中**，无需新增任何关闭 JS。

### 4.2 modal 内布局（四区，渐进式展示）

modal body 自上而下：

1. **拖放区**（`.drop-zone`，主区）：虚线边框大方块，⇣ 图标 + "拖放 .zip 文件或目录到此处"。`dragenter` / `dragover` 加 `.drag` 高亮态（`e.preventDefault()` 必须），`dragleave` 移除，`drop` 接收并读取 `e.dataTransfer.items`。
2. **选择按钮区**：两个按钮，分别触发隐藏 `<input>`：
   - 「选择 .zip」→ `<input type="file" accept=".zip">`
   - 「选择目录」→ `<input type="file" webkitdirectory directory>`（浏览器给出 `input.files`，每个 File 带 `webkitRelativePath`）
3. **路径兜底**：分隔线"—— 或直接输入路径 ——" + 文本框（沿用现有 `path` 字段语义，走现有 core）。
4. **高级选项**（默认折叠，用原生 `<details><summary>`）：`name`（可选，placeholder "默认读 SKILL.md"）、`scope`（select，local 默认 / global）、`force`（checkbox，"覆盖已存在"）。
5. **底部操作栏**（`.install-actions`）：右侧 [取消] + [▶ 安装]（主按钮）。**取消按钮显式接线关闭**——`onclick="this.closest('.browse-overlay').remove()"`（现有关闭委托 `layout.html:60` 只认 `.browse-close` 和点遮罩本身 `e.target === overlay`，普通取消按钮两者都不命中，不接线则点了无反应）。主按钮用 §4.5 新增的强调样式 `.install-actions .primary`，**不依赖 app.css 里不存在的 `.btn.primary`**（该 class 是从 demo 照搬的语言，app.css 从未移植，grep 无命中）。

**已选状态：** 用户拖入或选择文件/目录后，拖放区切换为 `.has-file` 态，显示"已选卡片"：文件名/目录名 + 大小 + 类型 + ✕ 清除（清除后回到拖放态）。此时 [安装] 按钮才启用。

**输入互斥：** 三种方式（拖放/选择/路径）任一提供输入即清空另两种的占用——拖放或选择文件后路径框置灰；反之在路径框输入则清空已选卡片。由前端 JS 维护单一输入源。

### 4.3 三条上传路径与技术落地（core 不改）

core `install_local(paths, src_path, name, scope, force)` 吃**路径**（zip 或目录），签名不变。三条路径在 server 端汇入同一个 core 调用，差别只在"如何拿到本地路径"：

**路径 ①：路径直接输入（沿用现有 handler，不改）**
- 表单 POST `path` / `name` / `scope` / `force` 到现有 `POST /{token}/skills/install-local`（`skills.rs:637-664`）。
- handler 调 `install_local(path, ...)`，安装流程行为不变（path/name/scope/force → install_local → 冲突/覆盖语义不变），仅成功 summary 标识从 `f.path` 统一改 `m.id`（见端点设计结论）。

**路径 ②：.zip 拖放 / 选择（新增 upload handler）**
- 前端：拿 `File` 对象（拖放从 `dataTransfer.files`，选择从 `input.files[0]`），构造 `FormData`，append 字段 `archive`（File）+ `name` / `scope` / `force`，POST 到新端点 `POST /{token}/skills/install-local/upload`。
- server：用 `axum::extract::Multipart` 逐 field 读取；把 `archive` 字段的字节写到 `tempfile::TempDir` 下的 `upload.zip`；调 `install_local(tmpdir_path/upload.zip, ...)`；handler 返回时 `TempDir` drop 自动清理。core 的 `extract_zip` 安全校验（ZipSlip / 拒 symlink / 体积上限 / 条目上限）全程复用。

**路径 ③：目录拖放 / 选择（新增 upload handler，同端点）**
- 前端递归收集目录内所有文件 + 相对路径：
  - 拖放目录：`e.dataTransfer.items[k].webkitGetAsEntry()` 递归 `FileSystemDirectoryEntry` / `FileSystemFileEntry`，每个 `File` 配其全路径。
  - 选择目录：`input.files`（`webkitdirectory` 已给出平坦列表），每个 `File` 的 `webkitRelativePath` 去掉顶层目录名即相对路径。
  - 构造 `FormData`，每个文件 append 一个 part：`formData.append('file', fileObj, relpath)`（第三参数 filename 携带相对路径，含 `/`）。额外 append `name` / `scope` / `force`（**不附 `mode` 字段**——端点按字段名 `archive` vs `file` 分流，`mode` 是冗余死字段）。POST 到同一 upload 端点。
- server：`Multipart` 逐 field 读取；对每个 `file` part，取 `field.file_name()` 作为 relpath，**安全过滤**（见 §8）后在 `TempDir` 下按 relpath 创建子目录并写入文件（重建目录树）；全部写入后调 `install_local(tmpdir_path, ...)`；`TempDir` drop 清理。
- **单端点双模**：upload handler 按 `mode` 字段（`zip` / `dir`）区分两种处理；或更简单——按是否出现 `archive` 字段（zip 模式）vs `file` 字段（目录模式）分流。spec 采用后者（按字段名分流，省一个字段）。

**端点设计结论：**
- `POST /{token}/skills/install-local`（现有，path 表单）—— 保留，路径兜底用。
- `POST /{token}/skills/install-local/upload`（新增，multipart）—— zip 与目录上传共用，按字段名分流。
- 两个 handler 成功都返回**完整 Skills 页**（`body outerHTML` 语义，符合前端强约束）。**summary 标识统一用 `install_local` 返回的 `SkillMeta.id`**（形如 `local/<name>`）——path ① 现状用 `f.path`（`skills.rs:654`，用户原始输入），upload 场景无用户可读路径（传的是临时 `upload.zip`/临时目录），故三路径统一改成 `m.id`，summary 形如「✓ 已安装本地 skill：local/my-skill」。失败都 `error_response` 返回 toast（4xx 不刷页）。

### 4.4 反馈机制（沿用现有写操作模式）

- **成功**：handler 返回完整 Skills 页，htmx 按 `hx-target="body" hx-swap="outerHTML"` 替换整个 body → modal（在旧 body 内）随旧 body 一并消失 → 新页面顶部 `<p class="summary">✓ 已安装…</p>` 横幅显示，4s 后自动淡出（现有 `layout.html` 机制）。与 rescope / assign / import 完全一致。
- **失败**：`error_response` 返回 toast（4xx 不刷页，`showToast` 解析 JSON.error），**modal 保持打开**，用户可修正后重试。
- **安装中**：[安装] 按钮 `disabled` + 显示 indicator 文案"安装中…"，防止重复提交（目录上传 multipart 可能耗时）。

### 4.5 CSS（`app.css` 新增专属规则，根除裸奔）

新增以下规则（对齐 demo 暖色 / mono / 卡片视觉语言）：

- `.install-modal`：复用 `.browse-modal` 基础（白底 + border + 圆角 10px + shadow + flex column），宽度调到 `width: 600px`，`max-height: min(80vh, 640px)`。
- `.install-body`：padding 16px，纵向 flex，gap 14px。
- `.drop-zone`：虚线边框（`2px dashed var(--line)`）、圆角 8px、padding 28px、居中文字、`transition`；`.drop-zone.drag`（拖入高亮，边框 + 底色变 `--accent-soft`）；`.drop-zone.has-file`（已选态，边框实线，内部渲染卡片）。
- `.drop-file-card`：已选卡片（文件名 mono + 大小 + ✕ 清除）。
- `.install-fields`：路径兜底与高级选项的纵向字段分组，每字段标题 + 说明 + 控件分行（解决"说明与输入混在一起无层次"）。
- `.install-actions`：底部操作栏，flex 右对齐，gap 8px。
- `.install-actions .primary`：主按钮强调态（`background: var(--ink); color: var(--bg); border-color: var(--ink)`，hover 转 `var(--accent)`，对齐 demo `.btn.primary` 深底语言 `demo/index.html:249-251`）。app.css 无 `.btn`/`.btn.primary`（grep 确认），主按钮样式在此新增落地，不照搬 demo 的 class 名。
- 删除 toolbar `.install-local-panel` span 相关的任何残留（本就无专属规则，确认 `app.css` 无 `.install-local-panel` / `.install-local-form` 残留）。
- toolbar 三动作按钮统一：确认 `.head-actions > button` / `.head-actions form.inline button` 视觉一致（高度、padding、字号对齐）。

## 5. 改动文件清单

| 文件 | 动作 | 内容 |
|------|------|------|
| `crates/server/templates/fragments/install_local_modal.html` | **新建** | modal 片段（overlay/modal/header/body 四区） |
| `crates/server/templates/fragments/install_local_form.html` | **删除** | 被 modal 替代 |
| `crates/server/templates/fragments/skills_main.html` | 改 | toolbar 按钮统一 class + `hx-target="#modal-mount"` + 删 span 挂载点 |
| `crates/server/templates/layout.html` | 改 | 新增 `<div id="modal-mount">` + 拖放/目录递归/输入互斥/折叠 toggle 的原生 JS（关闭复用现有 `closeBrowseOverlay`） |
| `crates/server/src/routes/skills.rs` | 改 | GET `install_local_form` 改为渲染 modal 模板；新增 `install_local_upload`（Multipart handler，zip/目录分流 + 临时区 + 安全校验） |
| `crates/server/src/routes/mod.rs` | 改 | 注册 `POST /{token}/skills/install-local/upload` 路由 |
| `crates/server/static/app.css` | 改 | 新增 §4.5 规则 |
| `crates/core/**` | **不改** | `install_local` 签名行为不变 |
| `crates/cli/**` | **不改** | CLI `install local` 不受影响 |

依赖变更（`crates/server/Cargo.toml`，三条均为编译/测试阻断，须先改）：
- `axum` 加 `features = ["multipart"]`（🔴 P0：否则 `axum::extract::Multipart` 在新 upload handler 编译失败；axum 0.8 默认 features 不含 multipart）。
- `tempfile = "3"` 从 `[dev-dependencies]` 移到 `[dependencies]`（🔴 P0：upload handler 主代码用 `tempfile::TempDir`，当前仅在 dev-dep，主代码引用编译失败；对齐 core 已主依赖的做法）。
- `[dev-dependencies]` 加 `zip = "2"`（🟡 P2：构造测试用 skill zip，server 现无 zip 依赖，core 的 zip 用不到 server 侧）。

## 6. 前端约束遵守（`docs/frontend-rules.md`）

- ✅ 写操作（POST）返回完整页面 `hx-target="body" hx-swap="outerHTML"`——两个 handler 都 `render_skills` 返回完整页。
- ✅ SSE 刷新 `?fragment=1` 纯片段——本次不动 SSE 链路；modal 打开期间若需跳过 SSE，沿用 scan 浮层的"浮层开时跳过"模式（`closeBrowseOverlay` 对 scan 变体已有关闭后刷新逻辑，install-modal 不需要关闭后刷新，因成功已整页替换）。
- ✅ 片段外层固定 id——`#modal-mount` 固定；modal 内各区（`#install-drop-zone` / `#install-path` 等）固定 id。
- ✅ 禁 React/Vue/npm——纯原生 JS + htmx。
- ✅ 业务逻辑只在 core——路径安全过滤属"server 把上传落成可安装的本地路径"的 IO 编排，不是业务推导；name/scope/force 归一仍复用现有 handler 逻辑。
- ✅ 改模板/静态资源后跑 `make check`（Askama 编译错 + clippy 只有 check 暴露）。
- ✅ CSS/模板生效链路：改后 `cargo build -p skillkit-cli`（rust-embed 重打包）→ 重启 serve → 强刷。

## 7. 测试策略

**server 路由测试（`crates/server/src/routes/skills.rs` 或 `tests/`）：**
- 路径表单安装（现有测试保留，回归）。
- multipart 上传 zip 安装：构造 multipart body 含 `archive` 字段（一个合法 skill zip），断言返回完整 Skills 页 + summary 含 `local/<name>`（断言 `SkillMeta.id`，不是 `f.path`）。
- multipart 上传目录：构造 multipart body 含多个 `file` part（filename 携带 relpath，如 `my-skill/SKILL.md`、`my-skill/scripts/run.sh`），断言 server 重建目录树后安装成功。
- 逃逸拦截：multipart 某 `file` part 的 filename 含 `../` 或绝对路径，断言 server 拒绝（4xx toast），不落地任何文件。
- 冲突 + force 覆盖：上传已存在 skill，无 force 拒绝、有 force 覆盖无残留。
- 空字段归一：`name` 空串归 None（回归现有 §2.5 bug 修复）。
- 超上限拦截（🟠 P1）：断言 4xx + 无文件落地临时区。**测试方法**（避免「上限若取 100MiB 则无法不真造大 body」的字面矛盾）：测试用自定义 router 挂小 `DefaultBodyLimit`（如 1MiB）造一个略超的 multipart body 验 413 拒绝路径（确定、快速）；或若 §8 决策降上限，按降后值造超一个量级的假字段。防"测试绿但大文件装不进"盲区。
- GET modal 渲染（⚪ P3-2）：`GET /{token}/skills/install-local` 返回 modal 片段，断言含 `.browse-overlay.install-modal` / `#install-drop-zone` / 取消按钮，防 Askama 模板字段错位（现有 3 个 server 测试只测 POST，GET 无覆盖）。

**core 测试：** 不动（`install_local` 未改）。

**GUI 走查（playwright，DOM 断言，沿用交接 §3.4 约定）：**
- toolbar 三按钮样式一致（`getComputedStyle` 验高度/padding 对齐）。
- 点「安装本地」→ modal 出现（`.browse-overlay.install-modal` 可见）；✕ / 点遮罩 / ESC / **取消按钮** 四种关闭都生效（取消按钮走显式 `onclick` 接线，与现有关闭委托的 `.browse-close`/遮罩命中无关，需单独断言）。
- 拖放区 `.drag` 高亮态（模拟 dragover 事件）；drop 后 `.has-file` 已选卡片显示文件名/大小。
- 「选择 .zip」「选择目录」按钮触发对应隐藏 input。
- 路径输入与拖放/选择的互斥（输入路径清空已选卡片，反之亦然）。
- 高级选项折叠/展开。
- 成功：modal 关闭 + summary 横幅出现 + 4s 淡出（断言用轮询 DOM，不用 `expect_navigation`）。
- 失败：toast 出现 + modal 保持打开。

## 8. 安全考量

- **目录上传路径逃逸**：server 端重建目录树时，每个 `file` part 的 relpath（来自 multipart filename）必须经 `Path::components()` 过滤——拒绝任何 `Component::ParentDir`（`..`）和 `Component::RootDir` / 绝对路径前缀，只接受 `Normal` 分量拼接。重建后用 `canon` + `starts_with(tmpdir)` 做 containment 断言兜底。这与 core `extract_zip` 的 ZipSlip 防御是同等模式。
- **multipart 体积上限（🟠 P1，plan 阶段验证驱动决策）**：axum 0.8 的 `Multipart` 提取器受 `DefaultBodyLimit` 约束，**默认 2MB**，远小于 core 的 `MAX_ZIP_BYTES=100MiB`——upload 端点必须显式 `DefaultBodyLimit::max(...)`，否则 >2MB 的 zip/大目录在到达 core 校验前就被 413 拒掉（"测试绿但大文件装不进"盲区）。决策分两步：① plan 第一个任务实测 multer 默认 `Limits`（per-field 体积 / parts 数 / fields 数）能否承载「100MiB zip / 10000 文件目录」；② 够则 `axum::extract::Multipart` + `DefaultBodyLimit::max(100MiB)`；不够则二选一——(a) 降上限——zip 与目录**均受 multer 默认 per-field `file_size` 约束**（zip 是单个大文件，per-field 限制会先于总体上限生效，不能只降目录），plan 实测后按实际数值同步定 zip/目录各自上限（不假设 zip 能到 100MiB），UI 提示超大包走 CLI 兜底，YAGNI，本地单用户工具倾向此项或 (b) 加 `multer` crate 手工构造 `Multipart` + `Constraints`。测试必须含「超上限 → 4xx + 无文件落地」断言（测试方法见 §7：挂小 `DefaultBodyLimit` 造略超 body 验 413，不依赖真造大 body）。
- **临时区清理**：用 `tempfile::TempDir`，handler 无论成功失败，drop 时自动递归删除，无残留。
- **token 鉴权**：新 upload 端点与现有端点一样走 `/{token}/` 前缀，复用现有 token 校验，不新增攻击面。

## 9. 已知限制 / 风险

1. **multipart filename 含 `/` 的兼容性**：前端用 `formData.append('file', f, 'a/b/SKILL.md')` 把相对路径塞进 filename，依赖 `tokio-multipart` / axum `Multipart` 的 `field.file_name()` 原样返回含 `/` 的字符串。spec 假设可行；**plan 阶段先写一个最小测试锁定此契约**，若实现剥离了 `/`，退化为：每个文件 part 额外携带自定义字段（如 `X-relpath` header 或独立 `relpath` form 字段配对）。
2. **大目录上传耗时**：multipart 逐文件上传 + server 逐文件写盘，千文件级目录会有秒级延迟。本地单用户工具可接受；安装中 indicator 已覆盖体感。不做进度条（YAGNI，超出当前需求）。
3. **webkitGetAsEntry 递归**：极深目录或符号链接环可能导致前端递归异常；前端递归时跳过 symlink entry（对齐 core 拒 symlink 的语义）并设深度上限（建议 32）。
4. **浏览器拖放拿不到绝对路径**：这是本设计的根本前提（§4.3 三条路径的设计动因）。路径兜底（路径①）保留给"想直接用本地路径"的场景，但该路径需是 server 进程可访问的本机路径（本地工具成立）。
5. **install-modal 关闭不刷新 main**：与 scan-flyout 不同，install 成功已整页替换 body（含 main），失败保持现状，故 `closeBrowseOverlay` 对 install-modal 不需要关闭后刷新（复用时不带 `scan-flyout` class 即可）。
6. **body 整页替换后 JS 监听器重绑定**：写操作成功后 htmx 以 `body outerHTML` 替换整个 body，body 上的事件监听器会随之丢失。现有 rescope/assign 等写操作已验证：layout.html 的 inline `<script>` 在 body 替换后被 htmx 重新执行，监听器重绑定，故关闭浮层/summary 淡出等在写操作后仍可用。新增的拖放/互斥/折叠 JS 须放在 layout.html 同一 `<script>` 块，沿用此已验证模式；plan 阶段用一个"安装成功后再次开/关 modal"的用例显式锁定。
7. **file input 清空限制（⚪ P3-1）**：浏览器只能 `input.value = ''` 清空 file input（不能赋其他值），且同一文件二次选择不触发 `change`。互斥逻辑（路径框输入清空已选卡片）必须同时清 file input 的 value，否则"改路径后再拖同一文件"已选态残留。
8. **安装中按钮态与 htmx indicator（⚪ P3-4）**：目录上传用 `htmx.ajax` 提交（非表单 `hx-post`），`hx-indicator` 默认行为不适用，需在提交前后手动 set [安装] `disabled` + 切换"安装中…"文案，防重复提交。

## 10. 验收标准

- toolbar「安装本地」与「导入存量」「全部升级」三按钮视觉统一（同高同字号同 padding），点击不再导致任何按钮变形或文字折行。
- 点击「安装本地」弹出居中 modal 浮层（带遮罩），✕ / 点遮罩 / ESC 三种方式可关闭。
- modal 内拖放区支持拖入 .zip 文件和目录两种；「选择 .zip」「选择目录」按钮分别触发对应原生选择器；路径兜底文本框可用；三种方式互斥。
- 高级选项默认折叠，展开后 name/scope/force 可调。
- 三种方式安装成功后：modal 关闭、summary 横幅"✓ 已安装本地 skill：…"出现并 4s 淡出、Skills 表格出现新行。
- 安装失败（冲突/路径无效/逃逸）：toast 提示，modal 保持打开。
- core 与 CLI 零改动，`cargo test -p skillkit-core` 全绿，CLI `install local` 行为不变。
- `make check` 全绿（含新增 server 路由测试 + Askama 模板编译 + clippy）。
