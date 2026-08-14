# Spec Review 第 3 轮（收敛确认）— import 存量 skill 迁入 canonical 池设计（2026-08-14）

> 审查对象：`docs/superpowers/specs/2026-08-14-import-relocate-design.md`（r3 修订版，已按 r2 review 改 P2×3 / P3×5）
> 审查基准：对 r2 提出的 3 个 P2 + 5 个 P3 逐条核对落实，重点验 P2-1（补桥接自指 symlink）逻辑闭环；重审有无新引入 P1/P2。代码基准同 r2（`crates/core/src/{import,scope,symlink,install,error}.rs` 等）。
> 日期：2026-08-14
> 结论：**收敛通过。** r2 的 3 个 P2 + 5 个 P3 全部正确落实、逻辑自洽，无新增 P0/P1/P2。仅剩 1 个纯措辞 P3（§9「无条件幂等补桥接」与 §3.3 新增的条件判定字面冲突，改一词即可）。可进入实现。详见 §2/§3。

## 1. 总体结论

- **P2-1（补桥接自指 symlink）逻辑闭环已确认**：§3.3 补桥接改为 `canonical_path.starts_with(skillkit_skills_dir())` 前置条件后，dangling/symlink 归槽跳过的 canonical 因 canonical_path 仍指原位（非池路径）→ starts_with 为 false → 补桥接也跳过，彻底消除「target==link 自指环」。四种 canonical 情形（dangling / symlink / 真实目录归槽 / 本就在池）逐一遍历，全部收敛到正确行为（见 §3-A）。这是 r2 唯一有实际有害后果的项，现已闭环。
- **P2-2（Io 非 Tool）三处一致无遗漏**：grep 确认全文零 `Tool`（FS 错误语境），§3.5:116 / §6:145 / §7:160 均为 `Io`，且 §7 标注「对齐 scope.rs:64/69 范式源」（代码确认 scope.rs:64 `remove_dir_all(src)?`、:69 `rename(src, &target)?` 均裸 `?` → Io）。
- **P2-3（跨目录同名）§6 + §8 准确**：§6:147 跨目录同名行补全 codex/cursor 优雅 orphan vs agents+claude 触发 CanonicalCreate 中断的区分；§8:180 补对应测试，两子 case 均覆盖。
- **P3×5 全部落实**：§3.2 步骤 0 补 continue + 结构重构说明（:76）、§3.3 补 save 时机（:99）、§3.4 gemini 改泛指（:109）、§6 末尾补破坏性 rename 风险（:151）、§7 补计数维度（:159）。
- **无新增 P1/P2**。仅 1 个纯措辞 P3：§9:197「relink 无条件幂等补桥接」的「无条件」字面与 §3.3 新增的「仅当 canonical 已在池」条件冲突（逻辑一致，仅词陈旧）。

## 2. 问题清单

> 严重度分级：🔴 P0 必须修（计划级 bug）｜🟠 P1 需决策（与代码语义矛盾）｜🟡 P2 建议修｜⚪ P3 不阻塞。

### ⚪ P3 — §9「无条件幂等补桥接」措辞与 §3.3 条件判定字面冲突

- 现象：§9:197 决策理由仍写「relink **无条件**幂等补桥接（不只处理 canonical 不在池）」。r2 P2-1 修复后，§3.3 补桥接已加条件「仅当 canonical 已在池（starts_with）才调 ensure_global_claude，**不**对 dangling/symlink 补桥接」。两处字面冲突——§9 说「无条件」，§3.3 说「仅当 canonical 已在池」。
- 证据：§9:197「无条件幂等补桥接」；§3.3:95「仅当 canonical 已在池（canonical_path.starts_with(...)）时调 ensure_global_claude … **不**对归槽跳过的 dangling/symlink canonical 补桥接」。
- 评估：逻辑一致——§9 的「无条件」本意是「补桥接不只针对 canonical-不在池（刚归槽）的 skill，也覆盖 canonical-本就在池的中间态」，括注「不只处理 canonical 不在池」已点明。但它与 §3.3 新增的 dangling/symlink 排除条件字面打架，后续维护者读 §9 可能误以为补桥接对 dangling 也跑（即 r2 已修掉的 bug）。
- 建议：§9:197 把「无条件幂等补桥接」改为「对池内 canonical 幂等补桥接（含刚归槽与本就在池；dangling/symlink 跳过，见 §3.3）」。一词之改，消除歧义。不阻塞实现。
- 测试盲区：无（逻辑已由 §3.3 + §8 dangling 测试覆盖）。

### ⚪ P3 — §8 relink symlink 边界可补「无新建 symlink」断言（与 dangling 子 case 对齐）

- 现象：§8:177 relink 边界中，dangling 子 case 明确「断言 ~/.agents/skills/<x>、~/.claude/skills/<x> 均无新建 symlink，验无自指/悬空环」；但 symlink 子 case 只写「（跳过）」，未显式要求同款断言。
- 评估：symlink canonical 与 dangling 同属「canonical_path 不在池 → starts_with false → 补桥接跳过」，逻辑等价，dangling 的断言同样适用。不补也能过（实现者会自然类比），但显式写出更稳。
- 建议：§8 symlink 子 case 补「（跳过，且不补建桥接——同 dangling 断言无新建 symlink）」。不阻塞。

## 3. r2 修复逐条核对（重点 P2-1 逻辑闭环验证）

### A. P2-1 补桥接 starts_with 前置 — 四种 canonical 情形遍历（逻辑闭环确认）

