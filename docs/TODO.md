# skillkit 全项目待办跟踪

> 用途：全项目唯一的待办清单。散落在各 spec「已知限制 / 后续提醒 / YAGNI 边界」和 sessions 交接里的遗留事项统一收口到这里，新会话或新一轮开发先读本文。
>
> 维护规则：
> - 完成一项勾选 checkbox，并在条目末尾追加 commit hash（或 PR 链接）。
> - 新发现的遗留事项（spec review 残留、交接时的已知限制、走查发现的坑）入列，注明来源。
> - 第四类是 spec 声明过的取舍，动它们等于改设计决策——开工前先改对应 spec 再改代码。
> - 来源栏指向的 spec/review/sessions 过程文档已于 2026-10-31 全量清理（判定经四组判官+独立复核），路径仅供 git 历史溯源。
> - 快照日期：2026-10-30（基于 main `8fda816`，v0.1.8；本次快照核对由 2026-10-31 会话完成）。

## 一、基建与发布

- [x] 补齐三个 crate 的 Cargo.toml 元数据（`description` / `license` / `repository`）。README 已声明 MIT，元数据未跟上。完成标准：`crates/{core,cli,server}/Cargo.toml` 三处补齐且 `cargo package --list`（或 lint）无警告。已修 `bbbe479`：workspace.package 共享 license/repository + 各 crate description，`cargo package --list` 无警告，另补缺失的 MIT LICENSE 文件。
- [x] 建 GitHub Actions CI：`.github/workflows/` 目前不存在。Makefile 注释已写明「CI 与本地走同一套规则」，只需一个 workflow 跑 `make check`（fmt + clippy -D warnings + 全测试）。可选加 `make e2e-cli`（联网，见第三类第 6 条）作为手动触发 job。已修 `bbbe479`：macos runner 跑 `make check` + `v*` tag 自动出 Release（附 tarball 与 sha256.txt）。首跑因 runner 最新 stable 的新 clippy 挂过一次，`5b731a2` 适配后绿；e2e-cli 手动 job 未加（可选项，有需要再补）。
- [x] 发布分发未做：crates.io / brew tap 均未发布（M3 spec §11 YAGNI 边界）；主 spec §15 M3 的「打包进 mac-config Brewfile」待确认是否已做。已做（私有渠道，`bbbe479`）：GitHub Release v0.1.0 二进制（aarch64）+ mac-config 内嵌 `Formula/skillkit.rb`，经本地 tap（Homebrew 4.x 拒绝路径 formula）+ `just install_skillkit` 实测 `brew install skillkit` 成功（mac-config `23bc746dd`、`ada3d65db`）。crates.io 缓行，解冻条件：有外部受众或决定开源推广。
- [ ] `make e2e-all` 聚合 target：统一入口串起 check 之外的各层 e2e——`make e2e`（playwright 浏览器）、`make e2e-cli`（联网 CLI），以及 core 侧 m0/m3 `--ignored`（现无 Makefile target，需手动 `cargo test -- --ignored`）。多轮会话列为 Minor 未做。来源：sessions/2026-08-10-skillkit.md §5-6 等；2026-10-31 核实 Makefile `.PHONY` 仅 e2e/e2e-cli 两 target，无 e2e-all。

## 二、并发锁既有债（两处同源，全闭需池级共享锁）

- [x] 池物理变更方（uninstall 删目录 / rescope rename / install add 的 npx 写）不持锁，不与 install_local 的 `"registry"` 锁串行。来源：install-local spec §3.7 已知限制。已修 `9ffd194`：registry 写回段全部持锁（锁内重读 + save_raw），覆盖回滚根治；npx 物理写段留锁外（防 5s 锁超时），物理段并发仍是非目标取舍。
- [x] `import_existing` 全程不持 `FileLock`。来源：import-relocate spec §3.5（声明低频人工操作可接受）。已修 `9ffd194`：adopt/relink 每对 load→save 窗口级持锁（非全程，窗口间释放不阻塞并发写方），registry 写入互不覆盖。
- [ ] 池级锁缺口（剩余纯物理段）：uninstall 的物理 trash/删目录与 install(add) 的 npx 下载写段不持池级锁，与持锁物理方（rescope rename / import adopt / install_local 迁址）互不串行，物理段仍可能互踩。registry 快照覆盖已由 `9ffd194`（锁内重读再写）解决，本条仅剩物理段并发。来源：install-local spec §3.7 + import-relocate spec §3.5（两处均记为后续工作）；2026-10-31 核实 install.rs:58-59 与 :161-162 注释仍写「npx 下载/物理删除在锁外（秒级）」。

