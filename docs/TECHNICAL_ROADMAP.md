# USB SMS Reader - 技术路线文档

## 1. 项目概述

### 1.1 项目目标
通过USB连接Android手机，实时读取手机短信并转发到InfoPush服务，无需root权限。

### 1.2 核心需求
- 实时监听并读取新短信
- 无需root权限
- USB数据线连接
- 支持多品牌Android手机
- 短信自动分类
- 通过InfoPush转发到移动端

### 1.3 技术方案选择

| 方案 | 可行性 | 说明 |
|------|--------|------|
| ~~ADB命令直接读取~~ | ❌ 低 | Android 4.4+ SMS权限受限 |
| **辅助APK + 无障碍服务** | ✅ 高 | 不需root，完整获取短信内容 |
| ~~通知监听~~ | ⚠️ 中 | 内容可能被截断或隐藏 |
| ~~第三方工具+OCR~~ | ⚠️ 中 | 准确性依赖OCR |

**最终方案：辅助APK（无障碍服务）+ PC服务端（Rust）**

---

## 2. 系统架构

### 2.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────────┐
│                           整体架构                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐                    ┌──────────────┐              │
│  │   Android    │     USB/ADB       │              │              │
│  │     手机      │◄──────────────────►│   PC服务端   │              │
│  │              │                    │   (Rust)    │              │
│  │  ┌────────┐  │   Socket/WebSocket │              │              │
│  │  │辅助APK │◄─┼───────────────────►│  ┌────────┐  │              │
│  │  │无障碍  │  │                    │  │短信处理 │  │              │
│  │  │服务   │  │                    │  │分类器  │  │              │
│  │  └────────┘  │                    │  └────────┘  │              │
│  └──────────────┘                    │      │       │              │
│                                      │      │       │              │
│                                      │      │ HTTP  │              │
│                                      │      ▼       │              │
│                                      │ ┌──────────┐  │              │
│                                      │ │InfoPush  │  │              │
│                                      │ │客户端    │  │              │
│                                      │ └──────────┘  │              │
│                                      └──────┬───────┘              │
│                                             │                      │
│                                             │ HTTP API             │
│                                             ▼                      │
│                                      ┌──────────────┐              │
│                                      │  InfoPush    │              │
│                                      │   Server     │              │
│                                      └──────┬───────┘              │
│                                             │ WebSocket            │
│                                             ▼                      │
│                                      ┌──────────────┐              │
│                                      │ InfoPush App │              │
│                                      │  (用户手机)   │              │
│                                      └──────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 数据流

```
┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
│ 短信到达 │───►│无障碍服务│───►│ 数据传输 │───►│ 分类+存储│───►│ InfoPush│
│ (手机)  │    │ (APK)   │    │ (Socket)│    │ (Rust)  │    │  转发   │
└─────────┘    └─────────┘    └─────────┘    └─────────┘    └─────────┘
     │              │              │              │              │
     │              │              │              │              │
     ▼              ▼              ▼              ▼              ▼
  SMS广播      提取短信内容    JSON序列化    规则匹配+SQLite   POST API
```

---

## 3. 技术选型

### 3.1 手机端（Android APK）

| 组件 | 技术选择 | 说明 |
|------|----------|------|
| 开发语言 | Kotlin | Android首选语言 |
| 无障碍服务 | AccessibilityService | 监听短信通知 |
| 网络通信 | OkHttp / Socket | 与PC通信 |
| 序列化 | Gson / kotlinx.serialization | JSON序列化 |

### 3.2 PC服务端（Rust）

| 组件 | Crate | 版本 | 说明 |
|------|-------|------|------|
| 异步运行时 | tokio | 1.x | 高性能异步 |
| 序列化 | serde + serde_json | 1.x | JSON处理 |
| HTTP客户端 | reqwest | 0.11+ | InfoPush API调用 |
| 数据库 | rusqlite | 0.30+ | 本地存储 |
| 配置管理 | config | 0.14 | YAML/TOML配置 |
| 正则表达式 | regex | 1.x | 短信分类 |
| 日志 | tracing + tracing-subscriber | 0.1 | 结构化日志 |
| 错误处理 | thiserror + anyhow | 1.x | 错误处理 |

