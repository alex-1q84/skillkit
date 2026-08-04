# Projects 路径输入交互升级设计（2026-08-03）

> 范围：Projects 页两处路径输入交互升级——①「浏览...」目录列表从平铺展开改为居中浮层；②路径输入框增加 Tab 前缀补全。
> 上游规范：`CLAUDE.md` §7.5 前端约定 + `docs/frontend-rules.md`。

## 1. 背景与目标

Projects 页（`/{token}/projects`）的注册表单和扫描表单各有路径输入 +「浏览...」按钮。两个痛点：

- **浏览目录列表平铺**：点「浏览...」把目录列表渲染进表单下方的 `#browse-panel-add` / `#browse-panel-scan`，列表把页面内容往下顶，目录深时占大量纵向空间。
- **路径只能整段手输或鼠标点选**：键盘流式输入无补全。

两个目标：

1. **浏览浮层化**：目录浏览列表从「平铺展开」改成**居中浮层**（全屏遮罩 + 模态卡片），对齐桌面文件选择对话框直觉。表单（按钮 + 输入框）原位不动。
2. **Tab 补全**：路径输入框（注册 `#path` / 扫描 `#dir`）输入部分路径按 Tab 列出**前缀匹配**的子目录候选（shell/IDE 风格），方向键高亮、回车补全，逐级续补。与浏览浮层互补：浮层是鼠标目录树导航，Tab 补全是键盘流式输入。
3. **scan 浮层 + toggle**：扫描发现的项目候选从表单下方平铺改居中浮层；候选按全路径 canonical 精确匹配标记「已注册」；点候选按钮 toggle 注册/注销，浮层保持可连续操作多个项目，手动关闭。

非目标（YAGNI）：不改 `scan_results`（扫描发现的项目候选，仍渲染在扫描表单下方）；不引入 HTML5 `<dialog>`；不做多选 / 收藏目录等新功能。

## 2. 现状

| 文件 | 作用 |
|---|---|
| `templates/fragments/browse.html` | 目录浏览片段：cwd 栏 +「↑上级」+ 子目录列表（每条「进入」/「✓选定」） |
| `templates/fragments/browse_select.html` | 选定动作：oob 回填 input + 清空挂载点 |
| `src/routes/projects.rs::browse` | 列 path 下子目录渲染 browse.html；带 `select` 时渲染 browse_select.html |
| `src/routes/projects.rs` 私有 `resolve_dir` / `list_subdirs` | 路径解析（~ / home / canonicalize）+ 列直接子目录 |
| `templates/fragments/projects_main.html` | 注册 / 扫描表单，含 `#path` / `#dir` 输入框 + `<div id="browse-panel-*">` 挂载点 + 浏览按钮 |
| `static/app.css` | **无** `.browse-*` / `.complete-*` 样式 |

浏览 htmx 流程（现状）：点「浏览...」→ `GET browse?into=path&panel=browse-panel-add`，`hx-target=#browse-panel-add`（默认 innerHTML swap）→ browse.html 塞进挂载点；点「进入」→ 新 browse.html 替换；点「✓选定」→ browse_select.html 用 oob 回填 input + 清空挂载点。

现状缺陷：① browse.html 顶层带 `id={{panel}}`，swap 进同 id 挂载点造成 **id 重复嵌套**；② 列表平铺无样式、无滚动、顶页面；③ 无键盘补全。

## 3. 设计·浏览浮层化

### 3.1 浮层结构（browse.html 重写）

顶层**去掉 id**（消除嵌套），改为遮罩 + 模态两层；「进入」「上级」`hx-target` 明确指向挂载点 `#{{panel}}`：

