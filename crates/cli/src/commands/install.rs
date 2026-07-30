//! install/uninstall 子命令：调 core 的 install/uninstall。
use clap::{Args, Subcommand};
use skillkit_core::{install, paths::Paths, registry::Scope, uninstall};

#[derive(Args)]
pub struct InstallCmd {
    #[command(subcommand)]
    cmd: InstallSub,
}

#[derive(Subcommand)]
enum InstallSub {
    /// 安装 skill：skillkit install add <source> <skill> [--scope global|local]
    /// （spec §11 的 `install <id>` 简写：id = <source>/<skill>，本实现用 add 子命令显式拆分两个参数）
    Add {
        source: String,
        skill: String,
        #[arg(long, value_parser = parse_scope, default_value = "global")]
        scope: Scope,
    },
}

#[derive(Args)]
pub struct UninstallCmd {
    /// skill id，格式 <source>/<skill>
    pub id: String,
}

fn parse_scope(s: &str) -> Result<Scope, String> {
    match s {
        "global" => Ok(Scope::Global),
        "local" => Ok(Scope::Local),
        other => Err(format!("未知 scope：{other}（可选 global / local）")),
    }
}

pub fn run_install(cmd: InstallCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        InstallSub::Add {
            source,
            skill,
            scope,
        } => {
            let meta = install(&paths, &source, &skill, scope)?;
            println!(
                "✓ 已安装 {}（sha: {}）",
                meta.id,
                meta.commit_sha.as_deref().unwrap_or("?")
            );
        }
    }
    Ok(())
}

pub fn run_uninstall(cmd: UninstallCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    uninstall(&paths, &cmd.id)?;
    println!("✓ 已卸载 {}", cmd.id);
    Ok(())
}
