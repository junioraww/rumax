use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BotCommand {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl BotCommand {
    pub fn new(name: impl Into<String>, description: Option<String>) -> Self {
        Self {
            name: name.into(),
            description: description.and_then(|d| {
                let trimmed = d.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
        }
    }

    pub fn slash(&self) -> String {
        format!("/{}", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BotInfo {
    #[serde(default)]
    pub bot_id: u64,
    #[serde(default)]
    pub commands: Vec<BotCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl BotInfo {
    pub fn from_payload(bot_id: u64, payload: &serde_json::Value) -> Self {
        let commands = payload
            .get("commands")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let name = item.get("name")?.as_str()?.to_string();
                        if name.is_empty() {
                            return None;
                        }
                        let description = item
                            .get("description")
                            .and_then(|d| d.as_str())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());
                        Some(BotCommand { name, description })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let contact = payload.get("contact").cloned();
        let description = contact
            .as_ref()
            .and_then(|c| c.get("description"))
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());

        Self {
            bot_id,
            commands,
            contact,
            description,
        }
    }
}
