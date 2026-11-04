use crate::{errors::ClientResult, MaxClient};
use crate::models::{Response, StoryOwner};
use serde_json::{json, Value};

impl MaxClient {
    pub async fn get_stories_feed(
        &self,
        cursor: Option<String>,
        count: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "cursor": cursor.unwrap_or_else(|| "0".to_string()),
            "count": count,
        });
        self.send_and_wait(208, payload, 0).await
    }

    pub async fn get_stories_list_by_owners(
        &self,
        owners: Vec<StoryOwner>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "owners": owners,
        });
        self.send_and_wait(209, payload, 0).await
    }

    pub async fn get_stories_by_owners(
        &self,
        owners: Vec<StoryOwner>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "owners": owners,
        });
        self.send_and_wait(210, payload, 0).await
    }

    pub async fn get_story_stats(
        &self,
        story_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "storyId": story_id,
        });
        self.send_and_wait(211, payload, 0).await
    }

    pub async fn get_story_detailed_stats(
        &self,
        story_id: u64,
        filter: i64,
        marker: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "storyId": story_id,
            "filter": filter,
            "marker": marker,
        });
        self.send_and_wait(212, payload, 0).await
    }

    pub async fn react_story(
        &self,
        owner: StoryOwner,
        story_id: u64,
        reaction: Option<Value>,
    ) -> ClientResult<Response> {
        let mut payload = serde_json::Map::new();
        payload.insert("owner".to_string(), serde_json::to_value(owner).unwrap_or(Value::Null));
        payload.insert("storyId".to_string(), json!(story_id));
        if let Some(r) = reaction {
            payload.insert("reaction".to_string(), r);
        }
        self.send_and_wait(213, Value::Object(payload), 0).await
    }

    pub async fn mark_story_read(
        &self,
        owner: StoryOwner,
        story_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "owner": owner,
            "storyId": story_id,
        });
        self.send_and_wait(214, payload, 0).await
    }

    pub async fn send_story(
        &self,
        stories: Vec<Value>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "stories": stories,
        });
        self.send_and_wait(215, payload, 0).await
    }

    pub async fn edit_story(
        &self,
        story_id: u64,
        settings: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "storyId": story_id,
            "settings": settings,
        });
        self.send_and_wait(217, payload, 0).await
    }

    pub async fn delete_stories(
        &self,
        story_ids: Vec<u64>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "storyIds": story_ids,
        });
        self.send_and_wait(218, payload, 0).await
    }

    pub async fn get_stories_by_id(
        &self,
        owner: StoryOwner,
        story_ids: Vec<u64>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "owner": owner,
            "storyIds": story_ids,
        });
        self.send_and_wait(220, payload, 0).await
    }
}
