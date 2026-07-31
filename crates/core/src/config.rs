//! agent 能力声明（config.toml）。新增 agent 只改配置不改代码。
use crate::error::Result;
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub agents: Vec<Agent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    /// 是否支持 symlink 落地（Claude 支持，Cursor 不支持）。
    pub supports_symlink: bool,
    /// 是否直读 ~/.agents/skills/（Cursor/Codex 等直读，Claude 不直读）。
    pub reads_agents_dir: bool,
}

impl Default for Config {
    /// 默认只声明 Claude（需要 symlink 桥接）。
    fn default() -> Self {
        Self {
            agents: vec![Agent {
                name: "claude-code".to_string(),
                supports_symlink: true,
                reads_agents_dir: false,
            }],
        }
    }
}

impl Config {
    /// 读取 config.toml；文件不存在时返回默认配置（不报错，引导首次使用）。
    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let cfg: Config = toml::from_str(&content)?;
        Ok(cfg)
    }

    /// 原子写 config.toml（写临时文件 + rename）。
    pub fn save(&self, paths: &Paths) -> Result<()> {
        let _lock = crate::lock::FileLock::acquire(paths, "config")?;
        let path = paths.config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        crate::error::atomic_write(&path, &content)?;
        Ok(())
    }

    /// 按 name 查 agent 能力（apply 落地决定 symlink/copy）。
    pub fn find_agent(&self, name: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    #[test]
    fn missing_config_returns_default() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let cfg = Config::load(&paths).unwrap();
        assert_eq!(cfg.agents.len(), 1);
        assert_eq!(cfg.agents[0].name, "claude-code");
        assert!(cfg.agents[0].supports_symlink);
        assert!(!cfg.agents[0].reads_agents_dir);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let cfg = Config::default();
        cfg.save(&paths).unwrap();
        let loaded = Config::load(&paths).unwrap();
        assert_eq!(loaded.agents.len(), cfg.agents.len());
        assert!(paths.config_path().exists());
    }
}
