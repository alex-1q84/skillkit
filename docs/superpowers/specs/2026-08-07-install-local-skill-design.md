# 安装本地 skill 设计（2026-08-07）

> 范围：支持从本地 skill 目录或 zip 文件安装 skill 到 skillkit 管理库（canonical 池），注册为 managed、scope=local；CLI + GUI 双端。
> 上游规范：`CLAUDE.md` §5 约束（canonical 池 / 单版本模型 / 跨实体 id 引用）、§6 CLI 约定、§7.5 前端约定。

## 1. 背景与目标

skillkit 现有两条安装路径：
- `install add <source> <skill>`：从 npx skills（联网）下载到 canonical 池，managed。
- `import-existing`：只读**登记**已散落在 agent 目录（`~/.agents/skills` 等）的 skill，unmanaged、不复制。

缺口：用户手上的 skill 是外部文件形态（本地目录、从 GitHub 下的 zip）时，无处落地。手工复制进 canonical 池不注册 registry，绕过 skillkit 的版本/hash/归属管理；用 `import-existing` 只能登记已在 agent 目录的，且永远 unmanaged。

目标：新增 `install local`，把外部 skill 目录/zip **真正安装**到 canonical 池，算 hash 标记为 managed，作为 local skill 注册，纳入 skillkit 统一管理（profile 归属、project apply、rescope 全部可用）。

非目标（YAGNI）：不做 HTTP 文件上传（GUI 走磁盘路径输入，复用 Projects 浏览模式）；不存多版本；不自动从 GitHub URL 拉 zip（那是 install add 的 source 职责）。

## 2. 边界（与现有命令对比）

| | install add | import-existing | **install local（本次）** |
|---|---|---|---|
| 来源 | npx skills（联网） | 已在 agent 目录的 skill | 外部任意目录/zip |
| 动作 | 下载到 canonical 池 | 只读登记，不复制 | 复制/解压到 canonical 池 |
| 状态 | managed（有 hash） | unmanaged（无 hash） | managed（算 hash） |
| source | 真实 source 名 | 合成 `unmanaged` | 合成 `local` |
| id | `<source>/<skill>` | `unmanaged/<skill>` | `local/<skill>` |

## 3. 核心设计

### 3.1 source / id 模型

本地 skill 无 npx source。沿用 `import-existing` 的合成 source 先例：source 固定为 `local`，id = `local/<name>`，不进 SourcesStore（SourcesStore 是 npx source 的，带 package 字段，强行复用语义别扭）。`local` 作为保留伪 source，与 `unmanaged` 对称。

### 3.2 数据流

```
输入 path（目录 或 .zip）
  ├─ .zip → 解压到 tempfile::TempDir
  └─ 目录  → 直接用
        ↓
定位 skill 目录（resolve_skill_dir）：
  - 根有 SKILL.md → 根即 skill 目录
  - 根无、唯一子目录有 SKILL.md → 该子目录
  - 否则 → AmbiguousSkillArchive 报错
        ↓
读 SKILL.md frontmatter name（read_skill_name）；--name 覆盖；都没有→报错
校验 name 合法（仅 [A-Za-z0-9-_.]，无路径分隔符）→ 非法报错
        ↓
target = ~/.skillkit/.agents/skills/<name>/
  ├─ 已存在且无 --force → SkillAlreadyInstalled（引导 --force）
  └─ --force → 删旧 target
        ↓
copy_skill_dir 递归复制 → target
        ↓
hash_skill_dir 算 sha256（目录树确定性排序遍历）→ computed_hash
        ↓
registry upsert SkillMeta{
  id: local/<name>, source:"local", scope, computed_hash:Some(hash),
  version:None, canonical_path: target
}
        ↓
scope==Global → ensure_global_claude symlink（默认 local 跳过）
```

### 3.3 skill 目录布局兼容

zip/目录两种常见形态都支持：
- (a) 根直接是 skill 目录（含 SKILL.md）——自己打包的 skill。
- (b) 解压后是单层父目录，里面才是 skill——GitHub `repo-main.zip` 下载形态。

