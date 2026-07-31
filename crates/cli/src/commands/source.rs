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
    /// 添加源：skillkit source add <package> [--name <别名>]
    /// package 为 npx skills source format（github shorthand / git url / local path）；
    /// 名称默认从 package 推导（repo 名 / 目录名），--name 覆盖。
    Add {
        /// npx skills package（github shorthand / git url / local path）
        package: String,
        /// 源名称（覆盖自动推导）；缺省时取 repo 名 / 目录名
        #[arg(long)]
        name: Option<String>,
    },
    /// 列出所有源
    List,
    /// 移除源
    Remove { name: String },
}

pub fn run(cmd: SourceCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        SourceSub::Add { package, name } => {
            let name = match name {
                Some(n) if !n.trim().is_empty() => n.trim().to_string(),
                _ => match skillkit_core::derive_source_name(&package) {
                    Some(n) => n,
                    None => anyhow::bail!("package 不能为空（缺少可推导的名称，可用 --name 指定）"),
                },
            };
            let mut store = SourcesStore::load(&paths)?;
            if store.get(&name).is_ok() {
                anyhow::bail!("该名称已被源 {name} 占用（可用 --name 指定别名）");
            }
            store.add(Source {
                name,
                package: Some(package),
            })?;
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
