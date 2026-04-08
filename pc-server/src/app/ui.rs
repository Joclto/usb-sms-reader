use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::process::Command;
use tokio::sync::broadcast;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use super::state::{AppState, ConnectionStatus, SmsMessage, check_adb_available, list_adb_devices, setup_adb_forward, remove_adb_forward, check_adb_forward_active, set_adb_path};
use crate::config::Settings;
use crate::forwarder::InfoPushClient;

pub struct SmsReaderApp {
    state: Arc<Mutex<AppState>>,
    runtime: tokio::runtime::Runtime,
    settings: Option<Settings>,
    selected_sms: Option<i64>,
    adb_check_timer: f64,
    auto_setup_adb: bool,
    command_sender: broadcast::Sender<String>,
}

impl SmsReaderApp {
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
            let mut s = state.lock().unwrap();
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
        }

        let (tx, _) = broadcast::channel::<String>(16);

        let app = SmsReaderApp {
            state,
            runtime,
            settings,
            selected_sms: None,
            adb_check_timer: 0.0,
            auto_setup_adb: true,
            command_sender: tx.clone(),
        };

        app.start_server(tx.subscribe());
        app
    }

    fn start_server(&self, _command_rx: broadcast::Receiver<String>) {
        let state = Arc::clone(&self.state);
        let settings = self.settings.clone();
        let port = self.state.lock().unwrap().server_port;
        let cmd_sender = self.command_sender.clone();

        {
            let mut s = self.state.lock().unwrap();
            s.server_running = true;
            s.add_log("INFO", format!("Server starting on port {}...", port));
        }

        self.runtime.spawn(async move {
            let listen_port = settings.as_ref().map(|s| s.server.listen_port).unwrap_or(8080);

            let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", listen_port)).await {
                Ok(l) => l,
                Err(e) => {
                    if let Ok(mut s) = state.lock() {
                        s.add_log("ERROR", format!("Bind failed: {}", e));
                        s.server_running = false;
                    }
                    return;
                }
            };

            if let Ok(mut s) = state.lock() {
                s.add_log("INFO", format!("Server listening on port {}", listen_port));
            }

            loop {
                let accept_result = listener.accept().await;
                match accept_result {
                    Ok((stream, addr)) => {
                        if let Ok(mut s) = state.lock() {
                            s.add_log("INFO", format!("Client connected: {}", addr));
                            s.connection_status = ConnectionStatus::Connected;
                            s.connected_device = Some(super::state::DeviceInfo {
                                serial: addr.to_string(),
                                model: Some("Android Device".into()),
                                android_version: None,
                            });
                        }

                        let state_clone = Arc::clone(&state);
                        let (rd, mut wr) = stream.into_split();
                        let reader = tokio::io::BufReader::new(rd);
                        let mut lines = reader.lines();
                        let mut cmd_rx = cmd_sender.subscribe();

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
                                }
                            }

                            if let Ok(mut s) = state_clone.lock() {
                                s.connection_status = ConnectionStatus::WaitingForDevice;
                                s.connected_device = None;
                                s.add_log("INFO", "Client disconnected".into());
                            }
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

    fn check_and_setup_adb(&mut self) {
        let port = self.state.lock().unwrap().server_port;
        let state = Arc::clone(&self.state);
        let settings = self.settings.clone();
        let auto_setup = self.auto_setup_adb;

        self.runtime.spawn_blocking(move || {
            let adb_ok = check_adb_available();
            let devices = if adb_ok { list_adb_devices() } else { Vec::new() };
            let forward_active = if adb_ok && !devices.is_empty() {
                check_adb_forward_active(port)
            } else {
                false
            };

            {
                let mut s = state.lock().unwrap();
                let prev_device = s.adb_status.device_connected;
                let prev_forward = s.adb_status.port_forwarded;

                s.adb_status.adb_available = adb_ok;
                s.adb_status.device_connected = !devices.is_empty();
                s.adb_status.port_forwarded = forward_active;
                s.adb_status.device_serial = devices.first().cloned();
                s.adb_status.last_check = Some(std::time::Instant::now());

                if !prev_device && s.adb_status.device_connected {
                    if let Some(ref serial) = &s.adb_status.device_serial {
                        let serial = serial.clone();
                        s.add_log("INFO", format!("Device connected: {}", serial));
                    }
                }
                if prev_device && !s.adb_status.device_connected {
                    s.add_log("WARN", "Device disconnected".into());
                }
                if !prev_forward && s.adb_status.port_forwarded {
                    s.add_log("INFO", format!("ADB reverse {} active", port));
                }
            }

            if auto_setup && adb_ok && !devices.is_empty() && !forward_active {
                {
                    let mut s = state.lock().unwrap();
                    s.connection_status = ConnectionStatus::AdbForwarding;
                    s.add_log("INFO", format!("Setting up ADB reverse for port {}...", port));
                }

                match setup_adb_forward(port) {
                    Ok(()) => {
                        let mut s = state.lock().unwrap();
                        s.adb_status.port_forwarded = true;
                        s.add_log("INFO", "ADB reverse set up successfully".into());
                        s.connection_status = ConnectionStatus::WaitingForDevice;
                    }
                    Err(e) => {
                        let mut s = state.lock().unwrap();
                        s.add_log("ERROR", format!("Failed to setup ADB reverse: {}", e));
                        s.connection_status = ConnectionStatus::Error(format!("ADB reverse failed: {}", e));
                    }
                }
            }

            {
                let mut s = state.lock().unwrap();
                if !s.adb_status.adb_available {
                    if !matches!(s.connection_status, ConnectionStatus::Connected) {
                        s.connection_status = ConnectionStatus::Error("ADB not available".into());
                    }
                } else if devices.is_empty() {
                    if !matches!(s.connection_status, ConnectionStatus::Connected) {
                        s.connection_status = ConnectionStatus::WaitingForDevice;
                    }
                }
            }
        });
    }

    fn reconnect_adb(&mut self) {
        let port = self.state.lock().unwrap().server_port;
        let adb_path = self.settings.as_ref().map(|s| s.adb.path.clone()).unwrap_or_else(|| "./tools/adb".to_string());

        {
            let mut s = self.state.lock().unwrap();
            s.add_log("INFO", "Reconnecting ADB...".into());
        }

        // Remove existing forward first
        let _ = remove_adb_forward(port);
        
        // Kill and restart ADB server
        {
            let mut cmd = Command::new(&adb_path);
            #[cfg(target_os = "windows")]
            cmd.creation_flags(0x08000000);
            let _ = cmd.args(["kill-server"]).status();
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        {
            let mut cmd = Command::new(&adb_path);
            #[cfg(target_os = "windows")]
            cmd.creation_flags(0x08000000);
            let _ = cmd.args(["start-server"]).status();
        }
        
        // Recheck
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.check_and_setup_adb();
    }

    fn send_command(&self, command: &str) {
        let _ = self.command_sender.send(command.to_string());
        if let Ok(mut s) = self.state.lock() {
            s.add_log("INFO", format!("Queuing command: {}", command));
        }
    }

    fn fetch_sms(&self, limit: i32) {
        {
            let s = self.state.lock().unwrap();
            if !matches!(s.connection_status, ConnectionStatus::Connected) {
                let status = format!("{:?}", s.connection_status);
                drop(s);
                self.state.lock().unwrap().add_log("ERROR", format!("Cannot send: not connected ({})", status));
                return;
            }
        }
        let cmd = if limit > 0 {
            format!(r#"{{"type":"fetch_all_sms","limit":{}}}"#, limit)
        } else {
            r#"{"type":"fetch_all_sms","limit":0}"#.to_string()
        };
        let label = if limit > 0 { format!("Fetch SMS (latest {})", limit) } else { "Fetch All SMS".to_string() };
        self.state.lock().unwrap().add_log("INFO", format!("User requested: {}", label));
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
                let (adb_status, device_status, forward_status, connection_status, device_serial) = {
                    let s = self.state.lock().unwrap();
                    (
                        s.adb_status.adb_available,
                        s.adb_status.device_connected,
                        s.adb_status.port_forwarded,
                        s.connection_status.clone(),
                        s.adb_status.device_serial.clone(),
                    )
                };

                // ADB Available indicator
                let (adb_text, adb_color) = if adb_status {
                    ("ADB: OK", egui::Color32::GREEN)
                } else {
                    ("ADB: Not Found", egui::Color32::RED)
                };
                ui.colored_label(adb_color, adb_text);

                ui.separator();

                // Device Status
                let (device_text, device_color) = if device_status {
                    ("Device: Connected", egui::Color32::GREEN)
                } else {
                    ("Device: Not Connected", egui::Color32::YELLOW)
                };
                ui.colored_label(device_color, device_text);

                // Device serial
                if let Some(serial) = device_serial {
                    ui.label(format!("({})", serial));
                }

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
                    let s = self.state.lock().unwrap();
                    s.sms_list.iter().map(|sms| {
                        let time_str = chrono::DateTime::from_timestamp_millis(sms.timestamp)
                            .map(|t| t.format("%m-%d %H:%M").to_string())
                            .unwrap_or_default();
                        (sms.id, sms.sender.clone(), sms.body.chars().take(50).collect(), time_str)
                    }).collect()
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
                let sms_list = self.state.lock().unwrap().sms_list.clone();
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
                    let logs: Vec<_> = self.state.lock().unwrap().logs.iter().rev().take(100).cloned().collect();
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