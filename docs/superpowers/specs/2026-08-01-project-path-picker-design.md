# 项目路径文件选择向导 — 设计文档

> 日期：2026-08-01
> 范围：Projects 视图「注册项目」「扫描发现」两个表单的路径输入，从手输绝对路径改为「输入框 + 浏览按钮」混合形态——浏览器逐级选目录后回填。
> 代码层规范见 `CLAUDE.md`；本文是本次完善的设计权威。

## 1. 背景与动机

GUI parity 实现后，Projects 视图有了「注册项目」（`path` 输入）和「扫描发现」（`dir` 输入）两个表单，但路径都是**手输绝对路径**（如 `/Users/me/app`）。痛点：用户要切终端 `pwd` 复制、或手打长路径易错，违背「不要让用户思考」「系统承担复杂性」。

技术上，浏览器原生 `<input type="file" webkitdirectory>` 出于安全沙箱**不暴露完整本地绝对路径**（只给目录名），不满足 skillkit 需要绝对路径的要求。故用 server 端目录浏览：server 读本地目录（`127.0.0.1` 本地，无安全风险），渲染浏览片段，前端 htmx 逐级点击 + `hx-swap-oob` 回填。

## 2. 目标与非目标

**目标**

- 注册/扫描两表单加「浏览...」按钮，点开逐级目录浏览面板，选定回填输入框；输入框仍可手输（混合）。
- 严格三层：浏览端点 server 薄壳（`std::fs` 读目录 + 过滤），零业务逻辑；前端 htmx 片段 + `hx-swap-oob` 回填，零裸 JS。
- 遵循前端强规则（§7.5）：片段外层固定 id、htmx 片段渲染。

**非目标（YAGNI）**

- 不做文件选择（只目录；skillkit 关心项目根 / 扫描根，都是目录）。
- 不做跨平台 native 文件对话框（浏览器拿不到路径，server 浏览是替代）。
- 不持久化「上次浏览位置」（每次从输入框当前值或 home 起）。
- 不重构现有注册/扫描表单的提交逻辑（只加浏览按钮 + 面板 div）。

## 3. 设计

### 3.1 浏览端点（server 新增 1 个）

`GET /{token}/projects/browse`

query 参数：

| 参数 | 含义 |
|------|------|
| `path` | 要列的目录（缺省/空/不可读 → home）；含 `~` 展开为 home |
| `into` | 选定时回填的输入框 id（如 `project-path` / `scan-dir`） |
| `panel` | 浏览面板 div id（如 `browse-panel-add` / `browse-panel-scan`） |
| `select` | 可选；存在时表示「选定 path 下此子目录名」触发回填（不列目录） |

行为：

- **`select` 存在**（选定动作）：返回两个 `hx-swap-oob` 元素——`<input id="{into}" name="{原name}" value="{path}/{select}" ... hx-swap-oob="true">`（保留原 input 的 name/type/placeholder/required，仅设 value）+ `<div id="{panel}" hx-swap-oob="true"></div>`（清空面板关闭）。htmx 自动 oob 更新。
- **`select` 不存在**（浏览动作）：列 `path` 下子目录，跳过隐藏（`.` 开头）+ 跳过文件（`!is_dir`），按名字排序，渲染 `browse.html` 片段。

### 3.2 浏览片段 `fragments/browse.html`

```
┌──────────────────────────────────────┐
│ 📁 /Users/me/code          [↑ 上级]  │   当前 path + 上级按钮
├──────────────────────────────────────┤
│ project-a/   [进入]  [✓ 选定]        │   每个子目录两按钮
│ project-b/   [进入]  [✓ 选定]        │
└──────────────────────────────────────┘
```

- 顶部当前 path + 「↑ 上级」：`hx-get browse path={parent}, into, panel, target={panel}`（parent = path 的 parent）
- 每个子目录：
  - 「进入」：`hx-get browse path={当前}/{name}, into, panel, target={panel}`（刷新面板到子目录）
  - 「✓ 选定」：`hx-get browse path={当前}, select={name}, into, panel` + `hx-swap="none"`（触发 oob 回填，不替换按钮自身）
- 空目录：「（无子目录）」

### 3.3 前端改造（`projects_main.html`）

注册表单 + 扫描表单的输入框旁各加「浏览...」按钮，表单下各加面板 div。注册表单示例：

```html
<form class="inline" hx-post="/{{ token }}/projects" hx-target="body" hx-swap="outerHTML">
  <input id="project-path" name="path" placeholder="项目绝对路径（如 /Users/me/app）" required>
  <button type="button"
          hx-get="/{{ token }}/projects/browse?into=project-path&panel=browse-panel-add"
          hx-target="#browse-panel-add"
          hx-include="#project-path">  <!-- 带输入框当前值作 path 起点 -->
    浏览...
  </button>
  <input name="agents" placeholder="agents（可选）">
  <button>注册项目</button>
</form>
<div id="browse-panel-add"></div>
```

