use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub model: Option<String>,
    pub android_version: Option<String>,
    pub connected: bool,
}

pub struct DeviceManager {
    devices: Arc<RwLock<HashMap<String, DeviceInfo>>>,
}

impl DeviceManager {
    pub fn new() -> Self {
        DeviceManager {
            devices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_device(&self, device: DeviceInfo) {
        let mut devices = self.devices.write().await;
        devices.insert(device.id.clone(), device);
    }

    pub async fn remove_device(&self, device_id: &str) {
        let mut devices = self.devices.write().await;
        devices.remove(device_id);
    }

    pub async fn get_device(&self, device_id: &str) -> Option<DeviceInfo> {
        let devices = self.devices.read().await;
        devices.get(device_id).cloned()
    }

    pub async fn list_devices(&self) -> Vec<DeviceInfo> {
        let devices = self.devices.read().await;
        devices.values().cloned().collect()
    }

    pub async fn set_connected(&self, device_id: &str, connected: bool) {
        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(device_id) {
            device.connected = connected;
        }
    }
}