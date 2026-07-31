use clap::{Parser, Subcommand};

mod commands;

use commands::install::{InstallCmd, UninstallCmd};
use commands::profile::ProfileCmd;
use commands::project::ProjectCmd;
use commands::serve::ServeCmd;
use commands::source::SourceCmd;

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
    /// 卸载 skill
    Uninstall(UninstallCmd),
    /// profile 候选集管理
    Profile(ProfileCmd),
    /// project 精确管理
    Project(ProjectCmd),
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
        Cmd::Uninstall(cmd) => commands::install::run_uninstall(cmd)?,
        Cmd::Profile(cmd) => commands::profile::run(cmd)?,
        Cmd::Project(cmd) => commands::project::run(cmd)?,
        Cmd::Serve(cmd) => commands::serve::run(cmd)?,
    }
    Ok(())
}