扫描表单同理（`into=scan-dir`、`panel=browse-panel-scan`、`hx-include="#scan-dir"`，下接 `<div id="browse-panel-scan"></div>`）。

注意：

- 浏览按钮 `type="button"`（不触发表单提交）。
- `hx-include="#project-path"`：输入框 `name=path`，其值作为 browse 请求的 `path` 起点；空则 browse 端点兜底用 home。
- 面板 div 在表单外（表单下方），就近展开（方案 B）。

### 3.4 选定回填（`hx-swap-oob`，纯 htmx）

选定按钮 `hx-get browse ...&select={name}` + `hx-swap="none"`。server 响应：

```html
<input id="project-path" name="path" placeholder="..." value="/Users/me/code/project-a" required hx-swap-oob="true">
<div id="browse-panel-add" hx-swap-oob="true"></div>
```

htmx 收到 `hx-swap="none"` 的响应后扫描其中的 `hx-swap-oob` 元素，用 outerHTML 更新页面对应 id（输入框 value 带入 + 面板清空关闭）。零裸 JS。

### 3.5 路径与过滤规则

- 起始 path：输入框当前值（via `hx-include`）；空/无效 → `dirs::home_dir()`（绝不硬编码 `/Users/...`）。
- `~` 展开：手动（Rust `std::path` 不展开 `~`），`~/x` → `home/x`，`~` 单独 → `home`。
- canonicalize：相对路径转绝对（失败用原值，不 panic）。
- 列目录过滤：`is_dir && !name.starts_with('.')`（跳过隐藏目录 + 跳过文件）。
- 排序：按名字（确定性，便于测试断言）。

## 4. 取舍

- **混合（输入框 + 浏览按钮）vs 纯逐级浏览 vs 纯补全**：选混合。纯逐级不能快速粘贴已知路径；纯补全仍以输入为主。混合两者兼得。
- **`hx-swap-oob` 选定 vs `onclick` JS**：选 oob。htmx 原生机制，零裸 JS，符合 §7.5；`onclick` 虽 allowed 但 oob 更「htmx 兼容」。
- **跳过隐藏目录**：`.git`/`node_modules`/`.vscode` 是噪音；用户极少选它们做项目根。若需要，仍可手输入框。
- **各表单独立 panel div vs 共用**：选独立。面板就近展开在触发表单下，视线不跳。
- **不做文件选择**：注册/扫描都是目录，YAGNI。

## 5. 测试策略

集成测试（`crates/server/tests/routes.rs`）：

- tempdir 造 `a/`、`b/`、`.hidden/`、`file.txt`，GET browse `path=tempdir`：断言含 `a`、`b`，不含 `.hidden`、`file.txt`；含「上级」按钮（带 parent）；每个子目录有「进入」+「选定」按钮。
- GET browse `path=tempdir select=a into=project-path panel=browse-panel-add`：断言含 `id="project-path"` + `value=".../a"` + `hx-swap-oob="true"`；含 `id="browse-panel-add" hx-swap-oob="true"`。
- GET browse `path=/不存在`：断言含「不可读」提示，状态 200（不 panic）。
- GET browse 无 path 参数：断言渲染了 home 下某目录（不 panic；可用 `HOME` 环境变量隔离造 fake home）。

改完 `make check` + `make e2e`（若 e2e 选择器受 projects_main 改动影响，按需更新）。

## 6. 验收标准

- 注册/扫描两表单输入框旁都有「浏览...」按钮；点开逐级目录浏览面板，就近展开在触发表单下。
- 面板「进入」逐级、「↑ 上级」回退、「✓ 选定」回填输入框并关闭面板（纯 htmx，无裸 JS）。
- 输入框仍可手输（混合）。
- 跳过隐藏目录与文件。
- 路径不存在/无权限时给可读提示，不 panic。
- `make check` 双绿，新增 browse 集成测试。
- 现有注册/扫描端点行为不变（回归不破）。

## 7. 风险

- **目录不可读**（权限）：handler 兜底提示，不 panic。
- **符号链接循环**：浏览是单层展开（列直接子目录，不递归），`is_dir` 判定无递归风险。
- **超大目录**（如 `/usr/bin`）：本地 GUI 可接受；若实测卡顿，加「前 N 条 + 提示」（YAGNI，先不做）。
- **`~` 展开与 home 兜底务必用 `dirs::home_dir()`**，绝不硬编码路径（CLAUDE.md §7）。
