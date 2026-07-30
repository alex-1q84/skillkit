use clap::{Parser, Subcommand};

mod commands;

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
    /// skill 安装到 canonical 存储（M0 待实现）
    Install,
    /// 卸载 skill（M0 待实现）
    Uninstall,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Source(cmd) => commands::source::run(cmd)?,
        Cmd::Install | Cmd::Uninstall => println!("（M0 待实现）"),
    }
    Ok(())
}
