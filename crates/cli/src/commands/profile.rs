//! profile 子命令：调 core 的 Profile。
use clap::{Args, Subcommand};
use skillkit_core::{list_profile_names, paths::Paths, Profile, Registry};

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
    /// 列出 profile 内 skill 明细
    Show {
        profile: String,
        /// JSON 输出：{name, description, skills}
        #[arg(long)]
        json: bool,
    },
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
            let registry = Registry::load(&paths)?;
            let mut p = Profile::load(&paths, &profile)?;
            p.add_skill(&id, &registry)?;
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
        ProfileSub::Show { profile, json } => {
            let p = Profile::load(&paths, &profile)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else if p.skills.is_empty() {
                println!("{profile}（0 skills）{}", p.description);
            } else {
                println!("{profile}（{} skills）{}", p.skills.len(), p.description);
                for id in &p.skills {
                    println!("  {id}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// 测试入口：ProfileSub 私有，mod tests 内直接作 subcommand 字段。
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ProfileSub,
    }

    /// `profile show <name> --json`：位置参数 + flag 解析。
    #[test]
    fn show_parses_profile_and_json() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "show", "fe", "--json"]);
        let ProfileSub::Show { profile, json } = cmd else {
            panic!("expected Show")
        };
        assert_eq!(profile, "fe");
        assert!(json);
    }

    #[test]
    fn show_defaults_json_false() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "show", "fe"]);
        let ProfileSub::Show { json, .. } = cmd else {
            panic!("expected Show")
        };
        assert!(!json);
    }

    /// --json schema 锁定：Profile 序列化为 {name, description, skills}。
    #[test]
    fn show_json_schema_locks_profile_fields() {
        let p = Profile {
            name: "fe".into(),
            description: "前端".into(),
            skills: vec!["skills.sh/fe".into()],
        };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(
            json,
            r#"{"name":"fe","description":"前端","skills":["skills.sh/fe"]}"#
        );
    }
}
