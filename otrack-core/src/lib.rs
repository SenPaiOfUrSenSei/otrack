use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Result, Context};
use directories::ProjectDirs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub general: GeneralConfig,
    pub blacklist: BlacklistConfig,
    pub work_apps: WorkAppsConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GeneralConfig {
    pub database_path: String,
    pub idle_timeout: u64, // seconds
<<<<<<< HEAD
=======
    pub grace_period: u64, // seconds
>>>>>>> 2710bb5 (Version 1.0.1: minor fixes and added the option to configure grace period)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlacklistConfig {
    pub apps: Vec<String>,
    pub block_during_focus: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkAppsConfig {
    pub apps: Vec<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let dirs = ProjectDirs::from("org", "omarchy", "otrack")
            .context("Failed to determine config directory")?;
        let config_path = dirs.config_dir().join("config.toml");

        if !config_path.exists() {
            let default_config = Config {
                general: GeneralConfig {
                    database_path: "~/.local/share/otrack/usage.db".into(),
                    idle_timeout: 300,
<<<<<<< HEAD
=======
                    grace_period: 30,
>>>>>>> 2710bb5 (Version 1.0.1: minor fixes and added the option to configure grace period)
                },
                blacklist: BlacklistConfig {
                    apps: vec!["discord".into(), "slack".into(), "spotify".into()],
                    block_during_focus: true,
                },
                work_apps: WorkAppsConfig {
                    apps: vec!["foot".into(), "neovim".into(), "zed".into()],
                },
            };
            std::fs::create_dir_all(dirs.config_dir())?;
            std::fs::write(&config_path, toml::to_string(&default_config)?)?;
            return Ok(default_config);
        }

        let content = std::fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn db_path(&self) -> PathBuf {
        let path = if self.general.database_path.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_default();
            self.general.database_path.replacen("~", &home, 1)
        } else {
            self.general.database_path.clone()
        };
        PathBuf::from(path)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonRequest {
    GetStatus,
    GetReport,
    StartFocus { duration_mins: u32 },
    StopFocus,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonResponse {
    Status {
        active_app: Option<String>,
        session_start: Option<chrono::DateTime<chrono::Local>>,
        is_focus_mode: bool,
        focus_remaining_secs: Option<u64>,
    },
    Report {
        top_apps: Vec<(String, u64)>, // (app_class, duration_secs)
        today_total: u64,
    },
    Ok,
    Error(String),
}

pub const SOCKET_PATH: &str = "/tmp/otrack.sock";
