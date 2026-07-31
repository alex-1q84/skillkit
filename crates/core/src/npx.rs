//! npx skills 调用封装：在 cwd=~/.skillkit/ 跑 npx skills（project scope），
//! skill 落 ~/.skillkit/.agents/skills/，skills-lock.json 落 ~/.skillkit/。
//! skillkit 读 skills-lock.json 的 computedHash 做版本锁。下载委托 npx skills，角色到此完结。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use serde::Deserialize;
use std::process::Command;

/// project scope 装 universal agent，其目录是 .agents/skills/（npx skills 标准布局）。
const AGENT: &str = "universal";

/// 在 cwd=~/.skillkit/ 起 npx skills，关颜色便于解析输出。
fn npx(paths: &Paths) -> Command {
    let mut cmd = Command::new("npx");
    cmd.arg("skills@latest")
        .current_dir(paths.skillkit_dir())
        .env("NO_COLOR", "1");
    cmd
}

/// 安装：npx skills add <package>@<skill> -a universal --copy -y。
pub fn add(paths: &Paths, package: &str, skill: &str) -> Result<()> {
    let out = npx(paths)
        .args(["add", package, "-s", skill, "-a", AGENT, "--copy", "-y"])
        .output()
        .map_err(|e| SkillkitError::Tool {
            message: format!("启动 npx skills 失败：{e}（确认 Node 已安装）"),
        })?;
    if !out.status.success() {
        return Err(SkillkitError::Tool {
            message: format!(
                "npx skills add {package} -s {skill} 失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(())
}

/// registry 搜索候选（skills.sh 源 install 用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Candidate {
    /// owner/repo@skill，可直接传 npx skills add。
    pub spec: String,
    /// skills.sh 详情页（展示用）。
    pub url: Option<String>,
}

/// find：npx skills find <query>，解析输出得候选列表。
pub fn find(paths: &Paths, query: &str) -> Result<Vec<Candidate>> {
    let out = npx(paths)
        .args(["find", query])
        .output()
        .map_err(|e| SkillkitError::Tool {
            message: format!("启动 npx skills find 失败：{e}"),
        })?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(parse_find(&stdout))
}

/// 解析 find 输出：每行提取 owner/repo@skill + skills.sh URL。
fn parse_find(text: &str) -> Vec<Candidate> {
    text.lines()
        .filter_map(|line| {
            let clean = strip_ansi(line);
            let spec = clean.split_whitespace().find(|t| {
                t.contains('@')
                    && t.contains('/')
                    && !t.starts_with("http")
                    && !t.contains('<')
                    && !t.contains('>')
            })?;
            let url = clean
                .split_whitespace()
                .find(|t| t.starts_with("https://skills.sh/"))
                .map(str::to_string);
            Some(Candidate {
                spec: spec.to_string(),
                url,
            })
        })
        .collect()
}

/// 剥 ANSI 颜色码（NO_COLOR 不一定被 npx skills 遵守，兜底手动剥）。
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for cc in chars.by_ref() {
                if cc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// skills-lock.json 结构（只取 computedHash）。
#[derive(Deserialize)]
struct LockFile {
    skills: std::collections::BTreeMap<String, LockSkill>,
}

#[derive(Deserialize)]
struct LockSkill {
    #[serde(rename = "computedHash")]
    computed_hash: String,
}

/// 读 ~/.skillkit/skills-lock.json 拿指定 skill 的 computedHash。
pub fn read_computed_hash(paths: &Paths, skill: &str) -> Result<String> {
    let path = paths.skillkit_dir().join("skills-lock.json");
    let content = std::fs::read_to_string(&path).map_err(|_| SkillkitError::Tool {
        message: format!(
            "skills-lock.json 缺失（{}），install 可能未成功",
            path.display()
        ),
    })?;
    let lock: LockFile = serde_json::from_str(&content)?;
    lock.skills
        .get(skill)
        .map(|s| s.computed_hash.clone())
        .ok_or_else(|| SkillkitError::Tool {
            message: format!("skills-lock.json 找不到 skill：{skill}"),
        })
}

/// 卸载同步：npx skills remove <skill>。失败不阻塞——lock 只是缓存，registry 是事实源。
pub fn remove(paths: &Paths, skill: &str) -> Result<()> {
    let _ = npx(paths).args(["remove", skill, "-y"]).output();
    Ok(())
}

/// 升级：npx skills update <skill>。
pub fn update(paths: &Paths, skill: &str) -> Result<()> {
    let out = npx(paths)
        .args(["update", skill, "-y"])
        .output()
        .map_err(|e| SkillkitError::Tool {
            message: format!("启动 npx skills update 失败：{e}"),
        })?;
    if !out.status.success() {
        return Err(SkillkitError::Tool {
            message: format!(
                "npx skills update {skill} 失败：{}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_find_extracts_specs_and_urls() {
        let out = "Install with npx skills add <owner/repo@skill>\n\n\
                   anthropics/skills@pdf  169.3K installs  \u{2192}  https://skills.sh/anthropics/skills/pdf\n\
                   openai/skills@pdf  10.9K installs\n";
        let cs = parse_find(out);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].spec, "anthropics/skills@pdf");
        assert_eq!(
            cs[0].url.as_deref(),
            Some("https://skills.sh/anthropics/skills/pdf")
        );
        assert_eq!(cs[1].spec, "openai/skills@pdf");
        assert!(cs[1].url.is_none());
    }

    #[test]
    fn parse_find_strips_ansi_codes() {
        let out = "\x1b[38;5;145manthropics/skills@pdf\x1b[0m installs\n";
        let cs = parse_find(out);
        assert_eq!(cs[0].spec, "anthropics/skills@pdf");
    }
}
