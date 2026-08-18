# skillkit 全项目待办跟踪

> 用途：全项目唯一的待办清单。散落在各 spec「已知限制 / 后续提醒 / YAGNI 边界」和 sessions 交接里的遗留事项统一收口到这里，新会话或新一轮开发先读本文。
>
> 维护规则：
> - 完成一项勾选 checkbox，并在条目末尾追加 commit hash（或 PR 链接）。
> - 新发现的遗留事项（spec review 残留、交接时的已知限制、走查发现的坑）入列，注明来源。
> - 第四类是 spec 声明过的取舍，动它们等于改设计决策——开工前先改对应 spec 再改代码。
> - 快照日期：2026-08-17（基于 import 迁池完成后的 main）。

## 一、基建与发布

- [ ] 补齐三个 crate 的 Cargo.toml 元数据（`description` / `license` / `repository`）。README 已声明 MIT，元数据未跟上。完成标准：`crates/{core,cli,server}/Cargo.toml` 三处补齐且 `cargo package --list`（或 lint）无警告。
- [ ] 建 GitHub Actions CI：`.github/workflows/` 目前不存在。Makefile 注释已写明「CI 与本地走同一套规则」，只需一个 workflow 跑 `make check`（fmt + clippy -D warnings + 全测试）。可选加 `make e2e-cli`（联网，见第三类第 6 条）作为手动触发 job。
- [ ] 发布分发未做：crates.io / brew tap 均未发布（M3 spec §11 YAGNI 边界）；主 spec §15 M3 的「打包进 mac-config Brewfile」待确认是否已做。

## 二、并发锁既有债（两处同源，全闭需池级共享锁）

- [x] 池物理变更方（uninstall 删目录 / rescope rename / install add 的 npx 写）不持锁，不与 install_local 的 `"registry"` 锁串行。来源：install-local spec §3.7 已知限制。已修 `9ffd194`：registry 写回段全部持锁（锁内重读 + save_raw），覆盖回滚根治；npx 物理写段留锁外（防 5s 锁超时），物理段并发仍是非目标取舍。
- [x] `import_existing` 全程不持 `FileLock`。来源：import-relocate spec §3.5（声明低频人工操作可接受）。已修 `9ffd194`：adopt/relink 每对 load→save 窗口级持锁（非全程，窗口间释放不阻塞并发写方），registry 写入互不覆盖。

## 三、测试覆盖

- [ ] 联网测试 14 个 `#[ignore]`（cli e2e 9 + core m0 3 + m3 2），需真跑 `npx skills`，不进 `make check`，只能 `make e2e-cli` 手动跑。可评估挑稳定的进 CI 手动触发 job。
- [ ] GUI 浏览器 e2e（`make e2e`，playwright + chromium）不进 check。若进 CI 需装浏览器依赖，成本先评估。
- [ ] import-relocate r3 review 残留 P3：relink 的 symlink 子 case 补「无新建 symlink」断言，与 dangling 子 case 对齐（现只断言 skipped）。来源：`docs/review/2026-08-14-import-relocate-design-spec-review-r3.md` §2。

## 四、已知行为限制（spec 声明的取舍，做不做需决策）

- [ ] uninstall 对 unmanaged 只摘登记不删 canonical；迁池后留孤儿目录 + dangling 桥接，需手动 `rm` 或 GUI remove。来源：import-relocate spec §6 连带影响。
- [ ] GUI 绑 profile 是全量替换 `installed_skills`，CLI `project apply` 仍是追加，两者语义未统一。来源：projects-ui-redesign spec §7。
- [ ] legacy `installed_skills` 里的 global 在 apply 时幂等忽略、不落地、不报错。来源：scope-profile spec §8 已知限制。
- [ ] browse 路径含空格/中文/`&` 时 query 不 percent-encode。来源：projects-ui-redesign spec §7。
- [ ] GUI find 走 npx 偶发超 10s 时的超时方案未评估（现只有 loading 提示 + 可读错误）。来源：gui-parity spec §8 风险。
- [ ] local skill 的 `version` 恒为 `None`（`--version` 参数 / 读 SKILL.md frontmatter 均未做）。来源：install-local spec §5。

## 五、UI 打磨（可选）

- [ ] 其余页面对齐 demo 原型：Projects 卡片网格、Sources 源注册表、Profiles 粗分类（目前仅 Skills 视图对齐）。参照 `demo/index.html`。
- [ ] 大目录上传（install-local modal）无进度条，千文件级目录有秒级延迟。来源：install-local-ui spec §9 声明 YAGNI，实测痛再做。

## 未来备忘（预留升级路径，非待办，勿排期）

主 spec §16：同一 skill 多物理版本并存（canonical 按版本分目录）、profile 继承、更多 agent（改 config.toml 即可）。这些是有需求再启动的方向，不是欠账。
