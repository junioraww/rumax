use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    pub ver: u8,
    #[serde(default)]
    pub cmd: u8,
    pub seq: u64,
    pub opcode: u16,
    pub payload: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Response {
    pub ver: u8,
    pub cmd: u8,
    pub seq: u64,
    pub opcode: u16,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub device_id: String,
    pub mt_instance: String,
    pub user_agent: UserAgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAgent {
    pub device_type: String,
    pub app_version: String,
    pub os_version: String,
    pub timezone: String,
    pub screen: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_device_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,

    pub locale: String,
    pub build_number: i32,
    pub device_name: String,
    pub device_locale: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_user_agent: Option<String>,
}


impl UserAgent {
    pub fn to_web_payload(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(m) = v.as_object_mut() {
            m.retain(|k, _| matches!(k.as_str(),
                "deviceType" | "locale" | "deviceLocale" | "osVersion" | "deviceName" |
                "headerUserAgent" | "appVersion" | "screen" | "timezone"));
            m.entry("headerUserAgent").or_insert(serde_json::json!(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36"));
        }
        v
    }
}
