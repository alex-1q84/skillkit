# skillkit 发版流程

> 分发渠道：GitHub Release 预编译二进制 + 公开 tap [alex-1q84/homebrew-tap](https://github.com/alex-1q84/homebrew-tap)（不进 crates.io，解冻条件见文末）。
> 只发 aarch64（Apple Silicon）。tarball 由 CI 打包：`skillkit-<ver>-aarch64-apple-darwin.tar.gz` + `sha256.txt`。
> 打 tag 后 CI 自动把新版本号 + sha256 写入 tap 的 `Formula/skillkit.rb`（用仓库 secret `TAP_TOKEN`，一个对 homebrew-tap 有写权限的 PAT）。

## 步骤

1. 本地 `make check` 全绿。check 首步已内置工具链检查（`make toolchain`：`rustup check` 发现本地 stable 落后即拦截），被拦时跑 `rustup update stable` 后重试。
2. commit 并 push main。
3. 打 tag 并推送：`git tag vX.Y.Z && git push origin vX.Y.Z`。CI 自动构建、创建 Release（附 tarball 与 sha256.txt），随后自动更新 tap formula（约 2 分钟，进度看 Actions）。
4. 升级验证：`brew update && brew upgrade skillkit && skillkit --version` 输出新版本号。

## 版本号约定

pre-1.0 阶段（0.x.y）：`--json` 输出结构或 CLI 参数语义变更 bump x，行为修复与小功能 bump y。

## 坑位备忘

- CI runner 每次全新拉最新 stable 工具链，pedantic lint 随版本增长，本地长期不 `rustup update` 会在 CI 上爆出新 error（2026-08 v0.1.0 首发即因此挂过，11 处 map_unwrap_or 等；2026-09 v0.1.6 复发一次——本地 1.97 全绿、CI 1.98 爆 `Result::ok().is_some_and` 新 lint，tag 已推只能删 tag 重打。现已由 `make check` 的 toolchain 步前置拦截）。
- 第三方 tap 需 `brew trust`（一次性）：`brew trust alex-1q84/tap`。
- CI 的 tap 更新步骤在 secret `TAP_TOKEN` 未配置时静默跳过——release 正常出但 tap 不更新，发版后记得核对 tap 的 HEAD。
- tap formula 由 CI 全量生成（模板在 `.github/workflows/ci.yml` 的 tap 步骤里），手工改 tap 仓库的 formula 会在下次发版被覆盖。

## crates.io 缓行

解冻条件：出现外部用户，或决定开源推广。届时先查 crate 名可用性，并把 `--json` schema 锁定测试作为发布门槛过一遍。