## 三、测试覆盖

- [ ] 联网测试 11 个 `#[ignore]`（e2e_cli.rs 8 处 :201/:331/:400/:423/:447/:472/:492/:587 + m0_e2e.rs 2 处 :52/:102 + m3_e2e.rs 1 处 :53，另有 3 处 `//!` 文档注释不计），需真跑 `npx skills`，不进 `make check`，只能 `make e2e-cli` 手动跑。可评估挑稳定的进 CI 手动触发 job（2026-10-31 核实 `.github/workflows/` 仅 ci.yml、grep e2e 零命中，未做）。
- [ ] GUI 浏览器 e2e（`make e2e`，playwright + chromium）不进 check。若进 CI 需装浏览器依赖，成本先评估。
- [x] import-relocate r3 review 残留 P3：relink 的 symlink 子 case 补「无新建 symlink」断言，与 dangling 子 case 对齐（现只断言 skipped）。来源：`docs/review/2026-08-14-import-relocate-design-spec-review-r3.md` §2。已含 `b2953ad`：该 commit 引入 relink 测试时即带此断言（`relink_skips_symlink_canonical_without_bridge`，现 import.rs:662-675 断言「不建桥接」）。
- [ ] 测试残留注销：`skillkit project remove huasheng-data-server --yes`（只删 `~/.skillkit/projects/F0ACAB43.toml` 登记文件，不碰项目目录）。来源：sessions/2026-08-10-skillkit.md §5-4（同见 archive/2026-08-04-skillkit.md §5-2、archive/2026-08-07-skillkit.md §5-4）；2026-10-31 实测 F0ACAB43.toml 仍在，TODO.md 此前无收录。

## 四、已知行为限制（spec 声明的取舍，做不做需决策）

- [x] uninstall 对 unmanaged 只摘登记不删 canonical；迁池后留孤儿目录 + dangling 桥接，需手动 `rm` 或 GUI remove。来源：import-relocate spec §6 连带影响。已被 `cb458a3`（2026-09-11「卸载一律进系统回收站，unmanaged 不再只摘记录」）推翻：现行行为 = 撤桥接 + 池内 canonical 送系统回收站 + 摘记录，池外目录按防误删约定保留（install.rs:141-158）。残留瑕疵可顺带修：install.rs:85-87 `clear_occupied` 注释仍写「unmanaged 只摘记录」，未随 cb458a3 同步。
- [ ] GUI 绑 profile 是全量替换 `installed_skills`，CLI `project apply` 仍是追加，两者语义未统一，暂不统一。来源：projects-ui-redesign spec §7。2026-10-31 核实现状：`set_profiles`（project.rs:122-141，替换语义）与 `apply_profile`（project.rs:109-115，追加语义）并存，cli/commands/project.rs:91 仍调追加版。
- [ ] legacy `installed_skills` 里的 global 在 apply 时幂等忽略、不落地、不报错。来源：scope-profile spec §8 已知限制。
- [ ] browse 路径含空格/中文/`&` 时 query 不 percent-encode。来源：projects-ui-redesign spec §7。
- [ ] GUI find 走 npx 偶发超 10s（首次拉包/网络差）时的超时方案未评估（现只有 loading 提示 + 可读错误）。来源：gui-parity spec §4/§7/§8。2026-10-31 核实：routes/skills.rs grep timeout 零命中，skills_main.html:9 的 `delay:400ms` 是输入防抖非超时；候选方向 = 客户端 hx-trigger 超时或服务端超时，待评估。
- [ ] local skill 的 `version` 恒为 `None`（`--version` 参数 / 读 SKILL.md frontmatter 均未做）。来源：install-local spec §5。
- [ ] install-local 目录拖放依赖 `dataTransfer.files[k].webkitRelativePath` 填充相对路径：Chrome/Edge 填充、其他浏览器退化为仅文件名（「选择目录」按钮最稳），超大目录建议 CLI 兜底。此浏览器兼容坑未补记进 install-local spec §9 已知限制（八条中无此条）。来源：sessions/2026-08-10-skillkit.md §2.7；layout.html:319/347/381 三处依赖（2026-10-31 核实）。
- [x] project 的 `locked_shas` 残留（非 spec 取舍，新发现）：`~/.skillkit/projects/FF71C76A.toml` 的 `locked_shas` 仍含 `skills.sh/pdf`，但该项目 `installed_skills` 为空、项目 agent 目录无落地——skill 从项目移除后锁记录未随清理。需排查 remove-skill / apply-profile 路径是否漏清 `locked_shas`，并考虑 `project status` 把这类孤儿锁报告出来。同日另发现：`remove` global scope skill 后 `~/.agents/skills/` 的落地 symlink 残留成 dangling（canonical 已删、桥接未清）。来源：2026-08-21 skillkit skill 实测会话发现。已修（commit 待打）：三处清锁——`Project::remove_skill` 同步删条目、`set_profiles` 替换后 retain、`run_apply` 结尾重建 locked_shas 为 expected 快照（存量孤儿 apply 一次自愈，故 status 报告孤儿锁不再需要）；uninstall managed 分支补调 `remove_global_claude` 撤两层桥接（与 install 的 `ensure_global_claude`、rescope 降级路径对称），unmanaged 不受影响（本就无桥接语义）。四个回归测试覆盖。存量 `FF71C76A.toml` 需跑一次 `project apply` 自愈。

