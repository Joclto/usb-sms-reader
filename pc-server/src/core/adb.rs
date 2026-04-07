use std::process::Command;
use crate::config::AdbConfig;
use crate::error::{AppError, Result};

pub struct AdbManager {
    config: AdbConfig,
}

impl AdbManager {
    pub fn new(config: AdbConfig) -> Self {
        AdbManager { config }
    }

    pub async fn list_devices(&self) -> Result<Vec<String>> {
        let output = Command::new(&self.config.path)
            .arg("devices")
            .output()
            .map_err(|e| AppError::IoError(e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices: Vec<String> = stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == "device" {
                    Some(parts[0].to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(devices)
    }

    pub async fn setup_forward(&self, device_id: &str, local_port: u16) -> Result<()> {
        let status = Command::new(&self.config.path)
            .args(["-s", device_id, "forward", &format!("tcp:{}", local_port), "tcp:8080"])
            .status()
            .map_err(|e| AppError::IoError(e))?;

        if !status.success() {
            return Err(AppError::DeviceNotConnected);
        }

        Ok(())
    }

    pub async fn check_connection(&self, device_id: &str) -> Result<bool> {
        let devices = self.list_devices().await?;
        Ok(devices.iter().any(|d| d == device_id))
    }
}