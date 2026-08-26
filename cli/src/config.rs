use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_subnet")]
    pub subnet: String,
}

fn default_subnet() -> String {
    "10.10.20.0/24".to_string()
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            subnet: default_subnet(),
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
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

pub fn prompt_and_save_password() -> Result<()> {
    let mut config = load_config()?;
    eprintln!();
    eprintln!("Shelly device authentication");
    eprintln!("The password is stored locally and sent only to your devices.");
    if config.auth.password.is_some() {
        eprintln!("Press Enter to keep the saved password.");
    }
    eprint!("Password: ");
    io::stderr().flush()?;
    let password = if io::stdin().is_terminal() {
        rpassword::read_password()?
    } else {
        let mut value = String::new();
        io::stdin().read_line(&mut value)?;
        value.trim_end_matches(['\r', '\n']).to_string()
    };
    if !password.is_empty() {
        config.auth.password = Some(password);
    }
    if config.auth.password.is_none() {
        anyhow::bail!("a password is required");
    }
    save_config(&config)?;
    eprintln!("Password saved securely. Returning to shelly watch…");
    Ok(())
}

fn save_config(config: &AppConfig) -> Result<()> {
    let path = config_path()?;
    save_config_to(config, &path)
}

fn save_config_to(config: &AppConfig, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = toml::to_string_pretty(config)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".into());
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.{}.{sequence}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&temporary)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_authentication_and_network_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let config = AppConfig {
            auth: AuthConfig {
                password: Some("secret".into()),
            },
            network: NetworkConfig {
                subnet: "192.0.2.0/24".into(),
            },
        };
        save_config_to(&config, &path).unwrap();
        let loaded: AppConfig = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(loaded.auth.password.as_deref(), Some("secret"));
        assert_eq!(loaded.network.subnet, "192.0.2.0/24");
    }

    #[cfg(unix)]
    #[test]
    fn saved_password_is_owner_readable_only() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let config = AppConfig {
            auth: AuthConfig {
                password: Some("secret".into()),
            },
            ..AppConfig::default()
        };

        save_config_to(&config, &path).unwrap();

        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
