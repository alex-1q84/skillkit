//! profile 子命令：调 core 的 Profile。
use clap::{Args, Subcommand};
use skillkit_core::{list_profile_names, paths::Paths, Profile};

#[derive(Args)]
pub struct ProfileCmd {
    #[command(subcommand)]
    cmd: ProfileSub,
}

#[derive(Subcommand)]
enum ProfileSub {
    /// 创建空 profile
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// profile 加 skill（id = <source>/<skill>）
    AddSkill { profile: String, id: String },
    /// profile 移除 skill
    RemoveSkill { profile: String, id: String },
    /// 列出所有 profile
    List,
}

pub fn run(cmd: ProfileCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        ProfileSub::Create { name, description } => {
            Profile {
                name: name.clone(),
                description: description.unwrap_or_default(),
                skills: vec![],
            }
            .save(&paths)?;
            println!("✓ 已创建 profile：{name}");
        }
        ProfileSub::AddSkill { profile, id } => {
            let mut p = Profile::load(&paths, &profile)?;
            p.add_skill(&id)?;
            p.save(&paths)?;
            println!("✓ {profile} 已加 {id}");
        }
        ProfileSub::RemoveSkill { profile, id } => {
            let mut p = Profile::load(&paths, &profile)?;
            p.remove_skill(&id)?;
            p.save(&paths)?;
            println!("✓ {profile} 已移除 {id}");
        }
        ProfileSub::List => {
            for name in list_profile_names(&paths)? {
                let p = Profile::load(&paths, &name)?;
                println!("{:16} ({} skills) {}", name, p.skills.len(), p.description);
            }
        }
    }
    Ok(())
}
