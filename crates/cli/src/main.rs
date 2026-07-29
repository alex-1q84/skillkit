use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "skillkit", about = "AI agent skill 统一管理工具")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// skill 安装源管理（M0 待实现）
    Source,
    /// skill 安装到 canonical 存储（M0 待实现）
    Install,
    /// 卸载 skill（M0 待实现）
    Uninstall,
}

fn main() {
    match Cli::parse().cmd {
        Cmd::Source => println!("source 子命令：M0 待实现"),
        Cmd::Install => println!("install 子命令：M0 待实现"),
        Cmd::Uninstall => println!("uninstall 子命令：M0 待实现"),
    }
}
