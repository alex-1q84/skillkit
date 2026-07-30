use clap::{Parser, Subcommand};

mod commands;

use commands::install::{InstallCmd, UninstallCmd};
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Source(cmd) => commands::source::run(cmd)?,
        Cmd::Install(cmd) => commands::install::run_install(cmd)?,
        Cmd::Uninstall(cmd) => commands::install::run_uninstall(cmd)?,
    }
    Ok(())
}
