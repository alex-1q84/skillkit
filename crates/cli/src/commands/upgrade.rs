//! upgrade 子命令：skillkit upgrade <id> | --all [--yes] [--json]。
//! 单挑冲突时 core 返回 UpgradeBlocked，人类模式打印受影响项目并 y/n 交互确认；--json 输出错误 JSON。
//! --all 冲突不中断也不静默：升级可升级的，被拦截的进 blocked 列出受影响项目（不交互）。
use clap::Parser;
use skillkit_core::{paths::Paths, SkillkitError};

#[derive(Parser)]
pub struct UpgradeCmd {
    /// skill id（升级单个；与 --all 互斥）
    id: Option<String>,
    /// 升级 registry 全部 skill（跳过 unmanaged / 未安装）
    #[arg(long)]
    all: bool,
    /// 跳过冲突确认
    #[arg(long)]
    yes: bool,
    /// JSON 输出
    #[arg(long)]
    json: bool,
}

pub fn run(cmd: UpgradeCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match (cmd.id, cmd.all) {
        (Some(id), false) => run_one(&paths, &id, cmd.yes, cmd.json),
        (None, true) => run_all(&paths, cmd.yes, cmd.json),
        (None, false) => anyhow::bail!("upgrade 需指定 <id> 或 --all"),
        (Some(_), true) => anyhow::bail!("<id> 与 --all 互斥"),
    }
}

fn run_one(paths: &Paths, id: &str, yes: bool, json: bool) -> anyhow::Result<()> {
    match skillkit_core::upgrade_skill(paths, id, yes) {
        Ok(r) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                println!(
                    "✓ 已升级 {} {} → {}",
                    r.id,
                    short(&r.old_hash),
                    short(&r.new_hash)
                );
                for p in &r.affected_projects {
                    println!("  ⚠ 项目 {p} 需重新 apply");
                }
            }
            Ok(())
        }
        Err(SkillkitError::UpgradeBlocked { id, affected }) => {
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({"error": "upgrade_blocked", "id": id, "affected": affected})
                );
                std::process::exit(1);
            }
            println!(
                "⚠ 升级 {id} 将影响 {} 个项目（{}），它们的版本基线会漂移",
                affected.len(),
                affected.join(", ")
            );
            print!("确认升级？[y/N] ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            if line.trim().eq_ignore_ascii_case("y") {
                run_one(paths, &id, true, json)
            } else {
                anyhow::bail!("已取消升级 {id}");
            }
        }
        Err(e) => Err(e.into()),
    }
}

fn run_all(paths: &Paths, yes: bool, json: bool) -> anyhow::Result<()> {
    let all = skillkit_core::upgrade_all(paths, yes)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&all)?);
    } else {
        println!("已升级 {} 个 skill", all.upgraded.len());
        for r in &all.upgraded {
            println!(
                "  ✓ {} {} → {}",
                r.id,
                short(&r.old_hash),
                short(&r.new_hash)
            );
            for p in &r.affected_projects {
                println!("    ⚠ 项目 {p} 需重新 apply");
            }
        }
        // 冲突拦截不静默：列出受影响项目，并给出下一步（反馈引导行动）
        for b in &all.blocked {
            println!(
                "⚠ 跳过 {}：升级将影响项目 {}（如需升级请 skillkit upgrade {}）",
                b.id,
                b.affected.join(", "),
                b.id
            );
        }
    }
    Ok(())
}

fn short(h: &str) -> &str {
    h.get(..8).unwrap_or(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_parses_id_and_flags() {
        let cmd = UpgradeCmd::try_parse_from(["skillkit", "dc/foo", "--yes", "--json"]).unwrap();
        assert_eq!(cmd.id.as_deref(), Some("dc/foo"));
        assert!(!cmd.all);
        assert!(cmd.yes);
        assert!(cmd.json);
    }

    #[test]
    fn upgrade_parses_all() {
        let cmd = UpgradeCmd::try_parse_from(["skillkit", "--all"]).unwrap();
        assert!(cmd.all);
        assert!(cmd.id.is_none());
    }

    #[test]
    fn upgrade_rejects_missing_target() {
        let cmd = UpgradeCmd::try_parse_from(["skillkit"]).unwrap();
        // 无 id 也无 --all：run 时手工校验报错
        assert!(cmd.id.is_none());
        assert!(!cmd.all);
    }
}