```html
<div class="browse-overlay">
  <div class="browse-modal" role="dialog" aria-label="选择目录">
    <header class="browse-header">
      <span class="browse-cwd" title="{{ current }}">📁 {{ current }}</span>
      <button type="button" class="browse-close" aria-label="关闭">✕</button>
    </header>
    {% if parent != current %}
    <div class="browse-toolbar">
      <button type="button"
              hx-get="/{{ token }}/projects/browse?path={{ parent }}&into={{ into }}&panel={{ panel }}"
              hx-target="#{{ panel }}">↑ 上级</button>
    </div>
    {% endif %}
    <div class="browse-body">
      {% if dirs.is_empty() %}
      <p class="muted browse-empty">（无子目录）</p>
      {% else %}
      <ul class="browse-list">
        {% for d in dirs %}
        <li>
          <span class="browse-name">📁 {{ d }}/</span>
          <span class="browse-ops">
            <button type="button"
                    hx-get="/{{ token }}/projects/browse?path={{ current }}/{{ d }}&into={{ into }}&panel={{ panel }}"
                    hx-target="#{{ panel }}">进入</button>
            <button type="button"
                    hx-get="/{{ token }}/projects/browse?path={{ current }}&select={{ d }}&into={{ into }}&panel={{ panel }}"
                    hx-swap="none">✓ 选定</button>
          </span>
        </li>
        {% endfor %}
      </ul>
      {% endif %}
    </div>
  </div>
</div>
```

要点：顶层 `.browse-overlay` **无 id**；「进入 / 上级」`hx-target=#{{panel}}`（挂载点），innerHTML swap 替换挂载点内容为新 overlay——无嵌套、无 id 重复；「✓选定」`hx-swap=none` 不变。

### 3.2 关闭交互（四路）

| 触发 | 机制 | 是否新增 |
|---|---|---|
| ✕ 按钮 | 原生 JS（事件委托）移除最近 `.browse-overlay` | 新增 |
| 点遮罩空白（`.browse-overlay` 本身，非 modal） | 同上 | 新增 |
| ESC | 移除所有 `.browse-overlay` | 新增 |
| 「✓选定」 | 复用 browse_select：oob 清空挂载点 → overlay 消失 | 不动 |

`layout.html` 的 `<script>` 追加（事件委托，对动态 swap 进来的 overlay 生效）：

```js
// 目录浏览浮层关闭：✕ / 点遮罩 / ESC（纯 UI 收起，不涉业务）
function closeBrowseOverlay(overlay) { overlay.remove(); }
document.body.addEventListener('click', (e) => {
  const overlay = e.target.closest('.browse-overlay');
  if (!overlay) return;
  if (e.target.closest('.browse-close') || e.target === overlay) {
    e.preventDefault();
    closeBrowseOverlay(overlay);
  }
});
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    document.querySelectorAll('.browse-overlay').forEach(closeBrowseOverlay);
  }
});
```

✕ 按钮为纯 `type="button"` 无 hx 属性，不触发 htmx。关闭是纯前端 UI 状态收起，符合「轻量原生 JS 增强交互」。

### 3.3 浏览样式（app.css 新增）

```css
/* ---------- 目录浏览浮层 ---------- */
.browse-overlay {
  position: fixed; inset: 0; z-index: 100;
  background: rgba(28, 25, 23, .42);
  display: flex; align-items: center; justify-content: center;
  padding: 24px;
}
.browse-modal {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  box-shadow: var(--shadow);
  width: 560px; max-width: 100%;
  max-height: min(70vh, 560px);
  display: flex; flex-direction: column;
  overflow: hidden;
}
.browse-header {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--line);
  background: var(--surface-2);
}
.browse-cwd {
  font-family: var(--mono); font-size: 12px; color: var(--ink-2);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.browse-close {
  padding: 2px 8px; color: var(--ink-3); border: none; background: none;
  font-size: 14px; line-height: 1; cursor: pointer;
}
.browse-close:hover { color: var(--danger); }
.browse-toolbar { padding: 8px 14px; border-bottom: 1px solid var(--line-2); }
.browse-body { overflow: auto; flex: 1; padding: 4px 0; }  /* 内容超出滚动核心 */
.browse-empty { padding: 16px 14px; }
.browse-list { list-style: none; }
.browse-list li {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 8px 14px; border-bottom: 1px solid var(--line-2);
}
.browse-list li:last-child { border-bottom: none; }
.browse-list li:hover { background: var(--surface-2); }
.browse-name {
  font-family: var(--mono); font-size: 12.5px; color: var(--ink);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.browse-ops { display: inline-flex; gap: 4px; flex-shrink: 0; }
```

