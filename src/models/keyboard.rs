use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InlineKeyboardButton {
    #[serde(default, rename = "type")]
    pub button_type: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

impl InlineKeyboardButton {
    pub fn new(button_type: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            button_type: button_type.into().to_uppercase(),
            text: text.into(),
            url: None,
            web_app: None,
            contact_id: None,
            payload: None,
        }
    }

    pub fn callback(text: impl Into<String>, payload: impl Into<String>) -> Self {
        Self {
            button_type: "CALLBACK".into(),
            text: text.into(),
            url: None,
            web_app: None,
            contact_id: None,
            payload: Some(payload.into()),
        }
    }

    pub fn url(text: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            button_type: "LINK".into(),
            text: text.into(),
            url: Some(url.into()),
            web_app: None,
            contact_id: None,
            payload: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InlineKeyboardLayout {
    #[serde(default)]
    pub buttons: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InlineKeyboardAttachment {
    #[serde(default = "default_keyboard_type", rename = "_type")]
    pub attach_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_id: Option<String>,
    #[serde(default)]
    pub keyboard: InlineKeyboardLayout,
}

fn default_keyboard_type() -> String {
    "INLINE_KEYBOARD".into()
}

impl InlineKeyboardAttachment {
    pub fn new(callback_id: Option<String>, rows: Vec<Vec<InlineKeyboardButton>>) -> Self {
        Self {
            attach_type: default_keyboard_type(),
            callback_id,
            keyboard: InlineKeyboardLayout { buttons: rows },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keyboard.buttons.iter().all(|row| row.is_empty())
    }
}
