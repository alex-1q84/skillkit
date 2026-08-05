//! rescope：skillkit rescope <id> <global|local> [--yes] [--json]，转移 scope + 同步物理落地。
use clap::Args;
use skillkit_core::{paths::Paths, set_scope, Registry, Scope};

#[derive(Args)]
pub struct RescopeCmd {
    /// skill id，格式 <source>/<skill>
    pub id: String,
    /// 目标 scope：global | local
    pub scope: ScopeArg,
    #[arg(long)]
    pub yes: bool,
    /// JSON 输出（隐含 --yes）：{id, from, to, affected_profiles, affected_projects}
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Debug)]
pub enum ScopeArg {
    Global,
    Local,
}

impl std::str::FromStr for ScopeArg {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "global" => Ok(Self::Global),
            "local" => Ok(Self::Local),
            _ => Err(format!("scope 必须是 global|local，得到 {s}")),
        }
    }
}

pub fn run_rescope(cmd: RescopeCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    let target = match cmd.scope {
        ScopeArg::Global => Scope::Global,
        ScopeArg::Local => Scope::Local,
    };
    let from = Registry::load(&paths)?.get(&cmd.id)?.scope;

    let skip_confirm = cmd.yes || cmd.json;
    if !skip_confirm {
        let (dir, hint) = match (from, target) {
            (Scope::Local, Scope::Global) => {
                ("local→global", "（将移除 profile/project 引用，不可撤销）")
            }
            (Scope::Global, Scope::Local) => {
                ("global→local", "（将撤销全局落地，可 rescope global 恢复）")
            }
            _ => ("(无变化)", ""),
        };
        println!("将 rescope {} {}{}，确认？(y/n)", cmd.id, dir, hint);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "y" {
            println!("已取消");
            return Ok(());
        }
    }

    let report = set_scope(&paths, &cmd.id, target)?;

    if cmd.json {
        println!(
            "{}",
            serde_json::json!({
                "id": cmd.id,
                "from": from.to_string(),
                "to": target.to_string(),
                "affected_profiles": report.affected_profiles,
                "affected_projects": report.affected_projects,
            })
        );
    } else {
        println!("✓ 已 rescope {} {}→{}", cmd.id, from, target);
        if !report.affected_profiles.is_empty() {
            println!("  从 profile 移除：{}", report.affected_profiles.join(", "));
        }
        if !report.affected_projects.is_empty() {
            println!(
                "  从项目移除：{}（需重新 apply 清理目录残留）",
                report.affected_projects.join(", ")
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(Subcommand)]
    enum TestCmd {
        Rescope(RescopeCmd),
    }

    #[test]
    fn rescope_parses_id_scope_flags() {
        let TestCli { cmd } =
            TestCli::parse_from(["skillkit", "rescope", "dc/fe", "global", "--yes", "--json"]);
        let TestCmd::Rescope(c) = cmd; // TestCmd 单变体，模式 irrefutable
        assert_eq!(c.id, "dc/fe");
        assert!(matches!(c.scope, ScopeArg::Global));
        assert!(c.yes && c.json);
    }

    /// --json schema 锁定：字段名 + from/to 为 lowercase scope 字符串。
    #[test]
    fn rescope_json_schema_locks_fields() {
        let json = serde_json::json!({
            "id": "dc/fe",
            "from": "local",
            "to": "global",
            "affected_profiles": ["fe"],
            "affected_projects": ["P1"],
        });
        assert_eq!(json["from"], "local");
        assert_eq!(json["to"], "global");
        assert_eq!(json["affected_profiles"][0], "fe");
        assert_eq!(json["affected_projects"][0], "P1");
    }
}