固定大小：宽 560px（`max-width:100%` 防小屏溢出），高 `min(70vh, 560px)`；`.browse-body` `overflow:auto` 滚动；`z-index:100` 盖过 topnav(50)。

## 4. 设计·Tab 补全

### 4.1 交互语义（前缀匹配）

适用：注册 `#path`、扫描 `#dir`，复用同一套逻辑。输入框值 P，按 Tab（preventDefault）→ 拆 `base / prefix`：

- P 以 `/` 结尾或空 → base = P（解析后），prefix = ""（列 base 全部子目录）。
- 否则 → base = P 的父目录，prefix = P 最后一段。
- `~` / 空按 home 解析（复用 `resolve_dir`）。

列 base 直接子目录，过滤 `prefix` 开头，渲染候选（短名显示，`data-path` 存 `base/子目录` 完整路径）。

操作：默认高亮首项；↓/↑ 循环移动高亮 + `scrollIntoView`；回车把高亮项 `data-path` 补全进输入框 + 关候选；Esc / 失焦关候选。

**补全路径带尾斜杠**（补成 `/Users/mywo/lab/`）——支持逐级续补：补完再 Tab，base = 新路径，列其子目录。最终注册时 `canonicalize` 自动去尾斜杠，无副作用。备选：不带斜杠、用户手动加 `/` 续补（省一次删除但每次续补多一步）；默认带斜杠，主人 review 可改。

边界：候选为空 → 候选区不显示，Enter 正常提交表单（注册 / 扫描）。

### 4.2 端点与数据

新端点 `GET /{token}/projects/complete?path=<P>&panel=<挂载点id>` → `fragments/complete.html`。handler 复用 `resolve_dir`（解析 ~ / home）+ `list_subdirs`（列直接子目录），加 base/prefix 拆分 + 前缀过滤。这俩是 server 层路径工具（browse 也用），非 core 业务逻辑，不跨界。路径无效 / 无子目录 / 无前缀匹配 → 返回空 `.complete-list`。

### 4.3 候选片段（新增 fragments/complete.html）

```html
<div id="{{ panel }}" class="complete-list">
  {% for c in &candidates %}
  <div class="complete-item" data-path="{{ c.full }}/">{{ c.short }}/</div>
  {% endfor %}
</div>
```

`Candidate { short: String, full: String }`，full = base.join(子目录)。

### 4.4 前端 JS（layout.html `<script>` 追加）

- `htmx:afterSettle` 事件委托给 `input[data-complete]` 幂等绑定（`data-complete-bound` 防重复，仿 Sortable 模式），SSE 刷新 main 后重绑不丢。
- keydown：Tab → preventDefault + `htmx.ajax` 拉 complete 到对应 `.complete-panel`，afterSettle 默认高亮首项；↓/↑ → 移动 `.complete-item.active`（循环）；Enter → 有 active 则 preventDefault 补全 + 关候选，无 active 放行提交；Esc → 关候选。
- blur 延迟关候选（`setTimeout` 150ms，允许 mousedown 点候选补全）。

### 4.5 projects_main.html 改动

`#path` / `#dir` 各包一层 `<div class="input-wrap">`（`position:relative`），输入框加 `data-complete="complete-path"` / `"complete-dir"`，下方加 `<div class="complete-panel" id="complete-path|dir"></div>`。

### 4.6 补全样式（app.css 新增）

```css
/* ---------- 路径输入框 Tab 补全 ---------- */
.input-wrap { position: relative; display: inline-flex; align-items: center; }
.complete-list {
  position: absolute; top: calc(100% + 2px); left: 0;
  min-width: 100%; max-width: 440px;
  max-height: 240px; overflow: auto;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 6px;
  box-shadow: var(--shadow);
  z-index: 90;
}
.complete-item {
  padding: 6px 12px;
  font-family: var(--mono); font-size: 12.5px; color: var(--ink);
  cursor: pointer;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.complete-item:hover, .complete-item.active { background: var(--accent-soft); color: var(--accent); }
```

