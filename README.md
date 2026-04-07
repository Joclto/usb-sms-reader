# USB SMS Reader

通过 USB 连接读取 Android 手机短信并在 PC 端实时显示的跨平台应用。

## 功能特性

- **实时短信读取** - 监听 Android 手机接收的短信通知
- **PC 端显示** - 在电脑上实时查看短信内容
- **SIM 卡信息** - 显示 SIM 卡运营商和号码信息
- **双向通信** - TCP 协议支持命令和数据传输
- **自动重连** - 断线后自动尝试重新连接
- **详细日志** - Android 端内置日志系统便于调试

## 系统架构

```
┌─────────────────┐         USB          ┌─────────────────┐
│   Android App   │◄──────────────────►│   PC Server     │
│ (Accessibility  │    ADB Reverse      │   (Rust GUI)    │
│    Service)     │    tcp:8080         │                 │
└─────────────────┘                     └─────────────────┘
```

**通信流程：**
1. PC 启动 TCP 服务器监听 8080 端口
2. ADB 反向转发：`adb reverse tcp:8080 tcp:8080`
3. Android Accessibility Service 连接 `127.0.0.1:8080`
4. JSON 消息通过 TCP 传输

## 项目结构

```
usb-sms-reader/
├── android-app/           # Android 应用
│   ├── app/src/main/java/com/smsreader/
│   │   ├── ui/           # 界面
│   │   ├── service/      # 无障碍服务
│   │   ├── network/      # 网络客户端
│   │   ├── model/        # 数据模型
│   │   └── util/         # 工具类
│   └── build.gradle
├── pc-server/            # PC 服务端
│   ├── src/
│   │   ├── app/          # 应用逻辑
│   │   ├── config.rs     # 配置管理
│   │   └── main.rs       # 入口
│   ├── tools/            # ADB 工具
│   ├── config/           # 配置文件
│   └── Cargo.toml
└── README.md
```

## 快速开始

### 环境要求

**PC 端：**
- Rust 1.70+
- Windows / Linux / macOS

**Android 端：**
- Android Studio
- Android SDK 24+
- Android 设备（开启 USB 调试）

### 安装步骤

#### 1. 克隆仓库

```bash
git clone https://github.com/your-username/usb-sms-reader.git
cd usb-sms-reader
```

#### 2. 构建 PC 服务端

```bash
cd pc-server
cargo build --release
```

运行：
```bash
# Windows
.\target\release\usb-sms-reader.exe

# Linux/macOS
./target/release/usb-sms-reader
```

#### 3. 构建 Android 应用

1. 在 Android Studio 中打开 `android-app/` 目录
2. Build > Build APK(s)
3. 安装到设备：`adb install app/build/outputs/apk/debug/app-debug.apk`

### 使用方法

#### PC 端

1. 连接 Android 设备到电脑（USB 调试已开启）
2. 运行 PC 应用
3. 应用会自动执行 `adb reverse` 建立连接

#### Android 端

1. 安装并打开应用
2. 授予必要权限：
   - 读取短信
   - 读取电话状态
   - 读取电话号码
3. 启用无障碍服务：
   - 点击应用内的"启用无障碍服务"按钮
   - 或手动在设置中启用 `SmsAccessibilityService`
4. 查看连接状态："PC连接: 已连接 ✓" 表示成功

## 配置说明

### PC 端配置 (`pc-server/config/config.yaml`)

```yaml
adb:
  path: "./tools/adb"  # ADB 二进制路径
server:
  listen_port: 8080    # TCP 服务器端口
```

### Android 权限

```xml
<!-- 读取短信 -->
<uses-permission android:name="android.permission.READ_SMS" />

<!-- 读取 SIM 卡信息 -->
<uses-permission android:name="android.permission.READ_PHONE_STATE" />
<uses-permission android:name="android.permission.READ_PHONE_NUMBERS" />
```

## 通信协议

### JSON 消息格式

**Android → PC (事件)**
```json
{
  "type": "new_sms",
  "data": {
    "sender": "10086",
    "body": "您的验证码是123456",
    "timestamp": "2025-01-08T10:30:00Z",
    "sim_slot": 0
  }
}
```

**Android → PC (SIM 卡信息)**
```json
{
  "type": "sim_cards",
  "data": [
    {
      "slot_index": 0,
      "carrier_name": "中国移动",
      "phone_number": "13800138000"
    }
  ]
}
```

**PC → Android (命令)**
```json
{
  "command": "fetch_all_sms"
}
```

## 常见问题

### Q: ADB not found

确保 `config.yaml` 中的 ADB 路径正确指向内置的 `tools/` 目录。

### Q: PC 显示"未连接"

检查：
- Android 无障碍服务是否已启用
- USB 调试是否开启
- ADB 是否识别设备（`adb devices`）

### Q: SIM 卡号码显示"未知"

Android 10+ 隐私限制，部分设备不返回真实号码。这是系统行为，无法修改。

### Q: 中文显示乱码

确保使用 `JSONObject` 进行 JSON 序列化，而非字符串拼接。

## 技术栈

**PC 端：**
- Rust
- eframe / egui (GUI)
- tokio (异步运行时)
- serde_json (JSON 序列化)

**Android 端：**
- Kotlin
- Accessibility Service
- Socket 网络通信

## 开发路线

- [ ] 短信分类（验证码、通知、广告）
- [ ] 本地数据库存储
- [ ] 短信搜索功能
- [ ] 多设备支持
- [ ] 消息推送（Webhook）

## 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支：`git checkout -b feature/amazing-feature`
3. 提交更改：`git commit -m 'Add amazing feature'`
4. 推送分支：`git push origin feature/amazing-feature`
5. 创建 Pull Request

## 许可证

MIT License - 详见 [LICENSE](LICENSE) 文件

## 致谢

- 感谢所有开源项目的贡献者
- 基于 Accessibility Service 实现短信监听方案