## 五、UI 打磨（可选）

- [ ] 其余页面对齐 demo 原型的参照前提已变：Skills 页经 `ec17791`（向导一键安装）/ `4e2efb6`（占用高亮 + 覆盖安装）/ `52bf73e`（SKILL.md 浮层）+ 同名副本面板（skills.rs:943-979）已远超 `demo/index.html` 原型（demo 最后提交 `5885ee9`，2026-08-01 后未再动），「对齐 demo」对 Skills 已无意义，原括注「目前仅 Skills 视图对齐」亦过时。Projects 卡片网格、Sources 源注册表、Profiles 粗分类是否仍以该 demo 为准，需定夺后再排。来源：projects-ui-redesign spec；2026-10-31 盘点核实。
- [ ] 大目录上传（install-local modal）无进度条，千文件级目录有秒级延迟。来源：install-local-ui spec §9 声明 YAGNI，实测痛再做。

## 六、代码收敛与文档同步（sessions/盘点遗留）

- [ ] 七项代码收敛：① summary 文案 CLI/server 成对重复三对（import/upgrade/profile-delete；如 cli/commands/import.rs:22 与 routes/skills.rs:454-455 同一格式串）② 测试 seed helper 分散、无 testutil 收口（原 ≥8 份，现余 4 处：apply.rs:645 / project.rs:363 / profile.rs:187 / scope.rs:121）③ skills.rs 三处 form_urlencoded 手工批量解析（:552/:591/:631）④ Scope 无 FromStr、字符串解析分散 ⑤ CLI confirm 模式 6 处 read_line 重复（rescope.rs:56 / upgrade.rs:66 / profile.rs:90 / skill.rs:147 / project.rs:166 / install.rs:67）⑥ render_str 4 份（profiles.rs:224 / skills.rs:933 / projects.rs:623 / sources.rs:150）⑦ 假 npx PATH guard 两份未收敛（core upgrade.rs:254 与 server/tests/common/mod.rs:27-44；竞态已由 `8973d2d` 加进程级互斥堵住，实现未收敛）。来源：sessions/2026-08-26-code-cleanup.md §5；2026-10-31 grep 七项全命中。
- [ ] `compute_status_pipeline_matches_manual_assembly`（apply.rs:666）测试前半「管线==手工组装」等价断言偏实现细节，择机精简（保留后半业务断言）。来源：sessions/2026-08-26-code-cleanup.md §7；`c7acdc5` 引入，现存。
- [ ] uninstall→remove 命名清理：CLI 命令面已是 `remove`（决策 29），内部仍叫 uninstall——core `crates/core/src/install.rs:113`（`pub fn uninstall`）、server `routes/mod.rs:130` 路由注册与 `routes/skills.rs:376` handler。来源：sessions/archive/2026-07-29-skillkit-design.md §5-3 显式遗留；2026-10-31 复核核实三处仍在。
- [x] specs/2026-08-04-skill-scope-profile-design.md §5.3 过滤机制描述更新至 pushState + HX-Current-URL 现状。已关闭（2026-10-31）：该 spec 已随过程文档全量清理删除（原文见 git 历史），pushState 现状唯一权威在 `docs/frontend-rules.md` §3/§4，无更新对象。来源：五组盘点 todos。

## 未来备忘（预留升级路径，非待办，勿排期）

主 spec §16：同一 skill 多物理版本并存（canonical 按版本分目录）、profile 继承、更多 agent（改 config.toml 即可）。这些是有需求再启动的方向，不是欠账。
