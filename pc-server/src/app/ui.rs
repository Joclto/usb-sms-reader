use eframe::egui;
use std::io::ErrorKind;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use super::state::{AppState, ConnectionStatus, SmsMessage, ANDROID_APP_PORT, check_adb_available, list_adb_devices, setup_adb_forward, remove_adb_forward, check_adb_forward_active, set_adb_path, adb_command_from_path};
use crate::config::Settings;
use crate::forwarder::InfoPushClient;

async fn bind_server_listener(
    listen_host: &str,
    preferred_port: u16,
    state: &Arc<Mutex<AppState>>,
) -> Result<(TcpListener, u16), std::io::Error> {
    match TcpListener::bind((listen_host, preferred_port)).await {
        Ok(listener) => Ok((listener, preferred_port)),
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            if let Ok(mut s) = state.lock() {
                s.add_log(
                    "WARN",
                    format!(
                        "Port {} is already in use. Switching PC listener to an available port and keeping Android side on {} via ADB reverse.",
                        preferred_port, ANDROID_APP_PORT
                    ),
                );
            }

            let listener = TcpListener::bind((listen_host, 0)).await?;
            let actual_port = listener.local_addr()?.port();
            Ok((listener, actual_port))
        }
        Err(e) => Err(e),
    }
}

fn with_shared_state_mut<R>(
    state: &Arc<Mutex<AppState>>,
    f: impl FnOnce(&mut AppState) -> R,
) -> Option<R> {
    match state.lock() {
        Ok(mut guard) => Some(f(&mut guard)),
        Err(_) => None,
    }
}

fn with_shared_state<R>(state: &Arc<Mutex<AppState>>, f: impl FnOnce(&AppState) -> R) -> Option<R> {
    match state.lock() {
        Ok(guard) => Some(f(&guard)),
        Err(_) => None,
    }
}

pub struct SmsReaderApp {
    state: Arc<Mutex<AppState>>,
    runtime: tokio::runtime::Runtime,
    settings: Option<Settings>,
    selected_sms: Option<i64>,
    adb_check_timer: f64,
    auto_setup_adb: bool,
    command_sender: broadcast::Sender<String>,
    device_switch_sender: broadcast::Sender<Option<String>>,
}

impl SmsReaderApp {
    fn with_state_mut<R>(&self, f: impl FnOnce(&mut AppState) -> R) -> Option<R> {
        with_shared_state_mut(&self.state, f)
    }

    fn with_state<R>(&self, f: impl FnOnce(&AppState) -> R) -> Option<R> {
        with_shared_state(&self.state, f)
    }

    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let state = Arc::new(Mutex::new(AppState::default()));
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let settings = Settings::new().ok();

        // Set ADB path from config
        if let Some(ref settings) = settings {
            set_adb_path(settings.adb.path.clone());
        }

        // Check ADB availability on startup
        {
            let _ = with_shared_state_mut(&state, |s| {
                s.adb_status.adb_available = check_adb_available();
                if let Some(ref settings) = settings {
                    s.server_port = settings.server.listen_port;
                    s.infopush_enabled = settings.infopush.enabled;
                }
                if s.adb_status.adb_available {
                    s.add_log("INFO", "ADB found, checking for devices...".into());
                } else {
                    s.add_log("WARN", "ADB not found. Ensure Android SDK platform-tools is in PATH.".into());
                }
            });
        }

        let (tx, _) = broadcast::channel::<String>(16);
        let (device_switch_tx, _) = broadcast::channel::<Option<String>>(16);

        let app = SmsReaderApp {
            state,
            runtime,
            settings,
            selected_sms: None,
            adb_check_timer: 0.0,
            auto_setup_adb: true,
            command_sender: tx.clone(),
            device_switch_sender: device_switch_tx,
        };

        app.start_server();
        app
    }

