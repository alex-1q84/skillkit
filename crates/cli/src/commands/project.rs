//! project 子命令：调 core 的 Project / apply。
use clap::{Args, Subcommand};
use skillkit_core::{detect_agents, list_project_ids, paths::Paths, Project};
use std::path::PathBuf;

#[derive(Args)]
pub struct ProjectCmd {
    #[command(subcommand)]
    cmd: ProjectSub,
}

#[derive(Subcommand)]
enum ProjectSub {
    /// 注册项目（生成随机 8 hex project-id）
    Add {
        path: PathBuf,
        #[arg(long, value_delimiter = ',')]
        agents: Option<Vec<String>>,
    },
    /// 重绑定：项目移动/改名后更新 path/name，id 不变
    Rebind { id: String, path: PathBuf },
    /// 注销项目：只删 skillkit 注册信息（toml），不碰项目目录本身
    Remove {
        project: String,
        /// 跳过交互确认
        #[arg(long)]
        yes: bool,
    },
    /// 扫描目录发现项目（只列 path，不自动注册）
    Scan {
        dir: PathBuf,
        #[arg(long, default_value = "3")]
        depth: u32,
    },
    /// 把 profile 的 skill 批量灌入 installed_skills
    ApplyProfile { project: String, profile: String },
    /// 精确加单个 skill
    AddSkill { project: String, id: String },
    /// 精确删单个 skill
    RemoveSkill { project: String, id: String },
    /// 幂等落地：按 installed_skills 同步到 agent 目录
    Apply {
        project: String,
        #[arg(long)]
        frozen: bool,
        #[arg(long)]
        json: bool,
    },
    /// 输出 diff（该有/缺/多/冲突）
    Status {
        project: String,
        #[arg(long)]
        json: bool,
    },
    /// 列出已注册项目
    List,
}

#[allow(clippy::too_many_lines)] // CLI dispatch，分支多固有长
pub fn run(cmd: ProjectCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        ProjectSub::Add { path, agents } => {
            let abs = path.canonicalize().unwrap_or_else(|_| path.clone());
            let agents = agents.unwrap_or_else(|| detect_agents(&abs));
            let proj = Project::register(abs, agents);
            let id = proj.id.clone();
            proj.save(&paths)?;
            println!("✓ 已注册项目 {id}");
        }
        ProjectSub::Rebind { id, path } => {
            let mut proj = Project::load(&paths, &id)?;
            proj.rebind(&path);
            proj.save(&paths)?;
            println!("✓ 已重绑定 {id} → {}", proj.path);
        }
        ProjectSub::Remove { project, yes } => run_remove(&paths, &project, yes)?,
        ProjectSub::Scan { dir, depth } => {
            let found = skillkit_core::scan_projects(&dir, depth)?;
            if found.is_empty() {
                println!("（未发现项目，project scan 只识别含 .git 的目录）");
            }
            for p in found {
                println!("{}", p.display());
            }
        }
        ProjectSub::ApplyProfile { project, profile } => {
            let mut proj = Project::load(&paths, &project)?;
            proj.refresh_agents();
            let p = skillkit_core::Profile::load(&paths, &profile)?;
            proj.apply_profile(&profile, &p.skills);
            proj.save(&paths)?;
            println!(
                "✓ {project} 已应用 profile {profile}（{} skills）",
                proj.installed_skills.len()
            );
        }
        ProjectSub::AddSkill { project, id } => {
            let registry = skillkit_core::Registry::load(&paths)?;
            let mut proj = Project::load(&paths, &project)?;
            proj.add_skill(&id, &registry)?;
            proj.save(&paths)?;
            println!("✓ {project} 已加 {id}");
        }
        ProjectSub::RemoveSkill { project, id } => {
            let mut proj = Project::load(&paths, &project)?;
            proj.remove_skill(&id)?;
            proj.save(&paths)?;
            println!("✓ {project} 已移除 {id}");
        }
        ProjectSub::Apply {
            project,
            frozen,
            json,
        } => {
            let mut proj = Project::load(&paths, &project)?;
            let report = skillkit_core::apply::run_apply(&paths, &mut proj, frozen)?;
            proj.save(&paths)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "✓ applied：{} created, {} removed, {} recopied, {} warnings",
                    report.created.len(),
                    report.removed.len(),
                    report.recopied.len(),
                    report.warnings.len()
                );
                for w in &report.warnings {
                    println!("  ⚠ {w}");
                }
            }
        }
        ProjectSub::Status { project, json } => {
            let proj = Project::load(&paths, &project)?;
            let reg = skillkit_core::Registry::load(&paths)?;
            let config = skillkit_core::config::Config::load(&paths)?;
            let diff = skillkit_core::apply::compute_diff(&proj, &reg, &config)?;
            let status = skillkit_core::apply::build_status(&paths, &proj, &diff)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                println!("expected:  {}", status.expected.join(", "));
                println!("missing:   {}", status.missing.join(", "));
                println!("extra:     {}", status.extra.join(", "));
                println!("conflicts: {}", status.conflicts.join(", "));
            }
        }
        ProjectSub::List => {
            for id in list_project_ids(&paths)? {
                let proj = Project::load(&paths, &id)?;
                println!(
                    "{:10} {} ({} skills)",
                    id,
                    proj.path,
                    proj.installed_skills.len()
                );
            }
        }
    }
    Ok(())
}

/// remove：skillkit project remove <project> [--yes]，注销项目（只删 toml 注册信息，不碰项目目录）。
/// 默认交互确认；--yes 跳过（CI/agent 友好）。对齐 skill remove 的确认模式。
fn run_remove(paths: &Paths, project: &str, yes: bool) -> anyhow::Result<()> {
    if !yes {
        println!("将注销项目 {project}（仅删 skillkit 注册信息，不碰项目目录），确认？(y/n)");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "y" {
            println!("已取消");
            return Ok(());
        }
    }
    Project::remove(paths, project)?;
    println!("✓ 已注销项目 {project}（注册信息已移除，项目目录未动）");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// 测试入口：ProjectSub 直接作为 subcommand 字段（mod tests 内可访问私有 enum）。
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ProjectSub,
    }

    /// `project remove <id> --yes`：解析出 project 位置参数 + yes=true（对齐 skill remove 的 CI/agent 契约）。
    #[test]
    fn remove_parses_project_and_yes() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "remove", "abc12345", "--yes"]);
        let ProjectSub::Remove { project, yes } = cmd else {
            panic!("expected Remove");
        };
        assert_eq!(project, "abc12345");
        assert!(yes);
    }

    /// 默认 yes=false（走交互确认），防误注销。
    #[test]
    fn remove_defaults_yes_false() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "remove", "abc12345"]);
        let ProjectSub::Remove { yes, .. } = cmd else {
            panic!("expected Remove");
        };
        assert!(!yes);
    }
}