relink 对每个 unmanaged+global skill，先归槽（`!starts_with(pool)` 入）、再补桥接（`starts_with(pool)` 入）。四种情形：

| canonical 情形 | 归槽（!starts_with 入） | canonical_path 变化 | 补桥接（starts_with 判定） | 结果 |
|---|---|---|---|---|
| dangling（不在池、不存在） | warn 跳过 | 不变（仍指原位非池路径） | false → **跳过** | 无桥接、无自指环 ✓ |
| symlink（不在池、是 symlink） | 跳过（只迁真实目录） | 不变（仍指原位非池路径） | false → **跳过** | 无桥接、无自指环 ✓ |
| 真实目录（不在池） | adopt + 更新 canonical_path 指池 | → 池路径 | true → ensure_global_claude | 池内 canonical 建有效桥接 ✓ |
| 本就在池（starts_with true） | 不入（!starts_with false） | 不变（池路径） | true → ensure_global_claude（幂等） | 补建/确认有效桥接 ✓ |

关键验证：dangling/symlink 归槽跳过后 canonical_path 不变（仍指 agents/claude 等非池原位），补桥接的 starts_with 判定为 false → 跳过。r2 担心的自指环（canonical_path 恰在 `~/.agents/skills/<name>`、target==link）因此不可能发生——agents 路径不在池（`~/.skillkit/.agents/skills/`）下，starts_with 必为 false。**逻辑闭环确认。**

补充确认：池内 canonical 但池目录被用户手删（canonical_path=starts_with true 但目录不存在）的边界——归槽不入（starts_with true）、补桥接会建指向池内空路径的悬空桥接（非自指环）。这与 managed global skill 池目录被删后的桥接行为一致（既有行为，非本 spec 引入），不构成新问题。

### B. P2-2 Io 三处一致性

grep 全文零 `Tool`（FS 错误语境）。三处确认：

| 位置 | 原文（r2） | 现文（r3） | 一致性 |
|---|---|---|---|
| §3.5:116 | 报 Tool 错 | 报 `Io` 错（`#[from]` 映射） | ✓ |
| §6:145 | SkillkitError::Tool | `SkillkitError::Io`（error.rs:25-26 `#[from]`，保留原始 io 信息） | ✓ |
| §7:160 | 复用 CanonicalCreate / Tool | 复用 `CanonicalCreate`；FS 错误走既有 `Io`（对齐 scope.rs:64/69） | ✓ |

代码印证：error.rs:25-26 `Io(#[from] std::io::Error)`；scope.rs:64 `remove_dir_all(src)?`、:69 `rename(src, &target)?` 均裸 `?` → Io。§3.1 adopt 代码同样裸 `?`，映射一致。无遗漏。

### C. P2-3 跨目录同名 — §6 + §8

§6:147 跨目录同名行：codex/cursor 副本走优雅 orphan（桥接不碰这两目录）；agents+claude 同名真实目录副本 → 首个 adopt 入池后建 claude 桥接撞占位 → CanonicalCreate 中断（归入桥接占位行），重跑持续撞同一占位直到清理。与 r2 分析的执行路径（adopt agents/foo→pool → ensure_global_claude agents 桥接 OK → claude 桥接撞 claude/foo 真实目录 → CanonicalCreate）一致。

§8:180 测试：预置 agents/foo + claude/foo（均真实目录）→ import 报 CanonicalCreate；codex/cursor 同名副本走优雅 orphan。两子 case 覆盖。✓

### D. P3×5 落实核对

| r2 P3 | r3 落实位置 | 核对 |
|---|---|---|
| 步骤 0 continue + dry_run 分叉前 + 结构重构 | §3.2:76「push skipped 后 continue（不落 imported，防双计）… 须在 dry_run 分叉前生效 … 需重构 import.rs:71/89/104」 | ✓ 行号 71/89/104 准确（managed / unmanaged-非dry / unmanaged-dry 三分支） |
| relink save 时机 | §3.3:99「每 skill 归槽成功后立即 save registry（对齐 §3.2 顺序）… 中途 EXDEV 中断时已迁 canonical_path 已落盘」 | ✓ 失败面可推导 |
| gemini 泛指 | §3.4:109「config.toml 声明 reads_agents_dir=true 的 agent 直读，默认 cursor/codex」 | ✓ config.rs:35/40 cursor/codex true，无 gemini |
| 破坏性 rename 风险 | §6:151「relink 对存在真实目录是破坏性 rename（不可逆）… canonical_path 由 skillkit 自管 … 风险低不额外防护（YAGNI）仅声明」 | ✓ |
| 计数维度 | §7:159「unmanaged/reinstalled 按 source 类型，relocated/relinked 按动作；一个 skill 可同时进两维度；imported 含全部登记成功的」 | ✓ 消除歧义 |

## 4. 结论

**收敛通过，可进入实现。** r2 的 3 个 P2（补桥接自指 symlink / Io 变体 / 跨目录同名中断）全部正确落实且逻辑自洽，P2-1 的 starts_with 前置经验四种 canonical 情形遍历确认闭环、自指环不可能发生；P2-2 三处 Io 一致无遗漏；P2-3 §6/§8 双覆盖。P3×5 全部落实。无新增 P1/P2。

仅剩 2 个纯措辞 P3（§9「无条件」改一词、§8 symlink 子 case 补断言），不阻塞——可在实现时顺手改 spec，或留作实现后文档同步。建议直接进入 writing-plans / 实现，按 §3 TDD 展开（现有 4 个 import.rs 测试更新 + §8 新增 7 条，含 dangling 不建桥接验 P2-1、跨目录同名中断验 P2-3）。