fn start_server(&self) {
        let state = Arc::clone(&self.state);
        let settings = self.settings.clone();
        let port = self.with_state(|s| s.server_port).unwrap_or(8080);
        let cmd_sender = self.command_sender.clone();
        let switch_sender = self.device_switch_sender.clone();

        let _ = self.with_state_mut(|s| {
            s.server_running = true;
            s.add_log("INFO", format!("Server starting on port {}...", port));
        });

        self.runtime.spawn(async move {
            let listen_host = settings
                .as_ref()
                .map(|s| s.server.listen_host.clone())
                .unwrap_or_else(|| "0.0.0.0".to_string());
            let listen_port = settings.as_ref().map(|s| s.server.listen_port).unwrap_or(8080);
            let (listener, actual_port) = match bind_server_listener(&listen_host, listen_port, &state).await {
                Ok(result) => result,
                Err(e) => {
                    if let Ok(mut s) = state.lock() {
                        s.server_running = false;
                        s.connection_status = ConnectionStatus::Error(format!("Server bind failed: {}", e));
                        s.add_log(
                            "ERROR",
                            format!("Failed to start TCP server on {}:{}: {}", listen_host, listen_port, e),
                        );
                    }
                    return;
                }
            };

            if let Ok(mut s) = state.lock() {
                s.server_port = actual_port;
                if actual_port == listen_port {
                    s.add_log("INFO", format!("Server listening on {}:{}", listen_host, actual_port));
                } else {
                    s.add_log(
                        "INFO",
                        format!(
                            "Server listening on {}:{} (configured {} was busy; Android still uses {} through ADB reverse)",
                            listen_host, actual_port, listen_port, ANDROID_APP_PORT
                        ),
                    );
                }
            }

            loop {
                let accept_result = listener.accept().await;
                match accept_result {
                    Ok((stream, addr)) => {
                        let selected_serial_at_connect =
                            with_shared_state(&state, |s| s.adb_status.device_serial.clone()).flatten();
                        let selected_serial_key = selected_serial_at_connect.clone();
                        let _ = with_shared_state_mut(&state, |s| {
                            s.active_client_count += 1;
                            if let Some(ref serial) = selected_serial_key {
                                *s.active_connections_by_device.entry(serial.clone()).or_insert(0) += 1;
                            }
                            s.add_log("INFO", format!("Client connected: {}", addr));
                            s.connected_device = Some(super::state::DeviceInfo {
                                serial: selected_serial_key.clone().unwrap_or_else(|| addr.to_string()),
                                model: Some("Android Device".into()),
                                android_version: None,
                            });
                            SmsReaderApp::apply_connection_status(s);
                        });

                        let state_clone = Arc::clone(&state);
                        let (rd, mut wr) = stream.into_split();
                        let reader = tokio::io::BufReader::new(rd);
                        let mut lines = reader.lines();
                        let mut cmd_rx = cmd_sender.subscribe();
                        let mut switch_rx = switch_sender.subscribe();

                        // Spawn task for handling connection
                        tokio::spawn(async move {
                            loop {
                                tokio::select! {
                                    result = lines.next_line() => {
                                        match result {
                                            Ok(Some(line)) => {
                                                if let Ok(mut s) = state_clone.lock() {
                                                    s.add_log("DEBUG", format!("Received: {}", line));
                                                    s.last_activity = std::time::Instant::now();
                                                }

                                                if let Ok(sms_data) = serde_json::from_str::<serde_json::Value>(&line) {
                                                    if let Some(msg_type) = sms_data.get("type").and_then(|v| v.as_str()) {
                                                        if msg_type == "ping" {
                                                            if let Ok(mut s) = state_clone.lock() {
                                                                s.last_activity = std::time::Instant::now();
                                                            }
                                                        } else if msg_type == "handshake" {
                                                            let _ = wr.write_all(b"{\"type\":\"handshake_ack\"}\n").await;
                                                            let _ = wr.flush().await;
                                                            if let Ok(mut s) = state_clone.lock() {
                                                                s.add_log("INFO", "Handshake acknowledged".into());
                                                            }
                                                        } else if msg_type == "sms_list" {
                                                            if let Some(messages) = sms_data.get("messages").and_then(|v| v.as_array()) {
                                                                let mut new_list: Vec<SmsMessage> = Vec::with_capacity(messages.len());
                                                                for msg in messages {
                                                                    if let Some(sender) = msg.get("sender").and_then(|v| v.as_str()) {
                                                                        let body = msg.get("body").and_then(|v| v.as_str()).unwrap_or("");
                                                                        let timestamp = msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                                                        let sim_slot = msg.get("simSlot").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                                                                        let sms_id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                                                        new_list.push(SmsMessage {
                                                                            id: sms_id,
                                                                            sender: sender.into(),
                                                                            body: body.into(),
                                                                            timestamp,
                                                                            category: String::new(),
                                                                            read: false,
                                                                            sim_slot,
                                                                        });
                                                                    }
                                                                }
                                                                if let Ok(mut s) = state_clone.lock() {
                                                                    s.sms_list = new_list;
                                                                    s.add_log("INFO", format!("Received {} SMS messages", messages.len()));
                                                                }
                                                            }
                                                        } else if msg_type == "new_sms" {
                                                            if let Some(sms) = sms_data.get("sms") {
                                                                if let Some(sender) = sms.get("sender").and_then(|v| v.as_str()) {
                                                                    let body = sms.get("body").and_then(|v| v.as_str()).unwrap_or("");
                                                                    let timestamp = sms.get("timestamp").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                                                    let sim_slot = sms.get("simSlot").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                                                                    let sms_id = sms.get("id").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                                                    if let Ok(mut s) = state_clone.lock() {
                                                                        s.add_sms(SmsMessage {
                                                                            id: sms_id,
                                                                            sender: sender.into(),
                                                                            body: body.into(),
                                                                            timestamp,
                                                                            category: String::new(),
                                                                            read: false,
                                                                            sim_slot,
                                                                        });
                                                                        s.add_log("INFO", format!("SMS from {}: {}", sender, body.chars().take(30).collect::<String>()));
                                                                    }
                                                                }
                                                            }
                                                        } else if msg_type == "sim_cards" {
                                                            if let Some(cards) = sms_data.get("cards").and_then(|v| v.as_array()) {
                                                                let sim_cards: Vec<super::state::SimCard> = cards.iter().filter_map(|c| {
                                                                    Some(super::state::SimCard {
                                                                        slot_index: c.get("slotIndex")?.as_i64()? as i32,
                                                                        phone_number: c.get("phoneNumber")?.as_str()?.to_string(),
                                                                        carrier_name: c.get("carrierName")?.as_str()?.to_string(),
                                                                        is_active: c.get("isActive")?.as_bool().unwrap_or(true),
                                                                    })
                                                                }).collect();
                                                                let sim_count = sim_cards.len();
                                                                if let Ok(mut s) = state_clone.lock() {
                                                                    s.sim_cards = sim_cards;
                                                                    if s.selected_sim.is_none() && !s.sim_cards.is_empty() {
                                                                        s.selected_sim = Some(0);
                                                                    }
                                                                    s.add_log("INFO", format!("Received {} SIM cards", sim_count));
                                                                }
                                                            }
                                                        }
                                                    } else if let Some(sender) = sms_data.get("sender").and_then(|v| v.as_str()) {
                                                        let body = sms_data.get("body").and_then(|v| v.as_str()).unwrap_or("");
                                                        let timestamp = sms_data.get("timestamp").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                                        let sim_slot = sms_data.get("simSlot").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                                                        let sms_id = sms_data.get("id").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                                        if let Ok(mut s) = state_clone.lock() {
                                                            s.add_sms(SmsMessage {
                                                                id: sms_id,
                                                                sender: sender.into(),
                                                                body: body.into(),
                                                                timestamp,
                                                                category: String::new(),
                                                                read: false,
                                                                sim_slot,
                                                            });
                                                            s.add_log("INFO", format!("SMS from {}: {}", sender, body.chars().take(30).collect::<String>()));
                                                        }
                                                    }
                                                }
                                            }
                                            Ok(None) | Err(_) => break,
                                        }
                                    }
                                    result = cmd_rx.recv() => {
                                        match result {
                                            Ok(cmd) => {
                                                let cmd_str = cmd.clone();
                                                let data = cmd + "\n";
                                                if let Err(e) = wr.write_all(data.as_bytes()).await {
                                                    if let Ok(mut s) = state_clone.lock() {
                                                        s.add_log("ERROR", format!("Failed to send command: {}", e));
                                                    }
                                                } else {
                                                    let _ = wr.flush().await;
                                                    if let Ok(mut s) = state_clone.lock() {
                                                        s.add_log("INFO", format!("Command sent: {}", cmd_str));
                                                    }
                                                }
                                            }
                                            Err(_) => {}
                                        }
                                    }
                                    result = switch_rx.recv() => {
                                        match result {
                                            Ok(new_selected_serial) => {
                                                if new_selected_serial != selected_serial_at_connect {
                                                    let _ = with_shared_state_mut(&state_clone, |s| {
                                                        s.add_log(
                                                            "INFO",
                                                            "Selected device changed, closing stale client connection".into(),
                                                        );
                                                    });
                                                    break;
                                                }
                                            }
                                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                                            Err(broadcast::error::RecvError::Closed) => break,
                                        }
                                    }
                                }
                            }

                            let _ = with_shared_state_mut(&state_clone, |s| {
                                s.active_client_count = s.active_client_count.saturating_sub(1);
                                if let Some(ref serial) = selected_serial_key {
                                    if let Some(count) = s.active_connections_by_device.get_mut(serial) {
                                        *count = count.saturating_sub(1);
                                        if *count == 0 {
                                            s.active_connections_by_device.remove(serial);
                                        }
                                    }
                                }
                                let active_connections = s.active_client_count;
                                SmsReaderApp::apply_connection_status(s);
                                s.add_log(
                                    "INFO",
                                    format!(
                                        "Client disconnected (active connections: {})",
                                        active_connections
                                    ),
                                );
                            });
                        });
                    }
                    Err(e) => {
                        if let Ok(mut s) = state.lock() {
                            s.add_log("ERROR", format!("Accept error: {}", e));
                        }
                    }
                }
            }
        });
    }

    fn apply_connection_status(state: &mut AppState) {
        let selected_serial = state.adb_status.device_serial.clone();
        let selected_connected = selected_serial
            .as_ref()
            .and_then(|s| state.active_connections_by_device.get(s))
            .copied()
            .unwrap_or(0)
            > 0;

        if selected_connected {
            state.connection_status = ConnectionStatus::Connected;
            if state.connected_device.is_none() {
                state.connected_device = selected_serial.map(|serial| super::state::DeviceInfo {
                    serial,
                    model: Some("Android Device".into()),
                    android_version: None,
                });
            }
            return;
        }

        state.connected_device = None;
        if !state.adb_status.adb_available {
            state.connection_status = ConnectionStatus::Error("ADB not available".into());
        } else if state.adb_status.devices.is_empty() || state.adb_status.device_serial.is_none() {
            state.connection_status = ConnectionStatus::WaitingForDevice;
        }
    }

    fn check_and_setup_adb(&mut self) {
        let host_port = self.with_state(|s| s.server_port).unwrap_or(8080);
        let state = Arc::clone(&self.state);
        let auto_setup = self.auto_setup_adb;
        let switch_sender = self.device_switch_sender.clone();

        self.runtime.spawn_blocking(move || {
            let adb_ok = check_adb_available();
            let devices = if adb_ok { list_adb_devices() } else { Vec::new() };

            let (effective_serial, prev_serial, need_setup) = with_shared_state_mut(&state, |s| {
                let prev_serial = s.adb_status.device_serial.clone();
                let prev_forwarded = s.adb_status.port_forwarded;

                s.adb_status.devices = devices;
                s.adb_status.adb_available = adb_ok;
                s.adb_status.last_check = Some(std::time::Instant::now());

                if s.adb_status.devices.len() == 1 {
                    s.adb_status.device_serial = Some(s.adb_status.devices[0].serial.clone());
                } else if let Some(ref sel) = s.adb_status.device_serial {
                    if !s.adb_status.devices.iter().any(|d| d.serial == *sel) {
                        s.adb_status.device_serial = None;
                        s.adb_status.port_forwarded = false;
                        s.add_log("WARN", "Selected device disconnected".into());
                    }
                }

                s.adb_status.device_connected = s.adb_status.device_serial.is_some();
                let effective_serial = s.adb_status.device_serial.clone();

                if prev_serial.is_none() && effective_serial.is_some() {
                    let serial_log = effective_serial.clone().unwrap_or_default();
                    s.add_log("INFO", format!("Device connected: {}", serial_log));
                }
                if prev_serial.as_ref() != effective_serial.as_ref() {
                    if let Some(ref cur) = effective_serial {
                        let cur_name = s
                            .adb_status
                            .devices
                            .iter()
                            .find(|d| d.serial == *cur)
                            .map(|d| d.display_name())
                            .unwrap_or_else(|| cur.clone());
                        s.add_log("INFO", format!("Device switched to: {}", cur_name));
                    }
                }

                let need_setup = !prev_forwarded && effective_serial.is_some();
                (effective_serial, prev_serial, need_setup)
            })
            .unwrap_or((None, None, false));

            if prev_serial.as_ref() != effective_serial.as_ref() {
                let _ = switch_sender.send(effective_serial.clone());
            }

            // Handle device switch: remove old device's forward
            if let Some(ref prev) = prev_serial {
                if let Some(ref cur) = effective_serial {
                    if prev != cur {
                        let _ = remove_adb_forward(prev, ANDROID_APP_PORT);
                    }
                }
            }

            // Only attempt forward setup when needed
            if need_setup && auto_setup && adb_ok {
                if let Some(ref serial) = effective_serial {
                    // First check if forward is already active (e.g. from previous session)
                    let forward_active = check_adb_forward_active(serial, ANDROID_APP_PORT, host_port);

                    if forward_active {
                        let _ = with_shared_state_mut(&state, |s| {
                            s.adb_status.port_forwarded = true;
                            s.add_log(
                                "INFO",
                                format!("ADB reverse active: Android {} -> PC {}", ANDROID_APP_PORT, host_port),
                            );
                            SmsReaderApp::apply_connection_status(s);
                        });
                    } else {
                        let _ = with_shared_state_mut(&state, |s| {
                            s.connection_status = ConnectionStatus::AdbForwarding;
                            s.add_log(
                                "INFO",
                                format!(
                                    "Setting up ADB reverse for device {}: Android {} -> PC {}...",
                                    serial, ANDROID_APP_PORT, host_port
                                ),
                            );
                        });

                        match setup_adb_forward(serial, ANDROID_APP_PORT, host_port) {
                            Ok(()) => {
                                let _ = with_shared_state_mut(&state, |s| {
                                    s.adb_status.port_forwarded = true;
                                    s.add_log(
                                        "INFO",
                                        format!("ADB reverse set up successfully: Android {} -> PC {}", ANDROID_APP_PORT, host_port),
                                    );
                                    SmsReaderApp::apply_connection_status(s);
                                });
                            }
                            Err(e) => {
                                let _ = with_shared_state_mut(&state, |s| {
                                    s.adb_status.port_forwarded = false;
                                    s.add_log("ERROR", format!("Failed to setup ADB reverse: {}", e));
                                    s.connection_status = ConnectionStatus::Error(format!("ADB reverse failed: {}", e));
                                });
                            }
                        }
                    }
                }
            }

            let _ = with_shared_state_mut(&state, |s| {
                SmsReaderApp::apply_connection_status(s);
            });
        });
    }

    fn reconnect_adb(&mut self) {
        let host_port = self.with_state(|s| s.server_port).unwrap_or(8080);
        let serial = self.with_state(|s| s.adb_status.device_serial.clone()).flatten();
        let switch_sender = self.device_switch_sender.clone();
        let _ = self.with_state_mut(|s| {
            s.add_log("INFO", "Reconnecting ADB...".into());
        });

        let state = Arc::clone(&self.state);
        self.runtime.spawn_blocking(move || {
            if let Some(ref s) = serial {
                let _ = remove_adb_forward(s, ANDROID_APP_PORT);
            }

            {
                let mut cmd = adb_command_from_path();
                let _ = cmd.args(["kill-server"]).status();
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            {
                let mut cmd = adb_command_from_path();
                let _ = cmd.args(["start-server"]).status();
            }
            std::thread::sleep(std::time::Duration::from_millis(500));

            let adb_ok = check_adb_available();
            let devices = if adb_ok { list_adb_devices() } else { Vec::new() };
            let selected_serial = with_shared_state(&state, |s| s.adb_status.device_serial.clone()).flatten();
            let effective_serial = if devices.len() == 1 {
                Some(devices[0].serial.clone())
            } else if let Some(ref sel) = selected_serial {
                if devices.iter().any(|d| d.serial == *sel) {
                    Some(sel.clone())
                } else {
                    None
                }
            } else {
                None
            };

            let _ = with_shared_state_mut(&state, |s| {
                s.adb_status.adb_available = adb_ok;
                s.adb_status.devices = devices;
                s.adb_status.device_connected = effective_serial.is_some();
                s.adb_status.device_serial = effective_serial.clone();
                s.adb_status.last_check = Some(std::time::Instant::now());
            });
            let _ = switch_sender.send(effective_serial.clone());

            if let Some(ref serial) = effective_serial {
                let _ = with_shared_state_mut(&state, |s| {
                    s.connection_status = ConnectionStatus::AdbForwarding;
                    s.add_log(
                        "INFO",
                        format!(
                            "Setting up ADB reverse for device {}: Android {} -> PC {}...",
                            serial, ANDROID_APP_PORT, host_port
                        ),
                    );
                });
                match setup_adb_forward(serial, ANDROID_APP_PORT, host_port) {
                    Ok(()) => {
                        let _ = with_shared_state_mut(&state, |s| {
                            s.adb_status.port_forwarded = true;
                            s.add_log(
                                "INFO",
                                format!("ADB reverse set up successfully: Android {} -> PC {}", ANDROID_APP_PORT, host_port),
                            );
                            SmsReaderApp::apply_connection_status(s);
                        });
                    }
                    Err(e) => {
                        let _ = with_shared_state_mut(&state, |s| {
                            s.add_log("ERROR", format!("Failed to setup ADB reverse: {}", e));
                            s.connection_status = ConnectionStatus::Error(format!("ADB reverse failed: {}", e));
                        });
                    }
                }
            } else {
                let _ = with_shared_state_mut(&state, |s| {
                    SmsReaderApp::apply_connection_status(s);
                });
            }
        });
    }

    fn send_command(&self, command: &str) {
        let _ = self.command_sender.send(command.to_string());
        if let Ok(mut s) = self.state.lock() {
            s.add_log("INFO", format!("Queuing command: {}", command));
        }
    }

    fn fetch_sms(&self, limit: i32) {
        if let Some(status) = self.with_state(|s| s.connection_status.clone()) {
            if !matches!(status, ConnectionStatus::Connected) {
                let _ = self.with_state_mut(|s| {
                    s.add_log("ERROR", format!("Cannot send: not connected ({:?})", status));
                });
                return;
            }
        } else {
            return;
        }
        let cmd = if limit > 0 {
            format!(r#"{{"type":"fetch_all_sms","limit":{}}}"#, limit)
        } else {
            r#"{"type":"fetch_all_sms","limit":0}"#.to_string()
        };
        let label = if limit > 0 { format!("Fetch SMS (latest {})", limit) } else { "Fetch All SMS".to_string() };
        let _ = self.with_state_mut(|s| {
            s.add_log("INFO", format!("User requested: {}", label));
        });
        self.send_command(&cmd);
    }
}

impl eframe::App for SmsReaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));

        self.adb_check_timer += 0.5;
        if self.adb_check_timer >= 5.0 {
            self.adb_check_timer = 0.0;
            self.check_and_setup_adb();
        }

        // Top panel with ADB status
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("USB SMS Reader");
                ui.separator();

                // Get status from state
                let (adb_status, forward_status, connection_status, devices, selected_serial) = self
                    .with_state(|s| {
                        (
                            s.adb_status.adb_available,
                            s.adb_status.port_forwarded,
                            s.connection_status.clone(),
                            s.adb_status.devices.clone(),
                            s.adb_status.device_serial.clone(),
                        )
                    })
                    .unwrap_or((false, false, ConnectionStatus::Disconnected, Vec::new(), None));

                // ADB Available indicator
                let (adb_text, adb_color) = if adb_status {
                    ("ADB: OK", egui::Color32::GREEN)
                } else {
                    ("ADB: Not Found", egui::Color32::RED)
                };
                ui.colored_label(adb_color, adb_text);

                ui.separator();

                // Device selection ComboBox
                let device_connected = selected_serial.is_some();
                let (device_text, device_color) = if devices.is_empty() {
                    ("No device", egui::Color32::YELLOW)
                } else if device_connected {
                    ("Device: Connected", egui::Color32::GREEN)
                } else {
                    ("Select device", egui::Color32::YELLOW)
                };
                ui.colored_label(device_color, device_text);

                let selected_text = if let Some(ref serial) = selected_serial {
                    devices.iter().find(|d| d.serial == *serial)
                        .map(|d| d.display_name())
                        .unwrap_or_else(|| serial.clone())
                } else {
                    "-- select --".to_string()
                };

                egui::ComboBox::from_id_salt("adb_device_select")
                    .selected_text(&selected_text)
                    .show_ui(ui, |ui| {
                        for device in &devices {
                            let label = device.display_name();
                            let is_selected = selected_serial.as_ref() == Some(&device.serial);
                            if ui.selectable_label(is_selected, &label).clicked() {
                                let changed = self
                                    .with_state(|s| s.adb_status.device_serial.as_ref() != Some(&device.serial))
                                    .unwrap_or(false);
                                if changed {
                                    let old_serial = self
                                        .with_state_mut(|s| {
                                            let old_serial = s.adb_status.device_serial.clone();
                                            s.adb_status.device_serial = Some(device.serial.clone());
                                            s.adb_status.port_forwarded = false;
                                            s.connection_status = ConnectionStatus::WaitingForDevice;
                                            s.connected_device = None;
                                            s.sms_list.clear();
                                            s.sim_cards.clear();
                                            s.selected_sim = None;
                                            s.add_log("INFO", format!("Selected device: {}", device.serial));
                                            SmsReaderApp::apply_connection_status(s);
                                            old_serial
                                        })
                                        .flatten();
                                    self.selected_sms = None;
                                    if let Some(ref old) = old_serial {
                                        let _ = remove_adb_forward(old, ANDROID_APP_PORT);
                                    }
                                    let _ = self.device_switch_sender.send(Some(device.serial.clone()));
                                    self.adb_check_timer = 5.0;
                                }
                            }
                        }
                    });

                ui.separator();

                // Port Forward Status
                let (forward_text, forward_color) = if forward_status {
                    ("Reverse: Active", egui::Color32::GREEN)
                } else {
                    ("Reverse: Inactive", egui::Color32::GRAY)
                };
                ui.colored_label(forward_color, forward_text);

                ui.separator();

                // Connection status
                let (status_text, status_color) = match &connection_status {
                    ConnectionStatus::Connected => ("App Connected", egui::Color32::GREEN),
                    ConnectionStatus::WaitingForDevice => ("Waiting for app...", egui::Color32::YELLOW),
                    ConnectionStatus::AdbForwarding => ("Setting up ADB...", egui::Color32::BLUE),
                    ConnectionStatus::Disconnected => ("Disconnected", egui::Color32::GRAY),
                    ConnectionStatus::Connecting => ("Connecting...", egui::Color32::YELLOW),
                    ConnectionStatus::Error(e) => (e.as_str(), egui::Color32::RED),
                };
                ui.colored_label(status_color, status_text);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_connected = matches!(connection_status, ConnectionStatus::Connected);
                    
                    // Fetch SMS dropdown menu
                    ui.menu_button("Fetch SMS", |ui| {
                        if ui.add_enabled(is_connected, egui::Button::new("Latest 100")).clicked() {
                            self.fetch_sms(100);
                            ui.close_menu();
                        }
                        if ui.add_enabled(is_connected, egui::Button::new("Latest 500")).clicked() {
                            self.fetch_sms(500);
                            ui.close_menu();
                        }
                        if ui.add_enabled(is_connected, egui::Button::new("All")).clicked() {
                            self.fetch_sms(0);
                            ui.close_menu();
                        }
                    });
                    
                    let reconnect_text = if forward_status {
                        "Restart ADB"
                    } else {
                        "Setup ADB"
                    };
                    
                    if ui.button(reconnect_text).clicked() {
                        self.reconnect_adb();
                    }

                    if ui.button("Refresh").clicked() {
                        self.check_and_setup_adb();
                    }
                });
            });
        });

        // SMS List panel
        egui::SidePanel::left("sms_list")
            .default_width(400.0)
            .show(ctx, |ui| {
                let sms_display: Vec<(i64, String, String, String)> = {
                    self.with_state(|s| {
                        s.sms_list
                            .iter()
                            .map(|sms| {
                                let time_str = chrono::DateTime::from_timestamp_millis(sms.timestamp)
                                    .map(|t| t.format("%m-%d %H:%M").to_string())
                                    .unwrap_or_default();
                                (sms.id, sms.sender.clone(), sms.body.chars().take(50).collect(), time_str)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
                };
                
                ui.heading(format!("SMS List ({})", sms_display.len()));

                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (id, sender, body, time) in sms_display.iter() {
                        let is_selected = self.selected_sms == Some(*id);

                        let response = ui.selectable_label(is_selected, format!(
                            "{}\n{}\n{}",
                            sender,
                            body,
                            time
                        ));

                        if response.clicked() {
                            self.selected_sms = Some(*id);
                        }
                    }
                });
            });

        // SMS detail panel
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(selected_id) = self.selected_sms {
                let sms_list = self.with_state(|s| s.sms_list.clone()).unwrap_or_default();
                if let Some(sms) = sms_list.iter().find(|s| s.id == selected_id) {
                    ui.heading(&sms.sender);
                    let time_str = chrono::DateTime::from_timestamp_millis(sms.timestamp)
                        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default();
                    ui.label(&time_str);
                    ui.separator();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(&sms.body);
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Copy").clicked() {
                            ui.output_mut(|o| o.copied_text = sms.body.clone());
                        }
                        if ui.button("Forward").clicked() {
                            if let Some(ref settings) = self.settings {
                                if settings.infopush.enabled {
                                    let client = InfoPushClient::new(settings.infopush.clone());
                                    let msg = crate::forwarder::PushMessage {
                                        title: format!("[{}]", sms.sender),
                                        content: sms.body.clone(),
                                        content_type: "text".into(),
                                        url: None,
                                    };
                                    self.runtime.spawn(async move {
                                        let _ = client.push(msg).await;
                                    });
                                }
                            }
                        }
                    });
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Select a SMS from the list");
                });
            }
        });

        // Log panel
        egui::TopBottomPanel::bottom("log_panel")
            .default_height(150.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Logs");
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let logs: Vec<_> = self
                        .with_state(|s| s.logs.iter().rev().take(100).cloned().collect())
                        .unwrap_or_default();
                    for log in logs {
                        let color = match log.level.as_str() {
                            "ERROR" => egui::Color32::RED,
                            "WARN" => egui::Color32::YELLOW,
                            "INFO" => egui::Color32::GREEN,
                            _ => egui::Color32::GRAY,
                        };
                        ui.horizontal(|ui| {
                            ui.colored_label(color, format!("[{}]", log.level));
                            ui.label(&log.message);
                        });
                    }
                });
            });
    }
}