### 3.3 转发服务（InfoPush）

使用已部署的InfoPush服务，接口说明：
- 服务端地址：`http://your-server:8000`
- 推送接口：`POST /push/{push_token}`
- 消息格式：`{ title, content, content_type }`

---

## 4. ABC模块详细设计

### 4.1 手机端APK设计

#### 4.1.1 无障碍服务配置

```xml
<!-- AndroidManifest.xml -->
<service
    android:name=".service.SmsAccessibilityService"
    android:permission="android.permission.BIND_ACCESSIBILITY_SERVICE"
    android:exported="true">
    <intent-filter>
        <action android:name="android.accessibilityservice.AccessibilityService" />
    </intent-filter>
    <meta-data
        android:name="android.accessibility_service"
        android:resource="@xml/accessibility_service_config" />
</service>
```

```xml
<!-- res/xml/accessibility_service_config.xml -->
<accessibility-service xmlns:android="http://schemas.android.com/apk/res/android"
    android:description="@string/accessibility_service_description"
    android:accessibilityEventTypes="typeNotificationStateChanged"
    android:accessibilityFeedbackType="feedbackGeneric"
    android:canRetrieveWindowContent="true"
    android:notificationTimeout="100"
    android:settingsActivity=".SettingsActivity" />
```

#### 4.1.2 无障碍服务实现

```kotlin
// SmsAccessibilityService.kt
class SmsAccessibilityService : AccessibilityService() {
    
    override fun onAccessibilityEvent(event: AccessibilityEvent) {
        if (event.eventType == AccessibilityEvent.TYPE_NOTIFICATION_STATE_CHANGED) {
            val packageName = event.packageName?.toString() ?: return
            
            // 检查是否为短信应用
            if (isSmsPackage(packageName)) {
                val notification = event.parcelableData as? Notification
                val smsData = extractSmsFromNotification(notification, event)
                
                smsData?.let {
                    sendToPc(it)
                }
            }
        }
    }
    
    private fun extractSmsFromNotification(
        notification: Notification?, 
        event: AccessibilityEvent
    ): SmsData? {
        // 从通知中提取短信内容
        val extras = notification?.extras
        return SmsData(
            sender = extras?.getString(Notification.EXTRA_TITLE) ?: "",
            body = extras?.getCharSequence(Notification.EXTRA_TEXT)?.toString() ?: "",
            timestamp = System.currentTimeMillis(),
            packageName = event.packageName?.toString() ?: ""
        )
    }
    
    private fun sendToPc(smsData: SmsData) {
        // 通过Socket发送到PC
        coroutineScope.launch {
            networkClient.send(smsData.toJson())
        }
    }
}
```

#### 4.1.3 网络通信模块

```kotlin
// NetworkClient.kt
class NetworkClient(private val host: String, private val port: Int) {
    private var socket: Socket? = null
    private var outputStream: OutputStream? = null
    
    suspend fun connect() {
        withContext(Dispatchers.IO) {
            socket = Socket(host, port)
            outputStream = socket?.getOutputStream()
        }
    }
    
    suspend fun send(data: String) {
        withContext(Dispatchers.IO) {
            outputStream?.write(data.toByteArray())
            outputStream?.flush()
        }
    }
    
    fun disconnect() {
        outputStream?.close()
        socket?.close()
    }
}
```

#### 4.1.4 数据模型

```kotlin
// models/SmsData.kt
data class SmsData(
    val sender: String,
    val body: String,
    val timestamp: Long,
    val packageName: String,
    val category: String? = null
) {
    fun toJson(): String {
        return Gson().toJson(this)
    }
}
```

### 4.2 PC服务端设计（Rust）

#### 4.2.1 项目结构

```
pc-server/
├── Cargo.toml
├── config/
│   └── config.yaml
├── src/
│   ├── main.rs                    # 入口
│   ├── lib.rs                     # 库入口
│   ├── config.rs                  # 配置管理
│   ├── error.rs                   # 错误定义
│   ├── core/
│   │   ├── mod.rs
│   │   ├── adb.rs                 # ADB管理
│   │   ├── receiver.rs            # Socket接收器
│   │   └── device.rs              # 设备管理
│   ├── forwarder/
│   │   ├── mod.rs
│   │   ├── infopush.rs            # InfoPush客户端
│   │   └── message_builder.rs    # 消息构建
│   ├── classifier/
│   │   ├── mod.rs
│   │   ├── rules.rs               # 分类规则
│   │   └── category.rs            # 分类定义
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── sqlite.rs              # SQLite存储
│   │   └── models.rs              # 数据模型
│   └── server/
│       ├── mod.rs
│       └── tcp_server.rs          # TCP服务器
└── .env
```

