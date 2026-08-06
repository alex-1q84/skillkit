//! agent 类型探测：精确判定项目实际使用的 agent，避免注册/绑定时默认给全部 agent 建目录。
//!
//! 探测顺序（对项目根目录）：
//! 1. 配置目录优先：`.claude` / `.codex` / `.cursor` / `.agents`，命中任一即按目录判定；
//! 2. 无配置目录时按指令文件判定：`CLAUDE.md` → claude-code，`AGENTS.md` → 开源标准；
//! 3. 全部未命中回退开源标准 `.agents/`（OpenCode/Gemini 等通用目录）。
use std::path::Path;

/// 开源标准 agent 名：落地 `.agents/skills/`（各 agent 通用目录，非某家私有）。
pub const OPEN_STANDARD_AGENT: &str = "agents";

/// 阶段 1 探测表：agent 名 → 项目内配置目录名。
const CONFIG_DIRS: &[(&str, &str)] = &[
    ("claude-code", ".claude"),
    ("codex", ".codex"),
    ("cursor", ".cursor"),
    (OPEN_STANDARD_AGENT, ".agents"),
];

/// 阶段 2 探测表：agent 名 → 项目内指令文件名。
const INSTRUCTION_FILES: &[(&str, &str)] = &[
    ("claude-code", "CLAUDE.md"),
    (OPEN_STANDARD_AGENT, "AGENTS.md"),
];

/// 探测项目实际使用的 agent 集合（保序去重，命中多个全部返回）。
pub fn detect_agents(project_root: &Path) -> Vec<String> {
    // 配置目录优先：命中任一即按目录判定，不再叠加指令文件（目录是更强信号）。
    let by_dirs = CONFIG_DIRS
        .iter()
        .filter(|(_, dir)| project_root.join(dir).exists())
        .map(|(agent, _)| (*agent).to_string())
        .collect::<Vec<_>>();
    if !by_dirs.is_empty() {
        return by_dirs;
    }
    // 无配置目录时按指令文件判定（如 Codex 项目只有 AGENTS.md、无 .codex 目录）。
    let by_files = INSTRUCTION_FILES
        .iter()
        .filter(|(_, file)| project_root.join(file).exists())
        .map(|(agent, _)| (*agent).to_string())
        .collect::<Vec<_>>();
    if !by_files.is_empty() {
        return by_files;
    }
    // 全部未命中 → 回退开源标准 `.agents/`，只建这一个目录。
    vec![OPEN_STANDARD_AGENT.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn root_with(names: &[&str]) -> std::path::PathBuf {
        let tmp = tempdir().unwrap();
        for n in names {
            let p = tmp.path().join(n);
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
                std::fs::write(&p, "# marker").unwrap();
            } else {
                std::fs::create_dir_all(&p).unwrap();
            }
        }
        tmp.keep()
    }

    #[test]
    fn config_dir_detects_single_agent() {
        let root = root_with(&[".claude"]);
        assert_eq!(detect_agents(&root), vec!["claude-code"]);
    }

    #[test]
    fn config_dir_detects_codex_and_cursor() {
        let root = root_with(&[".codex"]);
        assert_eq!(detect_agents(&root), vec!["codex"]);
        let root = root_with(&[".cursor"]);
        assert_eq!(detect_agents(&root), vec!["cursor"]);
    }

    #[test]
    fn multiple_config_dirs_all_detected_in_table_order() {
        let root = root_with(&[".cursor", ".claude"]);
        assert_eq!(
            detect_agents(&root),
            vec!["claude-code", "cursor"],
            "多个配置目录全部命中，按探测表顺序"
        );
    }

    #[test]
    fn open_standard_agents_dir_detected() {
        let root = root_with(&[".agents"]);
        assert_eq!(detect_agents(&root), vec!["agents"]);
    }

    #[test]
    fn config_dir_wins_over_instruction_file() {
        let root = root_with(&[".claude", "AGENTS.md"]);
        assert_eq!(
            detect_agents(&root),
            vec!["claude-code"],
            "配置目录存在时不叠加指令文件判定"
        );
    }

    #[test]
    fn instruction_file_fallback_when_no_config_dir() {
        let root = root_with(&["CLAUDE.md"]);
        assert_eq!(detect_agents(&root), vec!["claude-code"]);
    }

    #[test]
    fn agents_md_maps_to_open_standard() {
        let root = root_with(&["AGENTS.md"]);
        assert_eq!(detect_agents(&root), vec!["agents"]);
    }

    #[test]
    fn no_marker_falls_back_to_open_standard() {
        let root = root_with(&["src"]);
        assert_eq!(detect_agents(&root), vec!["agents"]);
    }
}
