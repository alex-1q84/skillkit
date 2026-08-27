# skillkit 交接（2026-08-26，全局死代码/重复代码清理重构）

> 用途：新会话读 §1（结果概要）+ §3（行为变更注意）+ §5（未做项）三段够用。本次是纯重构 + 一个遗留 fix 的补提交，无功能新增。
>
> **分支**：`refactor/code-cleanup`（自 main 32ce498 切出），11 个 commit（核心 8 个 + 评审轮修补 3 个，见 §7），未合并、未 push。合并决策留给主人。

## 1. 结果概要

任务：拉最新代码 → 建重构分支 → 全局分析死代码/重复代码/过度设计 → TDD 优化 → 按关注点提交。全部完成，`make check` 双绿（234 tests，clippy -D warnings 零告警），工作区干净。

commit 列表（按序，每个独立可 review）：

| commit | 关注点 | 性质 |
|---|---|---|
| 208fd58 | locked_shas 孤儿锁 + uninstall 桥接残留（上次会话已完成未提交的修复，本次补提交） | fix，4 回归测试 |
| 3f7cec3 | 删死代码（is_global、skill_profiles）+ 收窄 7 个内部函数可见性 | refactor，净 -57 行 |
| 56e48cd | 统一 adopt_into_pool 入池迁移双份（import ↔ scope） | refactor，表征测试已有 |
| dcaa684 | 提取 scan_extras 统一 apply/status 的 extra 判定 | refactor，补 status 侧断言 |
| 8385638 | 提取 with_registry 收敛 5 处持锁写事务 | refactor，TDD 单测先行 |
| 07308a2 | SourcesStore::register 收敛两壳 source add | refactor，TDD 红绿（4 新测试） |
| c7acdc5 | project::load_all + apply::compute_status 收敛加载/组装管线 | refactor，2 新测试 |

总量（至 78d4bb3）：18 文件，+511/-300。测试函数新增 12、删 1（净增 11，`#[test]` 172→183；208fd58 的 4 个 + register 4 + with_registry 2 + load_all/compute_status 2）。`make check` 当前全绿（235 tests，含评审轮新增文案测试）。

> 更正（08-26 评审轮）：原文误写「测试从 228 增至 234（净增 6）」——228 是分支中途的 cargo 计数误当 main 基线，实测以测试函数计为净增 11。

## 2. 分析结论（全局盘点）

### 2.1 死代码

- `SkillMeta::is_global`：零调用（模板只用 `is_local`，scope 判断各处内联比较）。已删。
- `skill_profiles`（单 skill 反查）：只被自己的测试调用，生产路径零使用（两壳用的都是批量版 `skills_profiles_map`）。已删（含测试）。
- 依赖无多余（uuid/zip/chrono/tempfile 等逐一核实有真实使用），无 cargo-udeps 需求。
- API 面过宽（非死代码但同类问题）：`landing_agents`/`write_exclude`/`new_id`/`acquire_with_timeout`/`remove_global_claude` 降 pub(crate) 并撤出 lib.rs re-export。`ensure_global_claude` 原计划一并收窄，发现 m0 集成测试在用，保持 pub——教训：死代码判定必须包含 `crates/*/tests/`，不只 src 和两壳。

### 2.2 重复代码（已收敛的）

1. **adopt_into_pool 双份**：import.rs 存量归槽与 scope.rs rescope 降级各自维护「池子已有删源 / 池子空 rename 迁入」（含数据删除决策），靠注释互相对齐。现 scope.rs 复用 import.rs 的 pub(crate) 函数，规则单点。
2. **apply/status extra 判定双份**：run_apply 清理循环与 build_status 计算循环重复同一套 expected key 构建 + canonicalize alias 豁免判定。提取私有 `scan_extras` 共用——「感知-执行」闭环两端不再可能漂移。
3. **持锁写事务 7 处**：「acquire → load → mutate → save_raw」在 install/uninstall/upgrade/import×2 手工复制，每处依赖开发者记住 save vs save_raw 的死锁约束。提取 `registry::with_registry` 收敛 5 处。**两处有意不收敛**：install_local（锁窗口覆盖解压/复制/原子就位全流程，防装竞态的宽窗口设计）；set_scope（same-scope 早退 + 锁内 match 分支，收敛反而复杂化，且有 `rescope_survives_concurrent_registry_writer` 并发回归测试守护）。
4. **两壳 source add 业务副本**：CLI 与 server 各一份「显式名 trim 回退推导 → 推导失败报错 → 撞名检查 → add+save」，违反「业务只在 core」约束。收敛为 `SourcesStore::register`（新增错误变体 SourceNameUnderived / SourceNameTaken，server 按变体映射 400/500）。
5. **项目加载循环 6 处 + status 组装 3 处**：`project::load_all`（跳过坏文件语义单点）+ `apply::compute_status`（组装单点，容错降级策略留壳层）。

### 2.3 过度设计

结论：无实质问题。无单实现 trait、无过早抽象、lock.rs（轮询 try_lock + RAII guard）与 detect.rs 设计克制。API 面过宽是唯一接近项，已在 3f7cec3 处理。CLAUDE.md 的 YAGNI 原则（单版本模型不分目录等）在代码中被遵守。

## 3. 行为变更注意（review 重点）

> 更正（08-26 评审轮）：原文称「两处行为变化均在 c7acdc5」，归因有误——server source add 的变化在 07308a2（c7acdc5 未触碰 sources.rs）。以下为修正后的完整清单。

