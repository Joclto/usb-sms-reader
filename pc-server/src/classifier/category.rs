#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

impl From<&str> for SmsCategory {
    fn from(s: &str) -> Self {
        match s {
            "验证码" | "verification" => SmsCategory::Verification,
            "通知" | "notification" => SmsCategory::Notification,
            "营销" | "promotion" => SmsCategory::Promotion,
            "金融" | "finance" => SmsCategory::Finance,
            _ => SmsCategory::Default,
        }
    }
}