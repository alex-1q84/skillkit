//! source 子命令：调 core 的 SourcesStore。Source 极简成 {name, package}。
use clap::{Args, Subcommand};
use skillkit_core::{paths::Paths, source::SourcesStore, Source};

#[derive(Args)]
pub struct SourceCmd {
    #[command(subcommand)]
    cmd: SourceSub,
}

#[derive(Subcommand)]
enum SourceSub {
    /// 添加源：skillkit source add <name> [package]（package 省略=registry 搜索入口 skills.sh）
    Add {
        name: String,
        /// npx skills package（github shorthand / git url / local path）；省略=registry 搜索入口
        package: Option<String>,
    },
    /// 列出所有源
    List,
    /// 移除源
    Remove { name: String },
}

pub fn run(cmd: SourceCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        SourceSub::Add { name, package } => {
            let mut store = SourcesStore::load(&paths)?;
            store.add(Source { name, package })?;
            store.save(&paths)?;
            println!("✓ 已添加源");
        }
        SourceSub::List => {
            let store = SourcesStore::load(&paths)?;
            if store.list().is_empty() {
                println!("（暂无源，先 `skillkit source add` 添加）");
            }
            for s in store.list() {
                let pkg = s
                    .package
                    .clone()
                    .unwrap_or_else(|| "（registry 搜索入口）".into());
                println!("{:16} {}", s.name, pkg);
            }
        }
        SourceSub::Remove { name } => {
            let mut store = SourcesStore::load(&paths)?;
            store.remove(&name)?.save(&paths)?;
            println!("✓ 已移除源：{name}");
        }
    }
    Ok(())
}
