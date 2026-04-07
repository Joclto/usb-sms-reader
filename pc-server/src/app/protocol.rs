use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub version: u8,
    #[serde(flatten)]
    pub payload: MessagePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum MessagePayload {
    #[serde(rename = "command")]
    Command { data: CommandData },
    
    #[serde(rename = "response")]
    Response { data: ResponseData },
    
    #[serde(rename = "event")]
    Event { data: EventData },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandData {
    pub command: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseData {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum EventData {
    #[serde(rename = "new_sms")]
    NewSms { sender: String, body: String, timestamp: i64 },
    
    #[serde(rename = "device_connected")]
    DeviceConnected { device_id: String },
    
    #[serde(rename = "device_disconnected")]
    DeviceDisconnected { device_id: String },
    
    #[serde(rename = "status_update")]
    StatusUpdate { status: String },
}

impl ProtocolMessage {
    pub fn command(cmd: &str, params: serde_json::Value) -> Self {
        ProtocolMessage {
            version: 1,
            payload: MessagePayload::Command {
                data: CommandData {
                    command: cmd.to_string(),
                    params,
                },
            },
        }
    }
    
    pub fn response(success: bool, data: Option<serde_json::Value>, error: Option<String>) -> Self {
        ProtocolMessage {
            version: 1,
            payload: MessagePayload::Response {
                data: ResponseData { success, data, error },
            },
        }
    }
    
    pub fn event(event_data: EventData) -> Self {
        ProtocolMessage {
            version: 1,
            payload: MessagePayload::Event { data: event_data },
        }
    }
    
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}