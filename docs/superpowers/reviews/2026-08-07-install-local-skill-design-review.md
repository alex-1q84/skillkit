# Codex 对抗审查：install-local-skill spec（2026-08-07）

> 审查对象：`docs/superpowers/specs/2026-08-07-install-local-skill-design.md`
> 工具：`/codex:adversarial-review`（codex-companion.mjs）
> 结论：needs-attention——不建议按当前设计直接进实现，至少修 finding 1/2/3。
> 说明：codex 返回的 JSON 带 markdown 代码围栏致结构化解析失败，下方为原始评审内容（已核实技术断言均成立）。

## Verdict

`needs-attention`

不建议按当前设计进入实现。四个问题会直接造成数据丢失或安全边界突破：
1. name 校验允许 `.`/`..`，target 拼出 canonical 池外路径，`--force` 可删掉整个池（`~/.skillkit/.agents`）；
2. 存在性检查按池目录 `<name>` 而非 registry id `local/<name>`，`--force` 会删掉同名的其他 source（如 `skills.sh/foo`）的 canonical，留下悬空 registry 引用；
3. zip 解压与递归复制没有任何路径穿越/symlink/体积约束，对 GitHub zip 这一信任边界敞开任意写与机密外泄口；
4. hash 算法把「路径+内容」无分隔拼接进一个 Sha256，可构造碰撞，漂移/版本比对会漏报。

另有 `--force` 覆盖非原子、check-then-act 在文件锁外，失败留半删/孤儿/并发交错状态。至少修 1/2/3 再进实现。

## Findings

### 1. [critical] name 校验允许 `.`/`..` → canonical 池路径逃逸，`--force` 可删除整个池

§3.4 规定 name 仅允许 `[A-Za-z0-9-_.]` 且「含路径分隔符或空 → 拒绝」，该正则放行 `.` 与 `..`（无路径分隔符）。§3.2 `target = ~/.skillkit/.agents/skills/<name>/`：`name='..'` 时 target 解析为 `~/.skillkit/.agents`（池的父目录），`name='.'` 时为池本身。`--force` 分支执行「删旧 target」（remove_dir_all），`name='..'` 会直接删除整个 canonical 池（全部已安装 managed skill），`name='.'` 会删除池本身；非 `--force` 时文件被复制到 `.agents` 根而非 `skills/<name>`，registry 记录 canonical_path 指向错误位置，apply/rescope 全部失效；`scope=global` 时 `ensure_global_claude` 还会把 `~/.agents/skills/..`（即 `~/.agents`）建 symlink。name 来源包含 SKILL.md frontmatter（用户从 GitHub 下载的 zip，不可信输入），因此可被恶意/损坏的 skill 触发。文档第 77 行声称「防 canonical 池路径逃逸」，但恰好放行了最危险的两个名字。

修复：显式拒绝 `.`/`..`（及所有不以字母数字开头的纯点串），并在 join 后用 containment 断言（`target.starts_with(skillkit_skills_dir())`）兜底；frontmatter 派生的 name 与 `--name` 走同一校验。

### 2. [high] `--force` 以池目录 `<name>` 为键而非 registry id，可删除其他 source 的 canonical 并留下悬空引用

§3.2/§4 的冲突判定只看池目录 `~/.skillkit/.agents/skills/<name>` 是否存在，`--force` 直接「删旧 target」。但池目录名按 skill name 全局唯一（现有 install.rs 对 npx 路径同样只查 `target.exists()`），registry id 却是 `<source>/<name>`。真实场景：`skills.sh/foo`（managed，canonical 在池 `skills/foo`）已安装，用户再执行 `install local foo.zip --force` → 删掉 `skills.sh/foo` 的 canonical，registry 里 `skills.sh/foo` 仍存在但 canonical_path 悬空；已锁该 skill 的 project apply 会从缺失目录复制/建链失败，status 全量报漂移。同理 `unmanaged/foo` 被 rescope 到 local 后 canonical 迁入池 `skills/foo`，再 force 装 `local/foo` 会删掉它。现有代码只有 uninstall 走 registry（`computed_hash.is_some()` 才删），本设计把「按名删目录」引入到非卸载路径，是新增的数据丢失面。

修复：冲突键改为 registry id `local/<name>`（reg.get 判定）；force 删除前用 name 反查 registry，若该目录属于其他 id（`skills.sh/foo` 等）则拒绝并提示先 uninstall。

### 3. [high] zip 解压与递归复制未规定安全约束：路径穿越、symlink 跟随、无大小上限

