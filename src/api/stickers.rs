use crate::{errors::ClientResult, MaxClient};
use serde_json::json;
use crate::models::Response;
use chrono::Utc;

impl MaxClient {
    pub async fn get_sticker_sections(&self, sync: i64) -> ClientResult<Response> {
        let payload = json!({
            "type": "STICKER",
            "sync": sync,
        });
        self.send_and_wait(27, payload, 0).await
    }

    pub async fn get_favorite_stickers(&self, sync: i64) -> ClientResult<Response> {
        let payload = json!({
            "type": "FAVORITE_STICKER",
            "sync": sync,
        });
        self.send_and_wait(27, payload, 0).await
    }

    pub async fn get_assets_section(
        &self,
        section_id: String,
        from: i64,
        count: i32,
    ) -> ClientResult<Response> {
        let payload = json!({
            "sectionId": section_id,
            "from": from,
            "count": count,
        });
        self.send_and_wait(26, payload, 0).await
    }

    pub async fn get_assets_by_ids(
        &self,
        asset_type: String,
        ids: Vec<i64>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "type": asset_type,
            "ids": ids,
        });
        self.send_and_wait(28, payload, 0).await
    }

    pub async fn add_favorite_sticker_set(&self, set_id: i64) -> ClientResult<Response> {
        let payload = json!({
            "type": "FAVORITE_STICKER_SET",
            "id": set_id,
        });
        self.send_and_wait(29, payload, 0).await
    }

    pub async fn remove_favorite_sticker_set(&self, set_id: i64) -> ClientResult<Response> {
        let payload = json!({
            "type": "FAVORITE_STICKER_SET",
            "ids": [set_id],
        });
        self.send_and_wait(259, payload, 0).await
    }

    pub async fn move_asset(
        &self,
        asset_type: String,
        id: i64,
        position: i32,
    ) -> ClientResult<Response> {
        let payload = json!({
            "type": asset_type,
            "id": id,
            "position": position,
        });
        self.send_and_wait(260, payload, 0).await
    }

    pub async fn resolve_link(&self, link: String) -> ClientResult<Response> {
        let clean_link = if link.starts_with("http://") || link.starts_with("https://") {
            link
        } else {
            format!("https://max.ru/{link}")
        };
        let payload = json!({
            "link": clean_link,
        });
        self.send_and_wait(89, payload, 0).await
    }

    pub async fn send_sticker_message(
        &self,
        chat_id: i64,
        sticker_id: i64,
        notify: Option<bool>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "message": {
                "cid": -Utc::now().timestamp_millis(),
                "text": "",
                "elements": [],
                "attaches": [
                    {
                        "_type": "STICKER",
                        "stickerId": sticker_id,
                    }
                ],
            },
            "notify": notify.unwrap_or(true),
        });
        self.send_and_wait(64, payload, 0).await
    }
}
