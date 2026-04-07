use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    #[serde(rename = "fetch_sms")]
    FetchSms { limit: Option<u32> },
    
    #[serde(rename = "fetch_all_sms")]
    FetchAllSms,
    
    #[serde(rename = "ping")]
    Ping,
    
    #[serde(rename = "get_device_info")]
    GetDeviceInfo,
    
    #[serde(rename = "mark_read")]
    MarkRead { sms_id: i64 },
}

impl Command {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    
    pub fn fetch_sms(limit: Option<u32>) -> Self {
        Command::FetchSms { limit }
    }
    
    pub fn fetch_all_sms() -> Self {
        Command::FetchAllSms
    }
    
    pub fn ping() -> Self {
        Command::Ping
    }
    
    pub fn get_device_info() -> Self {
        Command::GetDeviceInfo
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "sms_list")]
    SmsList { messages: Vec<SmsData> },
    
    #[serde(rename = "pong")]
    Pong,
    
    #[serde(rename = "device_info")]
    DeviceInfo { 
        device_id: String,
        model: String,
        android_version: String,
    },
    
    #[serde(rename = "error")]
    Error { message: String },
    
    #[serde(rename = "new_sms")]
    NewSms { sms: SmsData },
    
    #[serde(rename = "success")]
    Success { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsData {
    pub id: i64,
    pub sender: String,
    pub body: String,
    pub timestamp: i64,
    pub read: bool,
}

impl From<SmsData> for crate::app::state::SmsMessage {
    fn from(data: SmsData) -> Self {
        crate::app::state::SmsMessage {
            id: data.id,
            sender: data.sender,
            body: data.body,
            timestamp: data.timestamp,
            category: String::new(),
            read: data.read,
            sim_slot: -1,
        }
    }
}