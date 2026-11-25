use crate::{errors::ClientResult, MaxClient};
use chrono::Utc;
use serde_json::{json, Map, Value};
use crate::models::Response;

impl MaxClient {
    pub async fn create_channel(
        &self,
        title: String,
        participant_ids: Option<Vec<i64>>,
        notify: Option<bool>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "message": {
                "cid": Utc::now().timestamp_millis(),
                "attaches": [{
                    "_type": "CONTROL",
                    "event": "new",
                    "chatType": "CHANNEL",
                    "title": title,
                    "userIds": participant_ids.unwrap_or_default()
                }]
            },
            "notify": notify.unwrap_or(true)
        });

        self.send_and_wait(64, payload, 0).await
    }
    pub async fn join_channel(
        &self,
        link: String
    ) -> ClientResult<Response> {
        self.join_group(link).await
    }

    pub async fn leave_channel(
        &self,
        chat_id: i64
    ) -> ClientResult<Response> {
        self.leave_group(chat_id).await
    }

    pub async fn resolve_channel_by_name(
        &self,
        link: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "link": format!("https://max.ru/{link}"),
        });
        self.send_and_wait(89, payload, 0).await
    }

    pub async fn get_members(
        &self,
        chat_id: i64,
        count: i64,
        marker: Option<i64>,
    ) -> ClientResult<Response> {
        let mut payload = Map::new();

        payload.insert("type".into(), json!("MEMBER"));

        if let Some(m) = marker {
            payload.insert("marker".into(), json!(m));
        }

        payload.insert("chatId".into(), json!(chat_id));
        payload.insert("count".into(), json!(count));

        self.send_and_wait(59, Value::Object(payload), 0).await
    }

    pub async fn find_members(
        &self,
        chat_id: i64,
        query: String
    ) -> ClientResult<Response> {
        let payload = json!({
            "type": "MEMBER",
            "query": query,
            "chatId": chat_id,
        });
        self.send_and_wait(59, payload, 0).await
    }

    pub async fn set_channel_access(
        &self,
        chat_id: i64,
        access: String,
        link: Option<String>,
    ) -> ClientResult<Response> {
        let mut map = Map::new();
        map.insert("chatId".into(), json!(chat_id));
        map.insert("access".into(), json!(access));
        if let Some(l) = link {
            if !l.trim().is_empty() {
                map.insert("link".into(), json!(l.trim()));
            }
        }
        self.send_and_wait(55, Value::Object(map), 0).await
    }

    pub async fn revoke_channel_invite_link(
        &self,
        chat_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "revokePrivateLink": true,
        });
        self.send_and_wait(55, payload, 0).await
    }

    pub async fn set_channel_comments(
        &self,
        chat_id: i64,
        enabled: bool,
    ) -> ClientResult<Response> {
        self.set_channel_options(chat_id, Some(enabled), None, None).await
    }

    pub async fn set_channel_options(
        &self,
        chat_id: i64,
        comments: Option<bool>,
        signatures: Option<bool>,
        join_request: Option<bool>,
    ) -> ClientResult<Response> {
        let mut opts = Map::new();
        if let Some(c) = comments {
            opts.insert("COMMENTS".into(), json!(c));
        }
        if let Some(s) = signatures {
            opts.insert("SIGN_ADMIN".into(), json!(s));
        }
        if let Some(j) = join_request {
            opts.insert("JOIN_REQUEST".into(), json!(j));
        }
        let payload = json!({
            "chatId": chat_id,
            "options": Value::Object(opts),
        });
        self.send_and_wait(55, payload, 0).await
    }

    pub async fn transfer_channel_ownership(
        &self,
        chat_id: i64,
        new_owner_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "changeOwnerId": new_owner_id,
        });
        self.send_and_wait(55, payload, 0).await
    }

    pub async fn assign_channel_admin(
        &self,
        chat_id: i64,
        user_id: i64,
        permissions: i64,
        _alias: Option<String>,
    ) -> ClientResult<Response> {
        let mut payload = Map::new();
        payload.insert("chatId".into(), json!(chat_id));
        payload.insert("operation".into(), json!("add"));
        payload.insert("members".into(), json!([{ "userId": user_id }]));
        payload.insert("type".into(), json!("ADMIN"));
        payload.insert("showHistory".into(), json!(true));
        if permissions != 0 {
            payload.insert("permissions".into(), json!(permissions));
        }
        self.send_and_wait(77, Value::Object(payload), 0).await
    }

    pub async fn revoke_channel_admin(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "operation": "remove",
            "members": [{ "userId": user_id }],
            "type": "ADMIN",
        });
        self.send_and_wait(77, payload, 0).await
    }
}
