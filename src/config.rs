use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_subnet")]
    pub subnet: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_subnet() -> String {
    "10.10.20.0/24".to_string()
}

fn default_timeout() -> u64 {
    3000
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            subnet: default_subnet(),
            timeout_ms: default_timeout(),
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
        .join("shelly-cli");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.toml"))
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let data = std::fs::read_to_string(&path)?;
    let config: AppConfig = toml::from_str(&data)?;
    Ok(config)
}
