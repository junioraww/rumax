use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryCoordinates {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default)]
    pub rotation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryClickableLink {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_result: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryLayer {
    #[serde(rename = "type", default)]
    pub layer_type: i64,
    pub coordinates: StoryCoordinates,
    pub clickable_link: StoryClickableLink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryOwner {
    pub owner_id: u64,
    #[serde(rename = "type", default)]
    pub owner_type: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryMedia {
    #[serde(rename = "_type")]
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_type: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryItem {
    pub id: Option<u64>,
    pub cid: Option<u64>,
    pub media: StoryMedia,
    #[serde(default)]
    pub layers: Vec<StoryLayer>,
    #[serde(default)]
    pub settings: i64,
    #[serde(default)]
    pub expiration: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryPreview {
    pub owner: StoryOwner,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub read_count: i64,
    #[serde(default)]
    pub last_story_expiration_time: i64,
    #[serde(default)]
    pub settings: i64,
    #[serde(default)]
    pub update_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerStories {
    pub owner: StoryOwner,
    #[serde(default)]
    pub stories: Vec<StoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryDetailedStats {
    pub user_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<Value>,
}
