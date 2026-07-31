//! install/uninstall 子命令：调 core 的 install/uninstall。
//! registry 源（package=None，即 skills.sh）install 时走 npx skills find 交互选候选。
use clap::{Args, Subcommand};
use skillkit_core::{install, npx, paths::Paths, registry::Scope, source::SourcesStore, uninstall};
use std::io::{self, Write};

#[derive(Args)]
pub struct InstallCmd {
    #[command(subcommand)]
    cmd: InstallSub,
}

#[derive(Subcommand)]
enum InstallSub {
    /// 安装 skill：skillkit install add <source> <skill> [--scope global|local]
    /// 固定源直接装；skills.sh（registry）源走 find 选候选
    Add {
        source: String,
        skill: String,
        #[arg(long, value_parser = parse_scope, default_value = "local")]
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

/// registry 源（package=None）install：find 候选，交互式选返回 spec（owner/repo@skill）。
fn resolve_registry_package(paths: &Paths, skill: &str) -> anyhow::Result<String> {
    let candidates = npx::find(paths, skill)?;
    if candidates.is_empty() {
        anyhow::bail!("在 skills.sh 未找到 skill：{skill}");
    }
    println!("在 skills.sh 找到 {} 个候选：", candidates.len());
    for (i, c) in candidates.iter().take(20).enumerate() {
        println!("  [{}] {}  {}", i, c.spec, c.url.as_deref().unwrap_or(""));
    }
    print!("选择序号（默认 0）：");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let idx: usize = line.trim().parse().unwrap_or(0);
    candidates
        .get(idx)
        .map(|c| c.spec.clone())
        .ok_or_else(|| anyhow::anyhow!("无效序号：{idx}"))
}

pub fn run_install(cmd: InstallCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        InstallSub::Add {
            source,
            skill,
            scope,
        } => {
            let store = SourcesStore::load(&paths)?;
            let src = store.get(&source)?.clone();
            let package = match src.package {
                Some(p) => p,
                None => resolve_registry_package(&paths, &skill)?,
            };
            let meta = install(&paths, &source, &skill, &package, scope)?;
            println!(
                "✓ 已安装 {}（hash: {}）",
                meta.id,
                meta.computed_hash.as_deref().unwrap_or("?")
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
