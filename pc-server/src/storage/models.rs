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

impl SmsRecord {
    pub fn new(sender: String, body: String, category: String) -> Self {
        let now = Utc::now();
        SmsRecord {
            id: 0,
            sender,
            body,
            timestamp: now,
            category,
            forwarded: false,
            created_at: now,
        }
    }
}