非纯重构的行为变化（208fd58 是 fix，行为变更即其目的，不列）：

1. **CLI `project list` 容错语义**（c7acdc5）：原单个坏 toml 使整个命令报错退出；现统一为跳过该文件 + tracing warn（与 server 行为一致）。理由：列表视图不应因一个坏文件全挂。若要 CLI 保持严格报错，把 `load_all_projects` 换回手动循环即可。
2. **server source add 错误路径**（07308a2）：撞名/推导失败仍 400，load 失败从「500 加载 sources 失败」变为走 register 内部报错路径（仍 500）。行为等价，实现路径变化。
3. **CLI source add 文案**（07308a2，原文未披露）：成功输出「✓ 已添加源」→「✓ 已添加源 {name}」；撞名/推导失败文案改走 SkillkitError Display（非 `--json` 契约，GUI 400 文案不变；Display 文案后在评审轮 a839852 补了参数指引）。

其余 4 个 refactor commit（3f7cec3/56e48cd/dcaa684/8385638）行为不变（表征测试 + 全量回归守护）。

## 4. TDD 执行情况（如实记录）

- **严格红绿**：SourcesStore::register（4 测试先行见编译红 → 实现绿）；with_registry（2 个测试函数先行，覆盖三要素——落盘可见+锁释放可再事务合一测、闭包报错不落盘一测。原文误写「3 测试先行」，08-26 评审轮更正）。
- **表征测试先行（重构的正确形态）**：scan_extras（先在既有 alias 回归测试补 `status.extra` 不误报断言再提取）；adopt_into_pool（复用 scope.rs 两个既有分支测试）。
- **红绿倒置（实施顺序偏差，如实记录）**：load_all 与 compute_status 实现先于测试编写，测试为事后行为锁定（坏文件跳过用例、与手工组装等价用例）。
- **删除型改动**：死代码/可见性收窄无新行为可测，验证手段为 clippy -D warnings（dead_code lint）+ 全量测试 + 模板 grep（Askama .html 不进 rustc 视野，需单独确认 `is_global` 无模板引用）。

## 5. 未做项（分析发现、本次不修，按建议优先级）

分析共识别 4 个 Tier，本次只做 Tier 1 + 死代码。剩余项：

1. **报告 summary 文案三对重复**（中优先）：import/upgrade-all/profile-delete 的「报告→人话」格式串在 CLI 与 server 成对重复（如 `cli/commands/import.rs:21-31` vs `server/routes/skills.rs:355-369`）。建议在 core 报告结构体上加 `summary()` 方法。
2. **测试 seed helper ≥8 份**（中优先，改动量大但零风险）：`install_local_bare`/`seed_skill`/`reg_with` 等同族 helper 在 core src 测试、core/tests、server/tests 复制。建议建 `skillkit_core::testutil`（`#[doc(hidden)] pub mod`），SkillMeta 字段演进时 8 处不同时改。
3. **server skills.rs 批量表单解析 3 处**（中低）：`parse_bulk_body(body) -> Option<(String, Vec<String>)>` 可收敛 unassign/assign/assign_new。
4. **Scope 字符串解析 7 处分散**（低）：合法 scope 字符串集散在 4 个 server handler + 2 个 CLI 命令。建议 core 实现 `Scope: FromStr`。
5. **CLI 交互 confirm 模式 6 处**（低）：`print → read_line → trim != "y"` 可提取 `fn confirm(prompt) -> bool`。
6. **server render_str 4 份**（低）：仅 log 文案不同，可上提 routes/mod.rs。
7. **假 npx + PATH guard 两份**（低，测试设施）：upgrade.rs 测试与 server/tests/common 各一份 RAII guard。

## 6. 验证清单

```bash
cd /Users/mywo/lab/skillkit && git checkout refactor/code-cleanup
make check          # 已验证：format + clippy -D warnings + 235 tests 全绿
# 如需手动走查 GUI：
make run ARGS="serve --port 7317"   # 重点走查：sources 添加（撞名/推导失败 400）、projects 列表、workspace status
```

合并建议：核心 7 个 commit（至 78d4bb3）各自独立可 cherry-pick，均基于 main 线性叠加，任意前缀可停。208fd58 是 bug 修复（TODO.md 四、新发现项有记录），优先级最高。评审轮 3 个修补 commit 建议随核心一同合入。

## 7. 评审轮（08-26 二轮，commit a839852 / 73129b6 / 6ecca5f）

本分支经两轴 code review（Standards 轴对照 CLAUDE.md + Fowler smell 基线；Spec 轴对照任务指令 + TODO 登记）。结论：0 硬违规，4 判断类发现 + 4 项文档失实，全部处理：

- a839852：`SourceNameTaken` 文案补「--name / name 字段指定别名」参数指引（红绿：先加文案断言测试）。附带修复 Spec 轴发现的 CLI 文案退化。
- 73129b6：`load_all` 的 `list_ids` 失败从静默吞空改 warn（「不静默跳过」原则）。
- 6ecca5f：`StatusView` derive Default，server 两处 5 行降级样板改 `unwrap_or_default()`（收敛后残留的重复，比提 helper 更简）。
- 本 commit：文档三处失实修正（测试计数、with_registry 测试数、§3 行为变更归因）+ 补记本节。

评审遗留未处理（判断类，低价值，留主人裁量）：`compute_status_pipeline_matches_manual_assembly` 测试前半的「管线==手工组装」等价断言偏实现细节（后半业务断言有价值），可择机精简。
