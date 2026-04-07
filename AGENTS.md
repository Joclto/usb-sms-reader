# AGENTS.md

## Project Overview

USB SMS Reader - Read SMS from Android phone via USB and forward to PC.

**Architecture:**
- `pc-server/` - Rust + eframe (egui) GUI application
- `android-app/` - Kotlin Android app using Accessibility Service

## Build Commands

### PC Server (Rust)
```bash
cd pc-server
cargo build --release
```

Run: `./target/release/usb-sms-reader.exe` or `.\target\release\usb-sms-reader.exe` (Windows)

### Android App
Requires Android Studio. No gradlew wrapper exists.
1. Open `android-app/` in Android Studio
2. Build > Build APK(s) or Build > Rebuild Project

## Key Technical Details

### ADB Integration
- ADB binaries embedded in `pc-server/tools/`
- Config path: `config.yaml` → `adb.path: "./tools/adb"`
- Uses `adb reverse tcp:8080 tcp:8080` (NOT `adb forward`)
- Android app connects to `127.0.0.1:8080` via reversed port

### Connection Flow
1. PC starts TCP server on port 8080
2. ADB reverse forwards device port 8080 → PC port 8080
3. Android Accessibility Service connects to localhost:8080
4. JSON messages sent over TCP

### JSON Protocol
- `sim_cards` - Android sends SIM card list on connect
- `sms_list` - Response to `fetch_all_sms` command
- `new_sms` - Real-time SMS notification
- Commands: `fetch_all_sms`, `get_sim_cards`, `ping`

### Android Permissions Required
- `READ_SMS` - Read SMS messages
- `READ_PHONE_STATE`, `READ_PHONE_NUMBERS` - Get SIM info
- Accessibility Service - Monitor SMS notifications

### Logging
- Android: In-app log window with timestamps
- Connection state displayed in app UI
- Auto-reconnect every 3 seconds when disconnected

## Configuration

`pc-server/config/config.yaml`:
```yaml
adb:
  path: "./tools/adb"  # ADB binary path
server:
  listen_port: 8080    # TCP server port
```

## Common Issues

1. **"ADB not found"** - ADB path must point to embedded `tools/` directory
2. **"Disconnected" in PC app** - Android Accessibility Service not started or ADB reverse not active
3. **SIM shows "unknown number"** - Android 10+ privacy restriction, not fixable
4. **Chinese characters appear as squares** - Font encoding in JSON serialization, use `JSONObject` not string concatenation

## File Structure Notes

- `pc-server/src/app/ui.rs` - Main GUI (eframe)
- `pc-server/src/app/state.rs` - ADB commands, state management
- `android-app/.../service/SmsAccessibilityService.kt` - Core Android service
- `android-app/.../network/NetworkClient.kt` - TCP client with command listener
- `android-app/.../util/LogManager.kt` - Shared log state between service and activity