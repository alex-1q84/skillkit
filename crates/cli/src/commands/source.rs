//! source 子命令：调 core 的 SourcesStore。
use clap::{Args, Subcommand};
use skillkit_core::{
    paths::Paths,
    source::{Source, SourceType, SourcesStore},
};

#[derive(Args)]
pub struct SourceCmd {
    #[command(subcommand)]
    cmd: SourceSub,
}

#[derive(Subcommand)]
enum SourceSub {
    /// 添加源：skillkit source add <name> <skills-sh|git|local> [target] [--ref X] [--skills-dir D]
    Add {
        name: String,
        #[arg(value_parser = parse_type)]
        source_type: SourceType,
        /// git url 或 local path（skills-sh 源可省略）
        target: Option<String>,
        #[arg(long)]
        r#ref: Option<String>,
        /// skill 在仓库中的子目录（一仓库多 skill，如 skills）；省略=skill 在仓库根
        #[arg(long)]
        skills_dir: Option<String>,
    },
    /// 列出所有源
    List,
    /// 移除源
    Remove { name: String },
}

fn parse_type(s: &str) -> Result<SourceType, String> {
    match s {
        "skills-sh" => Ok(SourceType::SkillsSh),
        "git" => Ok(SourceType::Git),
        "local" => Ok(SourceType::Local),
        other => Err(format!(
            "未知源类型：{other}（可选 skills-sh / git / local）"
        )),
    }
}

pub fn run(cmd: SourceCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        SourceSub::Add {
            name,
            source_type,
            target,
            r#ref,
            skills_dir,
        } => {
            let mut store = SourcesStore::load(&paths)?;
            let source = Source {
                name,
                source_type,
                url: if matches!(source_type, SourceType::Git) {
                    target.clone()
                } else {
                    None
                },
                path: if matches!(source_type, SourceType::Local) {
                    target.clone()
                } else {
                    None
                },
                ref_: r#ref,
                skills_dir,
            };
            store.add(source)?;
            store.save(&paths)?;
            println!("✓ 已添加源");
        }
        SourceSub::List => {
            let store = SourcesStore::load(&paths)?;
            if store.list().is_empty() {
                println!("（暂无源，先 `skillkit source add` 添加）");
            }
            for s in store.list() {
                let kind = match s.source_type {
                    SourceType::SkillsSh => "skills-sh",
                    SourceType::Git => "git",
                    SourceType::Local => "local",
                };
                let target = s.url.clone().or(s.path.clone()).unwrap_or_default();
                let sdir = s.skills_dir.clone().unwrap_or_else(|| "-".into());
                println!("{:16} {:10} skills_dir={:8} {}", s.name, kind, sdir, target);
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