绝对定位不顶页面；`z-index:90`（< 浏览浮层 100，互不同时出现）。

## 5. 改动清单

| 文件 | 改动 |
|---|---|
| `templates/fragments/browse.html` | 重写为 overlay + modal 结构；顶层去 id；「进入 / 上级」hx-target 指挂载点；加 ✕ 关闭按钮 |
| `templates/fragments/complete.html` | **新增**：Tab 补全候选列表片段（`.complete-list`，每项 `data-path` 存完整路径） |
| `static/app.css` | 新增 `.browse-*`（浮层 + 滚动列表）+ `.input-wrap / .complete-list / .complete-item(.active)`（补全 dropdown）样式 |
| `templates/layout.html` | `<script>` 追加：浮层关闭事件委托 + Tab 补全键盘逻辑（Tab/↓/↑/Enter/Esc + afterSettle 幂等重绑） |
| `templates/fragments/projects_main.html` | `#path` / `#dir` 各包 `.input-wrap` + 下方加 `.complete-panel`；输入框加 `data-complete=<panel-id>` |
| `templates/fragments/browse_select.html` | 不动（panel 仍指挂载点 id，行为一致） |
| `src/routes/projects.rs` | **新增** `complete` handler（拆 base/prefix + 复用 `list_subdirs` + 前缀过滤 + 渲染 complete.html）+ `CompleteTpl` / `CompleteQuery` / `Candidate`；注册 `GET /{token}/projects/complete` 路由 |

## 6. 边界与不变量（遵守 CLAUDE.md §7.5 / frontend-rules）

- 业务逻辑只在 core：browse / complete handler 都是薄壳（路径解析 + 列目录是 server 层数据获取，非业务逻辑），浮层关闭 / 补全键盘纯前端 UI，不碰 core。
- htmx 服务端渲染片段优先：数据交互（进入 / 选定 / 上级 / 补全拉候选）走 htmx；仅关闭 / 键盘高亮用原生 JS（允许的轻量增强）。
- 片段外层 id 固定：挂载点 `#browse-panel-*` / `#complete-*` id 不随内容变。
- 写操作返回完整页：本次无写操作（browse / complete 都是 GET 片段），不涉及。
- 路径不硬编码：模板用 `{{ token }}`，handler 用 `resolve_dir` / `dirs::home_dir()`。
- 改模板 / 静态资源后跑 `make check`（Askama 模板错只有 check 能暴露）。

## 7. 测试要点

- `make check` 全绿（askama 编译 + clippy `-D warnings` + test）——必跑。
- 浏览浮层走查（`make run ARGS="serve --port 7317"`）：
  - 注册表单点「浏览...」→ 浮层居中弹出 + 遮罩；目录列表超出高度时滚动。
  - 「进入」子目录 → 浮层内列表更新，不闪不重开。
  - 「↑上级」→ 回上级；「✓选定」→ 输入框回填 + 浮层消失。
  - ✕ / 点遮罩 / ESC → 浮层关闭。
  - 扫描表单「浏览...」同理（独立浮层实例，样式共用）。
- Tab 补全走查：
  - `#path` 输入 `/Users/mywo/la` 按 Tab → 下方列 `la` 开头子目录候选，默认高亮首项。
  - ↓/↑ 循环移动高亮，回车 → 输入框补全为高亮候选完整路径（带尾斜杠）+ 候选关闭。
  - 补全后再 Tab → 列新路径子目录（逐级续补）。
  - Esc / 失焦 → 候选关闭；候选为空时 Enter 正常提交表单。
  - `#dir` 同理。
- 可选 e2e：`e2e/` 加用例覆盖「浏览浮层打开→选定」和「Tab 补全→回车」两条链路，断言输入框值。当前 e2e 不覆盖 projects 视图，属增量覆盖，主人定是否加。

## 8. 风险与回退