`resolve_skill_dir`：根有 SKILL.md 取根；否则若根下唯一子目录有 SKILL.md 取该子目录；其余（多个顶层条目且无根 SKILL.md、无 SKILL.md）报错。zip 解压后用同一逻辑。

### 3.4 name 派生与校验

- 优先序：`--name`（CLI）/ GUI name 输入框 > SKILL.md frontmatter `name` 字段。
- 都没有 → `SkillNameMissing` 报错。
- name 合法性：仅 `[A-Za-z0-9-_.]`，含路径分隔符或空 → 拒绝（防 canonical 池路径逃逸）。

### 3.5 hash 与 version

- **computed_hash**：确定性 sha256。按相对路径升序遍历 skill 目录所有文件，对每个文件依次向一个 `Sha256` 写入「相对路径字节（UTF-8）+ 文件内容字节」，取最终摘要。保证「同内容同 hash、内容变 hash 变、与遍历顺序无关」。用 `sha2` crate。
- **version**：留 `None`。SKILL.md frontmatter 不强求 version 字段；YAGNI，未来按需加 `--version` / 读 frontmatter。

## 4. CLI 接口

```
skillkit install local <path> [--name <n>] [--scope global|local] [--force] [--json]
```

- `<path>`：skill 目录或 .zip 文件（绝对/相对/`~/`，用 `resolve_dir` 展开）。
- `--name`：覆盖 skill 名（默认读 SKILL.md frontmatter）。
- `--scope`：默认 `local`（per 需求）；`global` 额外 symlink 落地。
- `--force`：target 已存在时覆盖（删旧重装 + 重算 hash）。
- `--json`：输出 SkillMeta，schema 与现有 install 一致（公开契约锁定）。
- 成功输出（人看）：`已安装 local/<name> → ~/.skillkit/.agents/skills/<name>/（sha256: <短hash>）`。

## 5. GUI 接口

Skills 页加「安装本地 skill」入口 → 复用 Projects 的路径输入 + 浏览浮层模式（通用路径补全，不开 multipart）：

- 浮层表单字段：`path`（路径输入 + 浏览按钮，复用 browse/complete 设施）、`name`（可选，默认读 frontmatter）、`scope`（local/global 下拉，默认 local）、`force`（勾选）。
- 端点：`POST /{token}/skills/install-local`（form-urlencoded），调 core `install_local`，成功返回完整页面（`hx-target="body" hx-swap="outerHTML"`）+ SSE 刷新；失败返回带 message 的浮层（保留输入）。
- 复用 Projects 已有 `browse.html` 浮层 + `complete` Tab 补全（路径补全是通用的，不绑 project 语义）。

## 6. 错误处理（反馈引导行动）

| 场景 | 文案 |
|---|---|
| 路径不存在 / 非 zip 非目录 | `本地 skill 源无效：<path>（需是含 SKILL.md 的目录或 .zip）` |
| zip 损坏 | `解压失败：<zip>（文件损坏或非 zip）` |
| 无 SKILL.md | `<path> 不是合法 skill：未找到 SKILL.md` |
| zip/目录多义 | `未明确 skill 根：<path> 下有多个目录且根无 SKILL.md，请直接传 skill 目录路径` |
| name 缺失 | `无法确定 skill 名：SKILL.md 缺 name 字段且未传 --name` |
| name 非法 | `skill 名非法：<name>（仅允许字母数字 - _ .）` |
| target 已存在（无 --force） | 复用 `SkillAlreadyInstalled`，补「加 --force 覆盖」 |

新增 `SkillkitError` 变体（合并成单变体带 reason，简洁）：`InvalidLocalSkill { path, reason }`（覆盖源无效 / 无 SKILL.md / name 缺失 / name 非法）；`AmbiguousSkillArchive { reason }`（多义布局）；zip 解压失败映射到 `InvalidLocalSkill`（reason=解压失败）。

## 7. 组件与依赖

