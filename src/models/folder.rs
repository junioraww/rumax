use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFolder {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub include: Vec<i64>,
    #[serde(default)]
    pub filters: Vec<i64>,
    #[serde(default)]
    pub options: Vec<i64>,
    #[serde(default)]
    pub favorites: Vec<i64>,
    #[serde(default)]
    pub update_time: Option<i64>,
    #[serde(default)]
    pub source_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldersPayload {
    #[serde(default)]
    pub folders: Vec<ChatFolder>,
    #[serde(default)]
    pub folders_order: Vec<String>,
    #[serde(default)]
    pub folder_sync: Option<i64>,
}