#### 4.2.2 主要依赖配置

```toml
# Cargo.toml
[package]
name = "usb-sms-reader"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.11", features = ["json", "tokio-rustls"] }
rusqlite = { version = "0.30", features = ["bundled"] }
config = "0.14"
serde_yaml = "0.9"
regex = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror = "1"
anyhow = "1"
tokio-util = { version = "0.7", features = ["codec"] }
bytes = "1"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tokio-test = "0.4"
```

#### 4.2.3 配置管理

```yaml
# config/config.yaml
server:
  listen_host: "0.0.0.0"
  listen_port: 8080
  workers: 4

adb:
  path: "adb"  # or full path
  device_timeout: 30

infopush:
  enabled: true
  server_url: "http://your-infopush-server:8000"
  push_token: "${INFOPUSH_PUSH_TOKEN}"  # 从环境变量读取
  timeout: 10
  retry_count: 3

storage:
  type: "sqlite"
  path: "./data/sms.db"

classifier:
  enabled: true
  rules:
    verification:
      keywords: ["验证码", "code", "动态码", "校验码", "auth"]
      patterns: ["\\d{4,6}"]
    notification:
      keywords: ["通知", "提醒", "成功", "失败", "已到账", "已发货"]
    promotion:
      keywords: ["优惠", "促销", "折扣", "中奖", "免费", "红包"]
    finance:
      keywords: ["银行", "支付", "转账", "余额", "账单"]

logging:
  level: "info"
  file: "./logs/app.log"
```

#### 4.2.4 核心代码示例

**配置模块** (`src/config.rs`):

```rust
use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub adb: AdbConfig,
    pub infopush: InfoPushConfig,
    pub storage: StorageConfig,
    pub classifier: ClassifierConfig,
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
    pub rules: std::collections::HashMap<String, CategoryRule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CategoryRule {
    pub keywords: Vec<String>,
    pub patterns: Option<Vec<String>>,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name("config/config"))
            .build()?;
        
        let mut settings: Settings = config.try_deserialize()?;
        
        // 从环境变量替换敏感信息
        if let Ok(token) = std::env::var("INFOPUSH_PUSH_TOKEN") {
            settings.infopush.push_token = token;
        }
        
        Ok(settings)
    }
}
```

**短信分类器** (`src/classifier/rules.rs`):

```rust
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum SmsCategory {
    Verification,
    Notification,
    Promotion,
    Finance,
    Default,
}

impl SmsCategory {
    pub fn emoji(&self) -> &'static str {
        match self {
            SmsCategory::Verification => "🔐",
            SmsCategory::Notification => "📢",
            SmsCategory::Promotion => "🎉",
            SmsCategory::Finance => "💰",
            SmsCategory::Default => "📱",
        }
    }
    
    pub fn label(&self) -> &'static str {
        match self {
            SmsCategory::Verification => "验证码",
            SmsCategory::Notification => "通知",
            SmsCategory::Promotion => "营销",
            SmsCategory::Finance => "金融",
            SmsCategory::Default => "其他",
        }
    }
}

pub struct SmsClassifier {
    rules: HashMap<String, (Vec<String>, Option<Vec<Regex>>)>,
}

impl SmsClassifier {
    pub fn new(rules: HashMap<String, (Vec<String>, Option<Vec<String>>)>) -> Self {
        // 编译正则表达式
        let compiled_rules = rules
            .into_iter()
            .map(|(category, (keywords, patterns))| {
                let compiled_patterns = patterns.map(|p| {
                    p.into_iter()
                        .filter_map(|pat| Regex::new(&pat).ok())
                        .collect::<Vec<_>>()
                });
                (category, (keywords, compiled_patterns))
            })
            .collect();
        
        SmsClassifier { rules: compiled_rules }
    }
    
    pub fn classify(&self, content: &str) -> SmsCategory {
        let content_lower = content.to_lowercase();
        
        // 按优先级检查
        let priority_order = [
            SmsCategory::Verification,
            SmsCategory::Finance,
            SmsCategory::Notification,
            SmsCategory::Promotion,
        ];
        
        for category in priority_order {
            if let Some((keywords, patterns)) = self.rules.get(category.label()) {
                // 检查关键词
                for keyword in keywords {
                    if content_lower.contains(&keyword.to_lowercase()) {
                        return category;
                    }
                }
                
                // 检查正则
                if let Some(ref compiled_patterns) = patterns {
                    for pattern in compiled_patterns {
                        if pattern.is_match(&content_lower) {
                            return category;
                        }
                    }
                }
            }
        }
        
        SmsCategory::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_classify_verification() {
        let classifier = create_test_classifier();
        assert_eq!(
            classifier.classify("您的验证码是123456"),
            SmsCategory::Verification
        );
    }
    
    fn create_test_classifier() -> SmsClassifier {
        let mut rules = HashMap::new();
        rules.insert(
            "验证码".to_string(),
            (
                vec!["验证码".to_string(), "code".to_string()],
                Some(vec!["\\d{4,6}".to_string()]),
            ),
        );
        SmsClassifier::new(rules)
    }
}
```

