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
    /// 删除 profile（先解绑所有绑定它的项目，再删文件）
    Delete {
        profile: String,
        /// 跳过交互确认
        #[arg(long)]
        yes: bool,
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
        ProfileSub::Delete { profile, yes } => {
            // 危险操作默认交互确认；--yes 跳过（CI/agent 友好）。对齐 skill/project remove 的确认模式。
            if !yes {
                println!("将删除 profile {profile}（所有绑定它的项目会解绑），确认？(y/n)");
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                if line.trim() != "y" {
                    println!("已取消");
                    return Ok(());
                }
            }
            let report = skillkit_core::remove_profile(&paths, &profile)?;
            println!("✓ 已删除 profile：{profile}");
            if !report.unbound.is_empty() {
                println!("  已解绑项目：{}", report.unbound.join("、"));
            }
            if !report.fallback.is_empty() {
                println!(
                    "  {} 落地失败仅清除绑定记录，项目内残留文件下次 apply 时清理",
                    report.fallback.join("、")
                );
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

    /// `profile delete <name> --yes`：位置参数 + flag 解析（CI/agent 契约，对齐 remove 系列）。
    #[test]
    fn delete_parses_profile_and_yes() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "delete", "fe", "--yes"]);
        let ProfileSub::Delete { profile, yes } = cmd else {
            panic!("expected Delete")
        };
        assert_eq!(profile, "fe");
        assert!(yes);
    }

    #[test]
    fn delete_defaults_yes_false() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "delete", "fe"]);
        let ProfileSub::Delete { yes, .. } = cmd else {
            panic!("expected Delete")
        };
        assert!(!yes, "默认走交互确认");
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
