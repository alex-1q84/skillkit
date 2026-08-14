//! import-existing 子命令：扫存量 skill 目录登记进 registry（dry-run 只输出不写）。
use clap::Parser;
use skillkit_core::{import_existing, paths::Paths};

#[derive(Parser)]
pub struct ImportExistingCmd {
    /// 只输出不写 registry
    #[arg(long)]
    dry_run: bool,
    /// JSON 输出 ImportReport
    #[arg(long)]
    json: bool,
}

pub fn run(cmd: ImportExistingCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    let report = import_existing(&paths, cmd.dry_run)?;
    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "imported {}（入池迁址 {}，含存量补迁 {}），reinstalled {}，skipped {}",
            report.imported.len(),
            report.relocated.len(),
            report.relinked.len(),
            report.reinstalled.len(),
            report.skipped.len()
        );
        for s in &report.skipped {
            println!("  - {s}");
        }
        if cmd.dry_run {
            println!("（dry-run，未写入 registry）");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn import_existing_parses_dry_run_and_json() {
        let cmd = ImportExistingCmd::parse_from(["skillkit", "--dry-run", "--json"]);
        assert!(cmd.dry_run);
        assert!(cmd.json);
    }

    #[test]
    fn import_existing_defaults_false() {
        let cmd = ImportExistingCmd::parse_from(["skillkit"]);
        assert!(!cmd.dry_run);
        assert!(!cmd.json);
    }

    #[test]
    fn import_json_schema_locks_fields() {
        let json = serde_json::json!({
            "imported": ["foo"],
            "unmanaged": ["foo"],
            "reinstalled": [],
            "skipped": [],
            "relocated": ["foo"],
            "relinked": ["bar"],
        });
        let s = json.to_string();
        for f in [
            "\"imported\"",
            "\"unmanaged\"",
            "\"reinstalled\"",
            "\"skipped\"",
            "\"relocated\"",
            "\"relinked\"",
        ] {
            assert!(s.contains(f), "import --json schema 应含 {f}：{s}");
        }
    }
}
