//! project 子命令：调 core 的 Project / apply。
use clap::{Args, Subcommand};
use skillkit_core::{config::Config, list_project_ids, paths::Paths, Project};
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

pub fn run(cmd: ProjectCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        ProjectSub::Add { path, agents } => {
            let abs = path.canonicalize().unwrap_or_else(|_| path.clone());
            let cfg = Config::load(&paths)?;
            let agents =
                agents.unwrap_or_else(|| cfg.agents.iter().map(|a| a.name.clone()).collect());
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
        ProjectSub::Scan { dir, depth } => {
            let found = scan_projects(&dir, depth)?;
            if found.is_empty() {
                println!("（未发现项目，project scan 只识别含 .git 的目录）");
            }
            for p in found {
                println!("{}", p.display());
            }
        }
        ProjectSub::ApplyProfile { project, profile } => {
            let mut proj = Project::load(&paths, &project)?;
            let p = skillkit_core::Profile::load(&paths, &profile)?;
            proj.apply_profile(&profile, &p.skills);
            proj.save(&paths)?;
            println!(
                "✓ {project} 已应用 profile {profile}（{} skills）",
                proj.installed_skills.len()
            );
        }
        ProjectSub::AddSkill { project, id } => {
            let mut proj = Project::load(&paths, &project)?;
            proj.add_skill(&id)?;
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
            let diff = skillkit_core::apply::compute_diff(&proj, &reg)?;
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

/// 扫描：找含 .git 的目录（depth 限制）。
fn scan_projects(dir: &std::path::Path, depth: u32) -> anyhow::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if dir.join(".git").exists() {
        found.push(dir.to_path_buf());
    }
    if depth > 0 {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && !p.starts_with(dir.join(".git")) {
                    found.extend(scan_projects(&p, depth - 1)?);
                }
            }
        }
    }
    Ok(found)
}