- **core 新模块 `install_local.rs`**：`pub fn install_local(paths, src_path, name: Option, scope, force) -> Result<SkillMeta>` + 私有 `resolve_skill_dir` / `read_skill_name` / `hash_skill_dir` / `copy_skill_dir`。独立模块（install.rs 是 npx 路径，职责分离）。lib.rs re-export。
- **`read_skill_name`**：手写极简 frontmatter name 提取（按行匹配 `^name:\s*(.+)`，trim + 去引号）。skill name 是 kebab-case 标识符，YAML 复杂值不会出现；零依赖比引 yaml crate 更稳。
- **新增依赖（core，均 pure rust，不破坏零运行时依赖）**：
  - `zip = "2"`：zip 解压。
  - `sha2 = "0.10"`：sha256。
- **error.rs**：加 `InvalidLocalSkill` / `AmbiguousSkillArchive` 变体。
- **cli**：`install.rs` 加 `Local` 子命令（`Add` 的兄弟）。
- **server**：`routes/skills.rs` 加 `install_local` handler + 模板浮层（fragments/install_local.html）。

## 8. 测试

原则：验证业务结果（install 后 canonical 池落地正确 + registry managed + 可被 profile/project 引用），不验证内部函数。

**core 单元（纯逻辑）**：
- `resolve_skill_dir`：根有 SKILL.md / 唯一子目录有 SKILL.md / 多子目录无根 → 报错 / 无 SKILL.md → 报错。
- `read_skill_name`：frontmatter 有 name / `--name` 覆盖 / 都无 → 报错 / name 含分隔符 → 拒绝。
- `hash_skill_dir` 确定性：同内容同 hash、内容变 hash 变。

**core 集成（tempdir 全流程 install→registry→canonical 池）**：
- 装目录：canonical 池落地 + registry `computed_hash` 有值（managed）+ scope local、无 symlink。
- 装 zip 两布局（根即 skill / 单层父目录）。
- 冲突：已存在 → SkillAlreadyInstalled；`--force` 覆盖且 hash 更新。
- `--json` schema 锁定 SkillMeta（id=local/<name>, source=local）。
- scope global：额外 `~/.agents/skills` symlink 落地。

**server**：`POST /skills/install-local`（目录成功重定向 / zip 成功 / 无 SKILL.md message / 冲突 message）+ 浮层渲染 200。

## 9. 关键决策与否定备选

- **伪 source `local`（不进 SourcesStore）**：与 `unmanaged` 对称，id 契约不变，最小改动。否定：注册成 SourcesStore 真 source（语义别扭，SourcesStore 带 package 字段）；用路径/文件名做 source（id 不稳定）。
- **独立 core 模块 `install_local.rs`**：install.rs 是 npx 委托路径，本地装是自复制 + 自算 hash，职责不同，分离清晰。否定：塞进 install.rs（混两条路径）；复用 `install()` 旁路 npx（npx::add 与本地复制无关）。
- **手写 frontmatter name 提取**：name 是 kebab-case 标识符，极简行匹配够稳，零依赖。否定：引 serde_yaml（单字段杀鸡用牛刀）。
- **GUI 走路径输入而非 HTTP 上传**：复用 Projects 浏览模式，免 multipart，UX 一致。否定：multipart 文件上传（server 无该特性，且路径输入更契合「装本地磁盘文件」）。
- **zip 布局兼容两种**：自己打包 / GitHub 下载是主流两种形态，自动识别省心。多义时报错而非猜（不静默）。
- **`--force` 覆盖**：本地装常用于迭代更新 skill 内容，需覆盖；默认报错防误删。

## 10. 后续提醒

- `local` 伪 source 与 `unmanaged` 一样，是保留名；未来若加「自定义 source 名」，注意避让。
- `hash_skill_dir` 的确定性算法（排序 + 路径参与）是版本比对的基础，改了会让所有 local skill「看似漂移」，变更需谨慎。
- GUI 浮层复用 Projects 的 browse/complete，若后续把路径补全抽成通用中间件，本次入口一并迁移。
