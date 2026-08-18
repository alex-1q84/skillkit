# skillkit 发版流程

> 私有渠道：GitHub Release 预编译二进制 + mac-config 本地 tap formula（不进 crates.io，解冻条件见文末）。
> 只发 aarch64（Apple Silicon）。tarball 由 CI 打包：`skillkit-<ver>-aarch64-apple-darwin.tar.gz` + `sha256.txt`。

## 步骤

1. 本地 `make check` 全绿；工具链与 CI 同为最新 stable（`rustup update stable`），旧工具链会漏新 clippy lint。
2. commit 并 push main。
3. 打 tag 并推送：`git tag vX.Y.Z && git push origin vX.Y.Z`。CI 自动构建、创建 Release 并附 tarball 与 sha256.txt（约 2 分钟，进度看 Actions）。
4. 回填 formula：从 Release 附件 `sha256.txt` 取值，更新 mac-config 仓库 `Formula/skillkit.rb` 的 `url` 与 `sha256`，commit（本地 tap 是 git clone 语义，不 commit 则 brew 看不到新版）。
5. 升级安装：`cd ~/lab/mac-config && just install_skillkit`（内部完成 tap 刷新 → trust → `brew upgrade skillkit`）。
6. 验证：`skillkit --version` 输出新版本号。

## 版本号约定

pre-1.0 阶段（0.x.y）：`--json` 输出结构或 CLI 参数语义变更 bump x，行为修复与小功能 bump y。

## 坑位备忘

- CI runner 每次全新拉最新 stable 工具链，pedantic lint 随版本增长，本地长期不 `rustup update` 会在 CI 上爆出新 error（2026-08 v0.1.0 首发即因此挂过，11 处 map_unwrap_or 等）。
- Homebrew 4.x 拒绝路径 formula（必须进 tap），第三方 tap 需 `brew trust`——两者都已在 just recipe 内处理，勿手装。
- formula sha256 回填前是全零占位，install 会被校验拦下（防误装旧版）。

## crates.io 缓行

解冻条件：出现外部用户，或决定开源推广。届时先查 crate 名可用性，并把 `--json` schema 锁定测试作为发布门槛过一遍。