§3.2/§3.3 只说「.zip → 解压到 tempfile::TempDir」与「copy_skill_dir 递归复制」，未规定任何条目约束：未要求对 zip 条目做 enclosed_name 式 containment 检查（`../`、绝对路径条目若解压实现不防御，可写出 tempdir 到用户目录任意位置）；未规定 zip 内 symlink 条目及 skill 目录内 symlink 的处置（递归复制若跟随 symlink，可把 skill 之外的任意文件——如 `~/.ssh`、dotfiles——拷入 canonical 池并被 hash 收录）；未规定解压体积/条目数上限（zip bomb 可打满磁盘）。spec 自身把「从 GitHub 下载的 zip」列为输入形态，这正是信任边界。代码库已有先例：import.rs 显式跳过 symlink 目录，说明仓库约定 symlink 不属于合法 skill 内容，本设计未延续该约定。

修复：规定解压逐条目用 enclosed_name 校验（拒绝非 enclosed 与 symlink 条目），copy/hash 阶段拒绝或跳过 symlink（对齐 import.rs），并设总解压体积与条目数上限。

### 4. [medium] hash 算法将「路径+内容」无分隔拼接进单一 Sha256，可构造碰撞导致漂移检测漏报

§3.5：对每个文件向同一个 Sha256 依次写入「相对路径字节 + 文件内容字节」，路径与内容之间、文件与文件之间均无长度前缀或分隔。可构造两个不同的目录树得到同一摘要，例如树 A `{a:'bc', d:''}` 与树 B `{ab:'c', d:''}` 都会喂出字节流 `abcd`。computed_hash 是版本比对/漂移检测的基础（apply.rs 的 compute_diff 用 locked_shas vs computed_hash 判定冲突；copy 模式用 `.skillkit-sha` 判断副本是否过期；§10 自述「该算法是版本比对的基础」），碰撞意味着内容已变但系统判定未漂移、项目不重新落地，静默地用旧内容。

修复：每条记录加长度前缀（`len(path)||path||len(content)||content`），或改为先对每个文件独立 hash、再对排序后的 `(path, file-hash)` 列表求 hash。

### 5. [medium] `--force` 覆盖非原子且 check-then-act 在文件锁外，失败留半删/孤儿/并发交错状态

§3.2 流程为：`target.exists()` 检查 →（force）删旧 target → copy_skill_dir 递归复制 → hash → registry upsert，除 registry save 自带锁外，检查与物理操作都不在 `~/.skillkit/.lock` 内（CLAUDE.md §3 承诺并发写用该锁串行化）。失败路径：(a) 删后复制前崩溃/磁盘满 → 池中条目缺失而 registry 仍引用旧 canonical_path，所有已锁该 skill 的 project 立即漂移；(b) 复制成功但 hash/registry 写失败 → 池里留下无 registry 的孤儿目录，且下次 install 同名校验「已存在」又要 `--force` 重删；(c) CLI 与 GUI 并发装同名 → 两个进程都通过 exists 检查后交错复制，池被污染。现有 npx install 也有 check-then-act，但无删除步骤；本设计新增「先删后拷」使破坏窗口更大且用户可见。

修复：整个 install（检查/删/拷/hash/upsert）持 `~/.skillkit/.lock`；先复制到暂存目录（`skills/.<name>.tmp-<pid>`）再原子 rename 进池；任一步失败删除暂存目录回滚。

## Next Steps

1. 修 1/2（name 校验 + registry-id 冲突键）后再评审数据流，确保 `--force` 不可能删到池外或跨 source canonical。
2. 补 zip 解压/复制安全约束（enclosed_name、symlink 拒绝、体积上限）到设计并加对抗单测。
3. 改 hash 算法为长度前缀框架，并加「不同树不同 hash」用例。
4. 将 install 全流程纳入文件锁 + 暂存目录原子 rename。
5. 当前沙箱只读，无法落盘：获得写权限后按仓库惯例把本评审存为 `docs/review/2026-08-07-install-local-skill-design-spec-review.md`。

---

## 第二轮（修订后复审，2026-08-07）

> 复审对象：按第一轮 5 条发现修订后的 spec。结论 `needs-attention` 但收窄：原 finding 1-4（name 逃逸 / 跨 source 冲突键 / zip-symlink / hash 长度前缀）方向正确且基本到位；剩余 4 个新发现**全集中在原 finding 5（原子/并发）只修了一半**。

### Verdict

