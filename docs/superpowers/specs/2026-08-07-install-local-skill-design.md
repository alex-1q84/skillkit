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
全流程持 ~/.skillkit/.lock（FileLock）串行化（见 §3.7）

输入 path（目录 或 .zip）
  ├─ .zip → 解压到 tempfile::TempDir（逐条目 enclosed_name 校验 + 体积上限，见 §3.6）
  └─ 目录  → 直接用
        ↓
定位 skill 目录（resolve_skill_dir）：
  - 根有 SKILL.md → 根即 skill 目录
  - 根无、唯一子目录有 SKILL.md → 该子目录
  - 否则 → AmbiguousSkillArchive 报错
        ↓
读 SKILL.md frontmatter name（read_skill_name）；--name 覆盖；都没有→报错
校验 name（见 §3.4）→ 非法报错
        ↓
target = ~/.skillkit/.agents/skills/<name>/
containment 断言：target 必须位于 skillkit_skills_dir() 之内（兜底防逃逸）
        ↓
冲突判定（键 = registry id local/<name>，reg.get；全程持 "registry" 锁见 §3.7）：
  - id 已存在（local/<name> 已装）：无 --force → SkillAlreadyInstalled（引导 --force）
  - id 不存在但 target 目录被占（池按 name 扁平，其他 source 如 skills.sh/foo
    共用 skills/<name>）→ SkillPoolOccupied（引导先 uninstall <owner_id>；
    无 owner 的孤儿目录给手动清理指引，见 §6）
        ↓
复制到暂存目录 skills/.<name>.staging-<pid>/（copy_skill_dir，拒/跳 symlink，见 §3.6）
        ↓
hash_skill_dir(staging) → computed_hash（在 rename 前算，失败只删 staging，缩小就位后失败面）
        ↓
force 归属反查（防跨 source 误删）：扫描 registry，若存在 local/<name> 之外的任何 id
其 canonical_path == target → SkillPoolOccupied 拒绝（防陈旧多 id 指向同目录时误删）
        ↓
原子就位（rename 不能覆盖非空目录，故 force 走三段 move-aside）：
  - 非 force：rename(staging → target)
  - force：rename(target → .old) → rename(staging → target) → remove_dir_all(.old)
           （先移走旧、再换新、最后清旧；target 始终有内容，删除永不先于新内容就位）
        ↓
registry upsert + save（持同一把 "registry" 锁，见 §3.7）
        ↓
两段回滚（不留半删/孤儿）：
  - 就位前失败（复制/hash）→ 删 staging
  - force 已 move-aside 后、就位前失败 → rename(.old → target) 还原
  - 就位后 reg.save 失败 → 非 force 删 target；force 删 target 后 rename(.old→target) 还原
