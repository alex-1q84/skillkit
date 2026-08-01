//! skill 实体的查询与移除：find（搜 skills.sh）/ list（列已装）/ remove（卸载，替换 uninstall）。
//! 复用 core 的 npx::find / Registry / uninstall，cli 只做薄壳与展示。
use clap::Args;
use skillkit_core::{npx, paths::Paths};

/// find：skillkit find <query> [--json]，搜 skills.sh registry，纯展示候选不安装。
#[derive(Args)]
pub struct FindCmd {
    /// skill 名（搜 skills.sh registry）
    pub query: String,
    /// JSON 输出：候选数组 [{spec,url}]
    #[arg(long)]
    pub json: bool,
}

/// 输出 find 候选：json=true 序列化数组，否则编号列表。install 的 registry 源 --json 分支也复用。
pub fn print_candidates(paths: &Paths, query: &str, json: bool) -> anyhow::Result<()> {
    let cs = npx::find(paths, query)?;
    if cs.is_empty() {
        anyhow::bail!("在 skills.sh 未找到 skill：{query}");
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&cs)?);
    } else {
        println!("在 skills.sh 找到 {} 个候选：", cs.len());
        for (i, c) in cs.iter().take(20).enumerate() {
            println!("  [{i}] {}  {}", c.spec, c.url.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn run_find(cmd: FindCmd) -> anyhow::Result<()> {
    print_candidates(&Paths::production(), &cmd.query, cmd.json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};
    use skillkit_core::Candidate;

    /// 测试入口：自建同形 Parser 解析顶层命令（main.rs 的 Cli 私有，这里复刻命令变体）。
    /// 后续 task 给 TestCmd 累积追加 List/Remove 变体。
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(Subcommand)]
    enum TestCmd {
        Find(FindCmd),
    }

    #[test]
    fn find_parses_query_and_json() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "find", "pdf", "--json"]);
        let TestCmd::Find(FindCmd { query, json }) = cmd;
        assert_eq!(query, "pdf");
        assert!(json);
    }

    #[test]
    fn find_defaults_json_false() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "find", "pdf"]);
        let TestCmd::Find(FindCmd { json, .. }) = cmd;
        assert!(!json);
    }

    /// --json schema 锁定：Candidate 序列化为 {"spec","url"}（纯序列化契约，不依赖 npx）。
    #[test]
    fn find_json_schema_locks_candidate_fields() {
        let cs = vec![
            Candidate {
                spec: "anthropics/skills@pdf".into(),
                url: Some("https://skills.sh/a".into()),
            },
            Candidate {
                spec: "openai/skills@pdf".into(),
                url: None,
            },
        ];
        let json = serde_json::to_string(&cs).unwrap();
        assert_eq!(
            json,
            r#"[{"spec":"anthropics/skills@pdf","url":"https://skills.sh/a"},{"spec":"openai/skills@pdf","url":null}]"#
        );
    }
}