- 「进入」hx-target 改指挂载点（原指 panel 自身 id）——若漏改，点进入后 hx-target 找不到节点、列表不更新。缓解：browse.html 顶层明确无 id，所有 target 统一 `#{{panel}}`；`make check` + 手动走查覆盖。
- 浮层关闭事件委托误伤：`.browse-overlay` 内点击（进入 / 选定 / 上级）不应触发关闭——`e.target === overlay` 只匹配遮罩本身，modal 内点击不关。已处理。
- Tab 补全 Enter 与表单提交冲突：候选存在且有 active 时 Enter 补全（preventDefault），无候选 / 无 active 放行提交。补全后候选自动关，再 Enter 即提交，流程顺。
- afterSettle 重绑防漏：SSE 刷新 main 后 `#path` / `#dir` DOM 重建，靠 `data-complete-bound` 标记幂等重绑，仿现有 Sortable 模式。
- 回退：改动集中在 browse.html / complete.html / app.css / layout.html(script) / projects_main.html / projects.rs(complete handler) 六处，`git checkout` 即可还原，无数据 / 状态迁移。

## 9. 实现期增量

实现期主人补充两块需求 + 修复一个浮层化引入的 bug，均在 commit cc06909 落地。

### 9.1 scan 浮层 + toggle（补充需求）

- scan_results.html 改浮层（复用 `.browse-overlay/modal`，加 `.scan-flyout` 标识区分）。
- 候选按 canonical 全路径精确匹配标记「已注册」（scan handler load projects 建 path set，候选 canonicalize 后比对 `project.path`）。
- 新增 toggle 端点 `POST /{token}/projects/toggle`：canonical 匹配，已注册→注销、未注册→注册，返回新按钮片段（`fragments/scan_toggle.html`，`hx-swap=outerHTML` 替换 form）。连续 toggle 多个，浮层保持。
- 连续操作保活：scan 浮层开时前端跳过 SSE 整页刷新（注册/注销写 toml 触发 SSE，刷 main 会清浮层）；浮层关闭时补刷一次 main 同步底层已注册列表。

### 9.2 hx-swap 继承 bug 修复

- 现象：① 浏览浮层内「进入」无反应；② 选定后关闭再点「浏览」不弹浮层。
- 根因：浏览/扫描按钮在 `<form hx-swap="outerHTML">` 内、自己没写 hx-swap——htmx 的 hx-swap **从最近祖先继承**，按钮继承 form 的 outerHTML，把整个挂载点 `#browse-panel-add`/`#scan-results` 替换成浮层（浮层顶层无 id），挂载点 id 丢失，后续「进入」「再浏览」的 `hx-target="#挂载点"` 找不到目标。浮层化前 browse.html 顶层有 `id={{panel}}` 正好掩盖此 bug，去 id 后暴露。
- 修复：三个触发按钮（注册浏览 / 扫描浏览 / 扫描 form）显式 `hx-swap="innerHTML"`，挂载点保留、浮层作为子节点（fixed 定位照常全屏遮罩）。
- 验证：playwright 复现 Bug1（浏览→进入，cwd 变化 + 挂载点保留）+ Bug2（选定→再浏览，浮层重弹）+ scan 两次扫描；`make check` 全绿。

## 10. 实现期增量 II（commit 7b6f94c）

主人 review 后提的 5 个改进，均在 commit 7b6f94c 落地：

- **browse dir alias**：扫描浏览按钮 `hx-include="#dir"`（input `name=dir`），但 browse 端点解析 `path` 字段 → 值没传进去（列了 home）。修：`BrowseQuery.path` 加 `#[serde(alias = "dir")]`。
- **~ 路径支持**：scan/browse 改用 `resolve_dir`（展开 `~` + canonicalize）。此前 scan handler `StdPath::new(&f.dir)` 不展开 `~`，`~/...` 扫描 0 候选。
- **注册表单简化**：去 agents 输入（用默认 agents）+ 去浏览按钮（浏览由扫描表单承担）。
- **扫描表单简化**：去 depth 输入，固定默认层深 3。
- **注册查重**：add handler 注册前按 canonical path 精确匹配查重，重复→拒绝 + `ProjectsTpl` 顶部 `message` 提示（`render_list` 加 message 参数透传）。