scope==Global → ensure_global_claude symlink（默认 local 跳过）
```

### 3.3 skill 目录布局兼容

zip/目录两种常见形态都支持：
- (a) 根直接是 skill 目录（含 SKILL.md）——自己打包的 skill。
- (b) 解压后是单层父目录，里面才是 skill——GitHub `repo-main.zip` 下载形态。

`resolve_skill_dir`：根有 SKILL.md 取根；否则若根下唯一子目录有 SKILL.md 取该子目录；其余（多个顶层条目且无根 SKILL.md、无 SKILL.md）报错。zip 解压后用同一逻辑。

### 3.4 name 派生与校验（防 canonical 池路径逃逸）

- 优先序：`--name`（CLI）/ GUI name 输入框 > SKILL.md frontmatter `name` 字段。**frontmatter name 与 `--name` 走同一校验**——name 来自不可信 zip（GitHub 下载），不能例外。
- 都没有 → `SkillNameMissing` 报错。
- name 合法性：
  - 拒空、拒 `.`、拒 `..`、拒纯点串（`...`）、拒含 `/` 或 `\`。
  - 字符集仅 `[A-Za-z0-9-_.]`，且不以纯点开头。
- **containment 断言兜底**：`target = pool.join(name)` 后断言 `target.starts_with(pool)` 且相对路径无 `..` 分量。即便校验有遗漏，join 后也绝不让 target 落到池外；`--force` 删除前再断言一次。杜绝 `name='..'` → `remove_dir_all(~/.skillkit/.agents)` 删光整个池。

### 3.5 hash 与 version（防碰撞）

- **computed_hash**：确定性 sha256，防碰撞。按相对路径升序遍历 skill 目录所有文件，对每个文件依次向一个 `Sha256` 写入「`len(path)` ‖ path(UTF-8) ‖ `len(content)` ‖ content」——路径与内容、文件与文件之间用长度前缀定界。杜绝树 A `{a:"bc", d:""}` 与树 B `{ab:"c", d:""}` 喂出同一字节流 `abcd` 的碰撞（computed_hash 是漂移/版本比对基础，碰撞=内容已变却判定未漂移、静默用旧内容）。用 `sha2` crate。
- **version**：留 `None`。SKILL.md frontmatter 不强求 version 字段；YAGNI，未来按需加 `--version` / 读 frontmatter。

### 3.6 安全约束（不可信输入：zip / 外部目录）

输入含从 GitHub 等下载的 zip 与外部目录，是不可信信任边界，须主动设防：

- **zip 条目路径穿越（ZipSlip）**：逐条目用 `enclosed_name` 包含性校验，拒绝 `..` 分量、绝对路径、symlink 条目；只接受相对 tempdir 根的安全路径。
- **symlink**：`copy_skill_dir` / `hash_skill_dir` 不跟随 symlink，遇 symlink 条目跳过或拒绝。对齐 `import.rs:129` 的 `skip_symlink` 约定（仓库已认定 symlink 非合法 skill 内容），防止把 `~/.ssh`、dotfiles 等池外文件拷入 canonical 池并被 hash 收录、机密外泄。
- **体积上限**：解压累计总字节数 + 文件数上限（防 zip bomb 打满磁盘），超限报错。

### 3.7 原子性与并发

- **持锁 key = "registry"，全程**：install_local 从 `reg.load` 到 `reg.save` 全程持 `FileLock(paths, "registry")`（lock.rs 是 per-key：`~/.skillkit/.lock/registry.lock`）。复用 "registry" key 让 install_local 与所有 registry 写入方（`Registry::save` 持同 key）串行，闭掉「install_local 的 load→save 窗口内其他 skill 的 registry 条目被 lost-update」。
  - **实现注意（同进程自死锁）**：`Registry::save` 内部也 acquire "registry"。install_local 已持锁时若再走 save 的加锁路径会**同进程 flock 自死锁**——须用「不重复加锁的 save 变体」（或 save 接受外部已持锁），由实现负责。
- **原子就位（force 三段 / 非 force 单段）**：见 §3.2。force 走 `target→.old → staging→target → rm .old`，删除永不先于新内容就位，失败可 move-aside 还原。hash 在 rename 前（对 staging 算），把就位后失败面缩到只剩 `reg.save`。
- **已知限制（诚实声明，本功能不全包）**：池竞争分两类——
  - (a) install_local 与其它 **registry 写入方**：已由 "registry" key 串行（闭 lost-update）。
  - (b) install_local 与**池目录物理变更方**（uninstall 删目录、rescope rename 目录、install add 的 npx 写）：这三者今天**不持任何锁**（grep 证实 `FileLock::acquire` 只在 source/config/registry/profile/project 各 save，既有债），不与 install_local 串行——并发 `skillkit skill remove skills.sh/foo` 仍可能在 install_local 持锁期间删掉 `skills/foo`。彻底闭需把所有池写入方纳入同一把池级锁，**超出本功能范围**，记为后续工作。本功能只保证 install_local 自身原子 + registry 一致，不声称串行化全部池竞争。

## 4. CLI 接口

```
skillkit install local <path> [--name <n>] [--scope global|local] [--force] [--json]
```

- `<path>`：skill 目录或 .zip 文件（绝对/相对/`~/`，用 `resolve_dir` 展开）。
- `--name`：覆盖 skill 名（默认读 SKILL.md frontmatter）。
- `--scope`：默认 `local`（per 需求）；`global` 额外 symlink 落地。
- `--force`：仅当 registry id `local/<name>` 已存在时覆盖（删旧重装 + 重算 hash，删前校验目录归属同一 id 且在池内）；若 target 被其他 source（如 `skills.sh/foo`）占用，拒绝并提示先 uninstall 该 id，绝不跨 source 误删。
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
| name 非法（空 / `.` / `..` / 纯点 / 含分隔符） | `skill 名非法：<name>（仅允许字母数字 - _ .，不能是 . / .. / 纯点或含路径分隔符）` |
| containment 断言失败（target 落池外） | `target 越界 <target>，已拒绝（不应发生，请报 bug）` |
| target 已存在（无 --force） | 复用 `SkillAlreadyInstalled`，补「加 --force 覆盖」 |
| target 被其他 source 占用（如 `skills.sh/foo`） | `<name> 已被 <owner_id> 占用，先 skillkit skill remove <owner_id> 再装（--force 不跨 source 删）` |
| target 被占但无 registry owner（孤儿目录） | `<name> 目录存在但无 registry 记录（孤儿），请手动删除 <target> 后重试` |
| zip 不安全条目（`..` / 绝对路径 / symlink） | `zip 含不安全条目：<entry>（拒绝路径穿越/symlink）` |
| zip 解压超体积/条目上限 | `zip 体积/文件数超限：<zip>（疑似 zip bomb）` |

新增 `SkillkitError` 变体：`InvalidLocalSkill { path, reason }`（源无效 / 无 SKILL.md / name 缺失 / name 非法 / zip 损坏 / 不安全条目 / 超体积）；`AmbiguousSkillArchive { reason }`（多义布局）；`SkillPoolOccupied { name, owner_id: Option<String> }`（target 被占；`Some` 引导 uninstall，`None` 为孤儿目录给手动清理指引）。

## 7. 组件与依赖

- **core 新模块 `install_local.rs`**：`pub fn install_local(paths, src_path, name: Option, scope, force) -> Result<SkillMeta>` + 私有 `resolve_skill_dir` / `read_skill_name` / `validate_name`（拒 `.`/`..`/纯点/分隔符 + containment 断言）/ `hash_skill_dir`（长度前缀）/ `copy_skill_dir`（拒 symlink）/ `extract_zip`（enclosed_name + 体积上限）。全流程持 `FileLock`；复制走暂存目录 + 原子 rename。独立模块（install.rs 是 npx 路径，职责分离）。lib.rs re-export。
- **`read_skill_name`**：手写极简 frontmatter name 提取（按行匹配 `^name:\s*(.+)`，trim + 去引号）。skill name 是 kebab-case 标识符，YAML 复杂值不会出现；零依赖比引 yaml crate 更稳。frontmatter name 与 `--name` 都过 `validate_name`。
- **`validate_name` + containment**：拒空 / `.` / `..` / 纯点 / 含 `/` 或 `\`；`pool.join(name)` 后断言 `starts_with(pool)` 且相对路径无 `..` 分量；`--force` 删除前再断言。
- **新增依赖（core，均 pure rust，不破坏零运行时依赖）**：
  - `zip = "2"`：zip 解压（用其 `enclosed_name` 做 ZipSlip 防御）。
  - `sha2 = "0.10"`：sha256（长度前缀框架）。
- **error.rs**：加 `InvalidLocalSkill` / `AmbiguousSkillArchive` / `SkillPoolOccupied` 变体。
- **cli**：`install.rs` 加 `Local` 子命令（`Add` 的兄弟）。
- **server**：`routes/skills.rs` 加 `install_local` handler + 模板浮层（fragments/install_local.html）。

## 8. 测试

原则：验证业务结果（install 后 canonical 池落地正确 + registry managed + 可被 profile/project 引用），不验证内部函数。

**core 单元（纯逻辑）**：
- `resolve_skill_dir`：根有 SKILL.md / 唯一子目录有 SKILL.md / 多子目录无根 → 报错 / 无 SKILL.md → 报错。
- `read_skill_name`：frontmatter 有 name / `--name` 覆盖 / 都无 → 报错。
- `validate_name`（防逃逸，对抗）：拒空 / `.` / `..` / `...` 纯点 / 含 `/` 或 `\`；**frontmatter 里恶意 name（`..`、`a/b`）也拒**（不因来自 zip 而豁免）；join 后 containment 断言 `target` 必在池内。
- `hash_skill_dir`（防碰撞，对抗）：同内容同 hash、内容变 hash 变；**构造碰撞对抗**——树 `{a:"bc", d:""}` 与 `{ab:"c", d:""}` 必须 hash 不同。

**core 集成（tempdir 全流程 install→registry→canonical 池）**：
- 装目录：canonical 池落地 + registry `computed_hash` 有值（managed）+ scope local、无 symlink。
- 装 zip 两布局（根即 skill / 单层父目录）。
- 冲突：同 id 已存在 → SkillAlreadyInstalled；`--force` 覆盖且 hash 更新。
- **跨 source 占用 + force 归属反查（防误删）**：先装 `skills.sh/foo` 占池 `skills/foo`，再 `install local foo` → SkillPoolOccupied(Some) 引导 uninstall `skills.sh/foo`；`--force` 也拒。陈旧态（registry 多 id 同指 `skills/foo`）下 force 仍拒。
- **孤儿目录**：target 被占但无任何 registry id 指向（模拟 rename 后 reg.save 失败遗留）→ SkillPoolOccupied(None) 给手动清理指引。
- zip 安全（对抗）：含 `../`/绝对路径/symlink 条目的 zip → 拒（InvalidLocalSkill）；超体积/条目上限 → 拒（防 zip bomb）。
- `copy_skill_dir` 跳过 symlink（对齐 import.rs），不把池外文件拷入。
- **force 三段原子**：注入 `staging→target` 之间失败 → `rename(.old→target)` 还原、registry 不变、target 内容回旧；非 force rename 失败 → 删 staging、target 不存在。
- **就位后 reg.save 失败**：非 force → 删 target（不留孤儿）；force → 删 target 后还原 .old。
- **lost-update**：install_local 持 "registry" 锁期间，另一 registry 写入（改其他 skill）被串行，install_local 的 save 不覆盖其条目。
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

### 审查驱动加固（codex 对抗审查 2026-08-07，评审存 `docs/superpowers/reviews/`）

- **name 校验 + containment 断言（防池路径逃逸）**：正则 `[A-Za-z0-9-_.]` 会放行 `.`/`..`，`name='..'` + `--force` 可 `remove_dir_all` 删光整个 canonical 池。修：显式拒 `.`/`..`/纯点/分隔符，frontmatter 恶意 name 同样拒；join 后 `starts_with(pool)` containment 断言兜底，force 删除前再断言。
- **冲突键改 registry id（防跨 source 误删）**：池按 name 扁平，`skills.sh/foo` 与 `local/foo` 共用 `skills/foo`。按目录名判冲突 + `--force` 删目录会误删其他 source 的 canonical 并留悬空引用。修：冲突键 = `local/<name>`（reg.get），删前校验目录归属同一 id，被其他 id 占用则拒（`SkillPoolOccupied`）引导 uninstall。
- **zip/symlink 安全边界（防路径穿越/机密外泄/zip bomb）**：输入含 GitHub 下载的 zip（不可信）。修：逐条目 `enclosed_name` 校验拒 `..`/绝对路径/symlink；`copy_skill_dir`/`hash_skill_dir` 不跟随 symlink（对齐 `import.rs` 既有约定）；解压设体积/条目上限。
- **hash 长度前缀（防碰撞）**：无定界的「路径+内容」拼接可构造碰撞（`{a:"bc"}` vs `{ab:"c"}`），computed_hash 是漂移检测基础，碰撞=静默用旧内容。修：`len(path)‖path‖len(content)‖content` 长度前缀框架。
- **文件锁 key="registry" + force 三段原子（防半删/孤儿/lost-update）**：复审发现首版「先删后 rename」非原子（删后崩溃留半删）、锁 key 未指定且 `Registry::save` 同进程自死锁、force 缺归属反查、就位后失败留孤儿。修：install_local 全程持 "registry" 锁（闭 lost-update，save 用不重复加锁变体防自死锁）；force 走 `target→.old → staging→target → rm .old` 三段原子（删除不先于新内容就位，失败可还原）；hash 移到 rename 前缩小就位后失败面；两段回滚不留孤儿。诚实声明：池物理变更方（uninstall/rescope/install-add 不持锁）与 install_local 不串行是既有债，全闭需池级共享锁，超出本功能范围。

## 10. 后续提醒

- `local` 伪 source 与 `unmanaged` 一样，是保留名；未来若加「自定义 source 名」，注意避让。
- `hash_skill_dir` 的长度前缀框架（`len‖path‖len‖content`）是版本比对的基础，改了会让所有 local skill「看似漂移」，变更需谨慎且补碰撞对抗用例。
- 安全约束（name containment / zip enclosed_name / symlink 拒绝 / 体积上限）是 GitHub zip 这一信任边界的承重墙，动它们前先评估逃逸/外泄/DoS 风险，勿为便利放松。
- GUI 浮层复用 Projects 的 browse/complete，若后续把路径补全抽成通用中间件，本次入口一并迁移。