`needs-attention`（收窄到原子性/归属/回滚域）。

### Findings（第二轮）

#### F1 [high] --force 仍是非原子「先删后 rename」，崩溃/失败窗口留半删 canonical

§3.2 force 分支 = remove_dir_all(target) 再 rename(staging→target)。rename(2) 无法覆盖非空目录，故设计选先删后换——非原子。窗口内（remove_dir_all 后、rename 前）任何失败（崩溃/磁盘满/权限/中途失败）都留「registry 仍引用旧 hash、canonical 缺失/半删」；apply.rs 用 canonical_path 读源比对，所有锁定该 skill 的项目立即全量漂移、apply 从缺失目录复制失败。「删暂存回滚」只删 staging，无法恢复已删旧 target——与「不留半删」承诺冲突，正是原 finding 5 描述的状态。

**已采纳**：force 改三段原子 `target→.old → staging→target → rm .old`，失败 `rename(.old→target)` 还原（见 spec §3.2/§3.7）。

#### F2 [medium] 锁 key 未指定且非协作写入方未纳入：registry lost-update 与池交错仍可能

§3.7 说「全流程持 ~/.skillkit/.lock」，但 FileLock（lock.rs）是 per-key，现有 key 只有 registry/config/sources/profile-*/project-*，无池级 key。(a) 若 install_local 用新 key，并发 Registry::save 只锁 "registry"，可在 install_local 的 reg.load 与 reg.save 之间写入 registry.json 被陈旧快照覆盖——lost-update；(b) uninstall（删目录在 save 前）、rescope（rename 目录进池）、install add（npx 完全无锁）都是池写入方，不与 install_local 串行。spec 只承认 npx 未锁，漏了 uninstall/rescope。

**已采纳**：install_local 全程持 "registry" key（闭 lost-update），并诚实声明池物理变更方不持锁是既有债、全闭需池级共享锁、超出本功能范围；实现注意 Registry::save 同进程自死锁（用不重复加锁变体）。spec §3.7 + §9 已落（核实 grep：install/uninstall/rescope 确不在 FileLock::acquire 列表，既有债属实）。

#### F3 [medium] force 分支缺「无其他 id 引用同一 target」校验，跨 source 误删在陈旧状态仍可复现

§3.2 只有「id 存在→force 删 / id 不存在但目录占→拒」两分支；§4 称「删前校验归属同一 id」但 §3.2 无可操作定义。唯一可靠实现=「扫描 registry，无其他 id 的 canonical_path==target」。containment 只防越界不防归属。可复现链：旧版本 force 已留 registry 双 id 同指 skills/foo 的悬空态 → 用户按引导操作后重装 → `install local foo --force`：id local/foo 存在 → 删 skills/foo → 再次删掉 skills.sh/foo 的 canonical。

**已采纳**：force 删/move-aside 前补 registry 反查（local/<name> 之外任何 id 的 canonical_path==target 则拒）。spec §3.2/§8 已落。

#### F4 [medium] rename 成功后 hash/registry 失败的回滚条款失效：孤儿 canonical + 引导死锁

流程 复制→rename→hash→registry upsert+save。「任一步失败→删暂存」只覆盖 rename 前失败；rename 后 staging 已不存在，hash 或 reg.save 失败 → 回滚无从执行 → 池里留无 registry 的孤儿 skills/<name>。下次装同名：id 不存在但 target 被占 → SkillPoolOccupied 引导 uninstall，但 registry 无 id 指向（孤儿），用户被卡死。

**已采纳**：hash 移到 rename **之前**（对 staging 算），把就位后失败面缩到只剩 reg.save；两段回滚（就位前删 staging / 就位后非 force 删 target、force 还原 .old）；SkillPoolOccupied 的 owner_id 为 Option，None（孤儿）时给手动清理指引。spec §3.2/§3.7/§6/§8 已落。

### Next Steps（第二轮）

1. force 改三段原子替换先删后 rename（F1）——✅ 已落 spec。
2. 指定锁 key="registry" 并说明 registry lost-update 防护 + 同进程自死锁注意 + 既有池竞争限制（F2）——✅ 已落 spec。
3. force 补 registry 反查归属再删（F3）——✅ 已落 spec。
4. 两段回滚覆盖 rename 后失败 + 孤儿占位处理（F4）——✅ 已落 spec。

> 两轮共 9 个发现均已落实进 spec。第二轮 4 条集中于原子/归属/回滚，是第一轮 finding 5 的深化闭合。
