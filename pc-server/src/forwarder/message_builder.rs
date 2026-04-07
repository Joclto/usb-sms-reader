use chrono::{DateTime, Utc};
use crate::classifier::SmsCategory;

pub fn build_push_message(
    sender: &str,
    body: &str,
    category: &SmsCategory,
    timestamp: &DateTime<Utc>,
) -> crate::forwarder::PushMessage {
    crate::forwarder::PushMessage {
        title: format!("{} 【{}】", category.emoji(), sender),
        content: format!(
            "[{}]\n{}\n\n{}",
            category.label(),
            body,
            timestamp.format("%Y-%m-%d %H:%M:%S")
        ),
        content_type: "text".to_string(),
        url: None,
    }
}