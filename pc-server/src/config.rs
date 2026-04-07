use config::{Config, ConfigError, File};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub adb: AdbConfig,
    pub infopush: InfoPushConfig,
    pub storage: StorageConfig,
    pub classifier: ClassifierConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub listen_host: String,
    pub listen_port: u16,
    pub workers: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdbConfig {
    pub path: String,
    pub device_timeout: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InfoPushConfig {
    pub enabled: bool,
    pub server_url: String,
    pub push_token: String,
    pub timeout: u64,
    pub retry_count: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    pub storage_type: String,
    pub path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClassifierConfig {
    pub enabled: bool,
    pub rules: HashMap<String, CategoryRule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CategoryRule {
    pub keywords: Vec<String>,
    pub patterns: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub file: String,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name("config/config"))
            .build()?;

        let mut settings: Settings = config.try_deserialize()?;

        if let Ok(token) = std::env::var("INFOPUSH_PUSH_TOKEN") {
            settings.infopush.push_token = token;
        }

        Ok(settings)
    }
}