# SkillKit GUI Demo 设计

- 日期：2026-07-29
- 状态：待评审
- 目的：用可交互的单文件 HTML 原型，让主人对 skillkit GUI 设计做最终 review

## 1. 目的与定位

进入 M0 实现计划前，先用一个可点击的 GUI 原型验证设计：四大视图的信息架构是否合理、Projects 的 apply 闭环交互是否清晰。这是"设计 review"工具，不是生产实现——聚焦信息架构与交互流程，不追求真实后端逻辑。

## 2. 形态

单文件 `index.html`（内嵌 CSS + JS），双击即开，零依赖零启动成本。mock 数据内嵌为 JS 对象。tab/hash 路由切换四大视图。

## 3. 交互深度：核心交互

- Sources / Skills / Profiles：展示为主（数据、布局、信息密度可 review），操作按钮可见但不深入响应。
- Projects：核心可交互——apply 闭环能点通（勾选 skill → APPLY → 看 status diff）。

## 4. 四大视图信息架构

| 视图 | 内容 | demo 交互 |
|------|------|-----------|
| Sources | 源注册表：skills.sh（skills-sh）、team-private（git）、my-local（local）；类型 + URL/path + ref + 源内 skill 数 | 展示为主 |
| Skills | registry 总览表：id ‧ scope ‧ version ‧ commit_sha ‧ source ‧ canonical_path；按 scope / source 筛选 | 展示为主 |
| Profiles | profile 列表（frontend、base），每个展开看 skill id 组成 + 描述 | 展示为主 |
| Projects | 项目卡片 → apply 闭环工作台 | **核心可交互** |

## 5. Projects apply 闭环（核心交互剧本）

点进一个项目后的工作台，三栏布局：

```
┌─ mac-config ────────────[/Users/mywo/lab/mac-config]── agents: claude, cursor ─┐
│  applied_profiles: [frontend, base]                                              │
│                                                                                  │
│  installed_skills (☑=期望落地)          shared (只读 · git 管)                   │
│  ☑ skills.sh/frontend-design    [glo]   · team-shared/lint                      │
│  ☑ skills.sh/dataviz            [glo]   · team-shared/git-hooks                 │
│  ☑ team-private/tdd             [loc]                                            │
│  ☑ team-private/api-spec        [loc]   status (apply 后的 diff)                │
│  ☐ skills.sh/old-thing          [glo]   ─────────────────────────                │
│                                        ✓ in-sync : frontend-design, dataviz    │
│  [+ add-skill]   [apply-profile ▼]     + missing : tdd  (未 install，点此装)    │
│              [ ▶ APPLY ]               - extra   : old-thing (未勾选，将移除)   │
│                                        ! conflict: api-spec sha 漂移           │
└──────────────────────────────────────────────────────────────────────────────────┘
```

剧本：勾选/取消 skill → 点 `APPLY` → 右侧 status 区实时算 diff（missing 补、extra 删、conflict 警告），完整模拟 spec §10.2 幂等落地 + §10.3 冲突检测。这条链验证「profile 粗分类 → project 精确选择 → apply 落地 → status 感知」闭环在 UI 上是否清晰。

三栏语义：
- **installed_skills**：可勾选，决定哪些 skill 落地（spec §9 精确事实）。
- **shared**：只读展示项目 git 里的 shared skill（spec §5/决策 6，skillkit 不管）。
- **status**：apply 后的 diff，对应 `project status --json` 的 `{expected, missing, extra, conflicts}`（spec §11）。expected = 勾选（☑）的 skill，对比 mock 当前落地状态算出 missing（☑ 但未落地）/ extra（未勾选但已落地）/ conflict（sha 漂移）。

## 6. mock 数据

基于 spec §8 数据模型 + 主人真实 skill 分布，造有真实感的数据：

- **sources**：skills.sh（skills-sh）、team-private（git，git@github.com:org/team-skills.git）、my-local（~/my-skills）
- **skills**：从 `~/.agents/` 全局池挑代表性的（frontend-design、dataviz、canvas-design 等 global），加几个 local（team-private/tdd、team-private/api-spec），版本 + commit_sha 齐全
- **profiles**：frontend（frontend-design + dataviz + canvas-design）、base
- **projects**：mac-config、skillkit（含 agents、applied_profiles、installed_skills、locked_shas）

## 7. 视觉风格

亮色、清爽、专业。高信息密度，数据列等宽对齐。frontend-design skill 发挥细节，做出有设计感而非通用 AI 模板的界面。

## 8. 文件结构

单文件 `index.html`，内部组织：

- `<style>` 内嵌 CSS（亮色主题、等宽数据列）
- mock 数据 JS 对象（sources / skills / profiles / projects）
- 视图渲染：四个 render 函数 + tab 路由
- Projects 的 apply/diff 模拟逻辑（勾选状态 → diff 计算 → status 渲染）

## 9. 不在范围内（YAGNI）

- 真实文件操作（symlink / copy / `.git/info/exclude`）——只模拟状态变化
- 真后端 / Axum / SSE / 文件锁
- 真实 install / upgrade / uninstall（Sources / Skills 视图操作只展示）
- 数据持久化（刷新即重置 mock）
- 响应式移动端（桌面 review 为主）