**InfoPush客户端** (`src/forwarder/infopush.rs`):

```rust
use reqwest::Client;
use serde::Serialize;
use thiserror::Error;
use crate::config::InfoPushConfig;

#[derive(Debug, Error)]
pub enum InfoPushError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("Push token not configured")]
    TokenNotConfigured,
}

#[derive(Debug, Serialize)]
pub struct PushMessage {
    pub title: String,
    pub content: String,
    #[serde(rename = "content_type")]
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PushResult {
    pub message_id: String,
    pub online_devices: u32,
}

pub struct InfoPushClient {
    config: InfoPushConfig,
    client: Client,
}

impl InfoPushClient {
    pub fn new(config: InfoPushConfig) -> Self {
        InfoPushClient {
            config,
            client: Client::new(),
        }
    }
    
    pub async fn push(&self, message: PushMessage) -> Result<PushResult, InfoPushError> {
        if self.config.push_token.is_empty() {
            return Err(InfoPushError::TokenNotConfigured);
        }
        
        let url = format!(
            "{}/push/{}",
            self.config.server_url, 
            self.config.push_token
        );
        
        let response = self.client
            .post(&url)
            .json(&message)
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await?;
        
        let result: serde_json::Value = response.json().await?;
        
        Ok(PushResult {
            message_id: result["message_id"].as_str().unwrap_or("").to_string(),
            online_devices: result["online_devices"].as_u64().unwrap_or(0) as u32,
        })
    }
}
```

**TCP服务器** (`src/server/tcp_server.rs`):

```rust
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncBufReadExt, BufReader};
use tokio_util::codec::{FramedRead, LinesCodec};
use tracing::{info, error, warn};

pub struct TcpServer {
    addr: String,
}

impl TcpServer {
    pub fn new(host: &str, port: u16) -> Self {
        TcpServer {
            addr: format!("{}:{}", host, port),
        }
    }
    
    pub async fn run<F, Fut>(&self, handler: F)
    where
        F: Fn(String) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = match TcpListener::bind(&self.addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind to {}: {}", self.addr, e);
                return;
            }
        };
        
        info!("Server listening on {}", self.addr);
        
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New connection from: {}", addr);
                    
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();
                        
                        loop {
                            match reader.read_line(&mut line).await {
                                Ok(0) => {
                                    // Connection closed
                                    info!("Connection closed: {}", addr);
                                    break;
                                }
                                Ok(_) => {
                                    // Process the line
                                    let trimmed = line.trim();
                                    if !trimmed.is_empty() {
                                        handler(trimmed.to_string()).await;
                                    }
                                    line.clear();
                                }
                                Err(e) => {
                                    error!("Error reading from {}: {}", addr, e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
    }
}
```

**数据库存储** (`src/storage/sqlite.rs`):

