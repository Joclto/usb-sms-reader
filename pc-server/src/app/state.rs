use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

static ADB_PATH: OnceLock<String> = OnceLock::new();

pub fn set_adb_path(path: String) {
    let _ = ADB_PATH.set(path);
}

fn get_adb_path() -> String {
    if let Some(path) = ADB_PATH.get() {
        if std::path::Path::new(path).exists() {
            return path.clone();
        }
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    
    if let Some(ref dir) = exe_dir {
        for candidate in ["tools/adb.exe", "tools/adb", "adb.exe", "adb"] {
            let full = dir.join(candidate);
            if full.exists() {
                return full.to_string_lossy().to_string();
            }
        }
    }
    
    ADB_PATH.get().cloned().unwrap_or_else(|| "adb".to_string())
}

fn adb_command() -> Command {
    let mut cmd = Command::new(get_adb_path());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[derive(Debug, Clone, Copy)]
pub enum PendingCommand {
    FetchAllSms,
    FetchSms { limit: u32 },
    Ping,
}

#[derive(Debug, Clone)]
pub struct SimCard {
    pub slot_index: i32,
    pub phone_number: String,
    pub carrier_name: String,
    pub is_active: bool,
}

impl SimCard {
    pub fn display_name(&self) -> String {
        if !self.carrier_name.is_empty() && !self.phone_number.is_empty() {
            format!("{} ({})", self.phone_number, self.carrier_name)
        } else if !self.carrier_name.is_empty() {
            format!("SIM{} ({})", self.slot_index + 1, self.carrier_name)
        } else {
            self.phone_number.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    WaitingForDevice,
    AdbForwarding,
    Error(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::Disconnected
    }
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "Disconnected"),
            ConnectionStatus::Connecting => write!(f, "Connecting..."),
            ConnectionStatus::Connected => write!(f, "Connected"),
            ConnectionStatus::WaitingForDevice => write!(f, "Waiting for device"),
            ConnectionStatus::AdbForwarding => write!(f, "Setting up ADB..."),
            ConnectionStatus::Error(e) => write!(f, "Error: {}", e),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SmsMessage {
    pub id: i64,
    pub sender: String,
    pub body: String,
    pub timestamp: i64,
    pub category: String,
    pub read: bool,
    pub sim_slot: i32,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub serial: String,
    pub model: Option<String>,
    pub android_version: Option<String>,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        DeviceInfo {
            serial: String::new(),
            model: None,
            android_version: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Instant,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct AdbStatus {
    pub adb_available: bool,
    pub device_connected: bool,
    pub port_forwarded: bool,
    pub device_serial: Option<String>,
    pub last_check: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub connection_status: ConnectionStatus,
    pub adb_status: AdbStatus,
    pub connected_device: Option<DeviceInfo>,
    pub sms_list: Vec<SmsMessage>,
    pub logs: VecDeque<LogEntry>,
    pub server_running: bool,
    pub server_port: u16,
    pub infopush_enabled: bool,
    pub last_activity: Instant,
    pub pending_command: Option<PendingCommand>,
    pub sim_cards: Vec<SimCard>,
    pub selected_sim: Option<usize>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            connection_status: ConnectionStatus::Disconnected,
            adb_status: AdbStatus::default(),
            connected_device: None,
            sms_list: Vec::new(),
            logs: VecDeque::with_capacity(1000),
            server_running: false,
            server_port: 8080,
            infopush_enabled: false,
            last_activity: Instant::now(),
            pending_command: None,
            sim_cards: Vec::new(),
            selected_sim: None,
        }
    }
}

impl AppState {
    pub fn add_log(&mut self, level: &str, message: String) {
        if self.logs.len() >= 1000 {
            self.logs.pop_front();
        }
        self.logs.push_back(LogEntry {
            timestamp: Instant::now(),
            level: level.to_string(),
            message,
        });
    }

    pub fn add_sms(&mut self, sms: SmsMessage) {
        if self.sms_list.iter().any(|s| s.id == sms.id) {
            return;
        }
        self.sms_list.insert(0, sms);
        if self.sms_list.len() > 500 {
            self.sms_list.pop();
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

// ADB helper functions
pub fn check_adb_available() -> bool {
    adb_command()
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn list_adb_devices() -> Vec<String> {
    let output = adb_command()
        .args(["devices", "-l"])
        .output();
    
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
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
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

pub fn setup_adb_forward(port: u16) -> Result<(), String> {
    let status = adb_command()
        .args(["reverse", &format!("tcp:{}", port), &format!("tcp:{}", port)])
        .status()
        .map_err(|e| format!("ADB reverse failed: {}", e))?;
    
    if status.success() {
        Ok(())
    } else {
        Err("ADB reverse command failed".to_string())
    }
}

pub fn remove_adb_forward(port: u16) -> Result<(), String> {
    let status = adb_command()
        .args(["reverse", "--remove", &format!("tcp:{}", port)])
        .status()
        .map_err(|e| format!("ADB remove reverse failed: {}", e))?;
    
    if status.success() {
        Ok(())
    } else {
        Err("ADB remove reverse command failed".to_string())
    }
}

pub fn check_adb_forward_active(port: u16) -> bool {
    let output = adb_command()
        .args(["reverse", "--list"])
        .output();
    
    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.contains(&format!("tcp:{}", port))
        }
        Err(_) => false,
    }
}