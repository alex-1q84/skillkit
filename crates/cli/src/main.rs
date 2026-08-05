use clap::{Parser, Subcommand};

mod commands;

use commands::import::{run as run_import, ImportExistingCmd};
use commands::install::InstallCmd;
use commands::profile::ProfileCmd;
use commands::project::ProjectCmd;
use commands::rescope::RescopeCmd;
use commands::serve::ServeCmd;
use commands::skill::{FindCmd, ListCmd, RemoveCmd};
use commands::source::SourceCmd;
use commands::upgrade::{run as run_upgrade, UpgradeCmd};

#[derive(Parser)]
#[command(name = "skillkit", about = "AI agent skill 统一管理工具")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// skill 安装源管理
    Source(SourceCmd),
    /// skill 安装到 canonical 存储
    Install(InstallCmd),
    /// 搜 skills.sh registry 中的 skill 候选
    Find(FindCmd),
    /// 列出全部已装 skill
    List(ListCmd),
    /// 卸载 skill（替换 uninstall）
    Remove(RemoveCmd),
    /// 转移 skill scope（global↔local）
    Rescope(RescopeCmd),
    /// 扫描导入现有 skill（存量目录登记进 registry）
    ImportExisting(ImportExistingCmd),
    /// profile 候选集管理
    Profile(ProfileCmd),
    /// project 精确管理
    Project(ProjectCmd),
    /// 升级 skill 到最新版本
    Upgrade(UpgradeCmd),
    /// 本地 web GUI
    Serve(ServeCmd),
}

fn main() -> anyhow::Result<()> {
    // 启动确保默认源种子（sources.toml 不存在则写入 skills.sh registry 入口）
    skillkit_core::SourcesStore::ensure_default(&skillkit_core::Paths::production())?;
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Source(cmd) => commands::source::run(cmd)?,
        Cmd::Install(cmd) => commands::install::run_install(cmd)?,
        Cmd::Find(cmd) => commands::skill::run_find(cmd)?,
        Cmd::List(cmd) => commands::skill::run_list(cmd)?,
        Cmd::Remove(cmd) => commands::skill::run_remove(cmd)?,
        Cmd::Rescope(cmd) => commands::rescope::run_rescope(cmd)?,
        Cmd::ImportExisting(cmd) => run_import(cmd)?,
        Cmd::Profile(cmd) => commands::profile::run(cmd)?,
        Cmd::Project(cmd) => commands::project::run(cmd)?,
        Cmd::Upgrade(cmd) => run_upgrade(cmd)?,
        Cmd::Serve(cmd) => commands::serve::run(cmd)?,
    }
    Ok(())
}