```rust
use rusqlite::{Connection, Result as SqliteResult};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SmsRecord {
    pub id: i64,
    pub sender: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub category: String,
    pub forwarded: bool,
    pub created_at: DateTime<Utc>,
}

pub struct SmsStorage {
    conn: Connection,
}

impl SmsStorage {
    pub fn new(path: &str) -> SqliteResult<Self> {
        let conn = Connection::open(path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sms (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sender TEXT NOT NULL,
                body TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                category TEXT NOT NULL,
                forwarded INTEGER DEFAULT 0,
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        
        Ok(SmsStorage { conn })
    }
    
    pub fn insert(&self, record: &SmsRecord) -> SqliteResult<i64> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO sms (sender, body, timestamp, category, forwarded, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        )?;
        
        stmt.insert((
            &record.sender,
            &record.body,
            record.timestamp.to_rfc3339(),
            &record.category,
            if record.forwarded { 1 } else { 0 },
            record.created_at.to_rfc3339(),
        ))
    }
    
    pub fn get_latest(&self, limit: usize) -> SqliteResult<Vec<SmsRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sender, body, timestamp, category, forwarded, created_at
             FROM sms ORDER BY timestamp DESC LIMIT ?1"
        )?;
        
        stmt.query_map([limit], |row| {
            Ok(SmsRecord {
                id: row.get(0)?,
                sender: row.get(1)?,
                body: row.get(2)?,
                timestamp: row.get::<_, String>(3)?.parse().unwrap_or(Utc::now()),
                category: row.get(4)?,
                forwarded: row.get::<_, i32>(5)? != 0,
                created_at: row.get::<_, String>(6)?.parse().unwrap_or(Utc::now()),
            })
        })?
        .collect()
    }
}
```

**主程序** (`src/main.rs`):

```rust
mod config;
mod error;
mod core;
mod forwarder;
mod classifier;
mod storage;
mod server;

use config::Settings;
use forwarder::{InfoPushClient, PushMessage};
use classifier::{SmsClassifier, SmsCategory};
use storage::{SmsStorage, SmsRecord};
use server::TcpServer;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;
use chrono::Utc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    
    // 加载配置
    let settings = Settings::new()?;
    info!("Configuration loaded successfully");
    
    // 初始化组件
    let classifier = SmsClassifier::new(settings.classifier.clone());
    let storage = SmsStorage::new(&settings.storage.path)?;
    let infopush = if settings.infopush.enabled {
        Some(InfoPushClient::new(settings.infopush.clone()))
    } else {
        None
    };
    
    // 创建TCP服务器
    let server = TcpServer::new(
        &settings.server.listen_host,
        settings.server.listen_port
    );
    
    // 定义消息处理函数
    let handler = |line: String| {
        let classifier = classifier.clone();
        let storage = storage.clone();
        let infopush = infopush.clone();
        
        async move {
            // 解析短信数据
            match serde_json::from_str::<SmsDataJson>(&line) {
                Ok(sms_data) => {
                    // 分类
                    let category = classifier.classify(&sms_data.body);
                    
                    // 存储
                    let record = SmsRecord {
                        id: 0,
                        sender: sms_data.sender.clone(),
                        body: sms_data.body.clone(),
                        timestamp: Utc::now(),
                        category: category.label().to_string(),
                        forwarded: false,
                        created_at: Utc::now(),
                    };
                    
                    if let Err(e) = storage.insert(&record) {
                        error!("Failed to save SMS: {}", e);
                    }
                    
                    // 转发
                    if let Some(ref client) = infopush {
                        let message = PushMessage {
                            title: format!("{} 【{}】", category.emoji(), sms_data.sender),
                            content: format!(
                                "[{}]\n{}\n\n{}",
                                category.label(),
                                sms_data.body,
                                record.timestamp.format("%Y-%m-%d %H:%M:%S")
                            ),
                            content_type: "text".to_string(),
                            url: None,
                        };
                        
                        match client.push(message).await {
                            Ok(result) => {
                                info!(
                                    "SMS forwarded successfully. Message ID: {}, Online devices: {}",
                                    result.message_id, result.online_devices
                                );
                            }
                            Err(e) => {
                                error!("Failed to forward SMS: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to parse SMS data: {}", e);
                }
            }
        }
    };
    
    info!("Starting SMS Reader Server...");
    server.run(handler).await;
    
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
struct SmsDataJson {
    sender: String,
    body: String,
    timestamp: i64,
    package_name: String,
}
```

---

## 5. 开发计划

### 阶段一：环境搭建与基础框架（3-5天）
- [ ] 项目初始化（PC端Rust项目、Android项目）
- [ ] 配置管理模块
- [ ] 日志系统
- [ ] 错误处理框架

### 阶段二：手机端APK开发（7-10天）
- [ ] Android项目基础结构
- [ ] 无障碍服务实现
- [ ] Socket通信模块
- [ ] 权限管理与引导界面
- [ ] APK测试与调试

### 阶段三：PC服务端核心功能（10-14天）
- [ ] ADB设备检测与连接
- [ ] TCP服务器实现
- [ ] 短信分类器
- [ ] SQLite存储
- [ ] InfoPush转发客户端
- [ ] 命令行界面(CLI)

### 阶段四：集成测试与优化（5-7天）
- [ ] 端到端集成测试
- [ ] 多设备支持
- [ ] 异常处理与重连机制
- [ ] 性能优化
- [ ] 内存泄漏检查

### 阶段五：文档与打包（3-5天）
- [ ] 完善文档
- [ ] APK打包与签名
- [ ] Rust编译优化（release模式）
- [ ] 部署指南

---

## 6. 技术风险与应对

| 风险 | 概率 | 影响 | 应对措施 |
|------|------|------|----------|
| 不同品牌手机无障碍服务兼容性差异 | 高 | 中 | 测试主流品牌（华为小米OPPOvivo），针对性适配 |
| Android版本差异导致的API变化 | 中 | 中 | 适配Android 8-14，使用兼容性API |
| 用户不愿授权无障碍服务 | 中 | 高 | 提供详细引导，简化操作流程 |
| ADB连接不稳定 | 中 | 中 | 实现心跳检测与自动重连 |
| Socket通信阻塞 | 低 | 中 | 使用异步IO，超时控制 |
| InfoPush服务不可达 | 低 | 中 | 本地缓存，失败重试机制 |

---

## 7. 部署说明

### 7.1 PC服务端部署

#### 编译
```bash
cd pc-server
cargo build --release
```

#### 首次运行
```bash
# 配置环境变量
export INFOPUSH_PUSH_TOKEN="your_push_token_here"

# 运行
./target/release/usb-sms-reader
```

#### 系统服务（Linux systemd）
```ini
# /etc/systemd/user/usb-sms-reader.service
[Unit]
Description=USB SMS Reader Service
After=network.target

[Service]
Type=simple
ExecStart=/path/to/usb-sms-reader
Restart=on-failure
Environment="INFOPUSH_PUSH_TOKEN=your_token"

[Install]
WantedBy=default.target
```

### 7.2 手机端安装

1. 编译APK
```bash
cd android-app
./gradlew assembleDebug
```

2. 通过ADB安装
```bash
adb install app/build/outputs/apk/debug/app-debug.apk
```

3. 授权引导
   - 打开设置 → 无障碍 → 启用SmsReaderService
   - 连接USB，确保USB调试已开启
   - PC端启动服务，等待连接

---

## 8. 后续扩展计划

### 功能扩展
- [ ] Web管理界面
- [ ] REST API接口
- [ ] 短信搜索与筛选
- [ ] 统计数据展示
- [ ] 多设备管理
- [ ] 规则自定义界面

### 平台扩展
- [ ] iOS支持（需研究替代方案）
- [ ] macOS/Linux客户端
- [ ] Docker容器化部署

---

## 9. 参考资源

### 官方文档
- [Android Accessibility Service](https://developer.android.com/guide/topics/ui/accessibility/service)
- [Android Debug Bridge (ADB)](https://developer.android.com/studio/command-line/adb)

### Rust资源
- [Tokio异步运行时](https://tokio.rs/)
- [Rust Book](https://doc.rust-lang.org/book/)

### 项目依赖
- InfoPush项目文档：`/InfoPush/README.md`
- ADB通信协议：基于TCP端口转发

---

## 更新记录

| 日期 | 版本 | 说明 |
|------|------|------|
| 2026-04-07 | v1.0 | 初始版本，确定技术路线 |