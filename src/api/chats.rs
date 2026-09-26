use crate::{errors::ClientResult, MaxClient};
use serde_json::{json, Map, Value};
use crate::models::Response;
use chrono::Utc;
use std::collections::HashMap;

impl MaxClient {
    pub async fn search_public(
        &self,
        query: String,
        count: i32,
        search_type: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "query": query,
            "count": count,
            "type": search_type,
        });

        self.send_and_wait(60, payload, 0).await
    }

    pub async fn search_msg(
        &self,
        query: String,
        count: i32,
        marker: Option<String>,
    ) -> ClientResult<Response> {
        let mut payload = Map::new();

        payload.insert("query".into(), json!(query));
        payload.insert("count".into(), json!(count));

        if let Some(m) = marker {
            payload.insert("marker".into(), json!(m));
        }

        self.send_and_wait(68, Value::Object(payload), 0).await
    }

    pub async fn get_chats(
        &self,
        chat_ids: Vec<i64>
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatIds": chat_ids,
        });

        self.send_and_wait(48, payload, 0).await
    }

    pub async fn create_group(
        &self,
        title: String,
        participant_ids: Option<Vec<i64>>,
        notify: Option<bool>
    ) -> ClientResult<Response> {
        let payload = json!({
            "message": {
                "cid": Utc::now().timestamp_millis(),
                "attaches": [{
                    "_type": "CONTROL",
                    "event": "new",
                    "chatType": "CHAT",
                    "title": title,
                    "userIds": participant_ids.unwrap_or_default()
                }]
            },
            "notify": notify.unwrap_or(true)
        });

        self.send_and_wait(64, payload, 0).await
    }

    pub async fn delete_chat(
        &self,
        chat_id: i64,
        last_event_time: Option<i64>,
        for_all: Option<bool>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "lastEventTime": last_event_time.unwrap_or(Utc::now().timestamp_millis()),
            "forAll": for_all.unwrap_or(false)
        });

        self.send_and_wait(52, payload, 0).await
    }

    pub async fn leave_group(
        &self,
        chat_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id
        });

        self.send_and_wait(58, payload, 0).await
    }

    pub async fn change_group_profile(
        &self,
        chat_id: i64,
        title: Option<String>,
        description: Option<String>,
    ) -> ClientResult<Response> {
        let mut payload = Map::new();

        payload.insert("chatId".into(), json!(chat_id));

        if let Some(t) = title {
            payload.insert("theme".into(), json!(t));
        }

        if let Some(d) = description {
            payload.insert("description".into(), json!(d));
        }

        self.send_and_wait(55, Value::Object(payload), 0).await
    }

    pub async fn join_group(
        &self,
        link: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "link": link,
        });

        self.send_and_wait(57, payload, 0).await
    }

    pub async fn resolve_group_by_link(
        &self,
        link: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "link": link,
        });

        self.send_and_wait(89, payload, 0).await
    }

    pub async fn refresh_invite_link(
        &self,
        chat_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "revokePrivateLink": true,
            "chatId": chat_id,
        });

        self.send_and_wait(55, payload, 0).await
    }

    pub async fn confirm_join_requests(
        &self,
        chat_id: i64,
        user_ids: Vec<i64>,
        show_history: Option<bool>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "userIds": user_ids,
            "type": "JOIN_REQUEST",
            "showHistory": show_history.unwrap_or(true),
            "operation": "add",
        });

        self.send_and_wait(77, payload, 0).await
    }

    pub async fn decline_join_requests(
        &self,
        chat_id: i64,
        user_ids: Vec<i64>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "userIds": user_ids,
            "operation": "remove",
        });

        self.send_and_wait(77, payload, 0).await
    }

    pub async fn change_group_settings(
        &self,
        chat_id: i64,
        all_can_pin_message: Option<bool>,
        only_owner_can_change_icon_title: Option<bool>,
        only_admin_can_add_member: Option<bool>,
        only_admin_can_call: Option<bool>,
        members_can_see_private_link: Option<bool>,
    ) -> ClientResult<Response> {
        let mut settings = Map::new();

        if let Some(b) = all_can_pin_message {
            settings.insert("ALL_CAN_PIN_MESSAGE".into(), json!(b));
            settings.insert("allCanPinMessage".into(), json!(b));
        }

        if let Some(b) = only_owner_can_change_icon_title {
            settings.insert("ONLY_OWNER_CAN_CHANGE_ICON_TITLE".into(), json!(b));
            settings.insert("onlyOwnerCanChangeIconTitle".into(), json!(b));
        }

        if let Some(b) = only_admin_can_add_member {
            settings.insert("ONLY_ADMIN_CAN_ADD_MEMBER".into(), json!(b));
            settings.insert("onlyAdminCanAddMember".into(), json!(b));
        }

        if let Some(b) = only_admin_can_call {
            settings.insert("ONLY_ADMIN_CAN_CALL".into(), json!(b));
            settings.insert("onlyAdminCanCall".into(), json!(b));
        }

        if let Some(b) = members_can_see_private_link {
            settings.insert("MEMBERS_CAN_SEE_PRIVATE_LINK".into(), json!(b));
            settings.insert("membersCanSeePrivateLink".into(), json!(b));
        }

        let payload = json!({
            "chatId": chat_id,
            "options": Value::Object(settings),
        });

        self.send_and_wait(55, payload, 0).await
    }

    pub async fn remove_users_from_group(
        &self,
        chat_id: i64,
        user_ids: Vec<i64>,
        clean_msg_period: i64
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "userIds": user_ids,
            "operation": "remove",
            "cleanMsgPeriod": clean_msg_period
        });

        self.send_and_wait(77, payload, 0).await
    }

    pub async fn invite_users_to_group(
        &self,
        chat_id: i64,
        user_ids: Vec<i64>,
        show_history: Option<bool>
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "userIds": user_ids,
            "showHistory": show_history.unwrap_or(true),
            "operation": "add",
        });

        self.send_and_wait(77, payload, 0).await
    }

    pub async fn get_join_requests(
        &self,
        chat_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "type": "JOIN_REQUEST",
            "count": 100
        });

        self.send_and_wait(59, payload, 0).await
    }

    pub async fn set_chat_mute(
        &self,
        chat_id: i64,
        dont_disturb_until: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "settings": {
                "chats": {
                    chat_id.to_string(): {
                        "dontDisturbUntil": dont_disturb_until
                    }
                }
            }
        });

        self.send_and_wait(22, payload, 0).await
    }

    pub async fn update_user_settings(
        &self,
        settings: HashMap<String, serde_json::Value>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "settings": {
                "user": settings
            }
        });

        self.send_and_wait(22, payload, 0).await
    }

    pub async fn get_folders(
        &self,
        folder_sync: Option<i64>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "folderSync": folder_sync.unwrap_or(0),
        });

        self.send_and_wait(272, payload, 0).await
    }

    pub async fn get_folder_by_id(
        &self,
        folder_ids: Vec<String>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "folderIds": folder_ids,
        });

        self.send_and_wait(273, payload, 0).await
    }

    pub async fn update_folder(
        &self,
        id: String,
        title: String,
        include: Vec<i64>,
        filters: Vec<i64>,
        options: Vec<i64>,
        favorites: Vec<i64>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "id": id,
            "title": title.trim(),
            "include": include,
            "filters": filters,
            "options": options,
            "favorites": favorites,
        });

        self.send_and_wait(274, payload, 0).await
    }

    pub async fn reorder_folders(
        &self,
        folders_order: Vec<String>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "foldersOrder": folders_order,
        });

        self.send_and_wait(275, payload, 0).await
    }

    pub async fn delete_folders(
        &self,
        folder_ids: Vec<String>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "folderIds": folder_ids,
        });

        self.send_and_wait(276, payload, 0).await
    }

    pub async fn get_chat_media(
        &self,
        chat_id: i64,
        message_id: i64,
        attach_types: Vec<String>,
        forward: i32,
        backward: i32,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "attachTypes": attach_types,
            "forward": forward,
            "backward": backward,
        });

        self.send_and_wait(51, payload, 0).await
    }

    pub async fn assign_admin(
        &self,
        chat_id: i64,
        user_id: i64,
        permissions: Vec<String>,
        alias: Option<String>,
    ) -> ClientResult<Response> {
        let mut admin_map = Map::new();
        let mut entry = Map::new();
        entry.insert("permissions".into(), json!(permissions));
        if let Some(a) = alias {
            if !a.trim().is_empty() {
                entry.insert("alias".into(), json!(a.trim()));
            }
        }
        admin_map.insert(user_id.to_string(), Value::Object(entry));

        let payload = json!({
            "chatId": chat_id,
            "adminParticipants": Value::Object(admin_map)
        });

        self.send_and_wait(55, payload, 0).await
    }

    pub async fn revoke_admin(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> ClientResult<Response> {
        let mut admin_map = Map::new();
        admin_map.insert(user_id.to_string(), Value::Null);

        let payload = json!({
            "chatId": chat_id,
            "adminParticipants": Value::Object(admin_map)
        });

        self.send_and_wait(55, payload, 0).await
    }

    pub async fn clear_chat_history(
        &self,
        chat_id: i64,
        last_event_time: Option<i64>,
        for_all: Option<bool>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "lastEventTime": last_event_time.unwrap_or(Utc::now().timestamp_millis()),
            "forAll": for_all.unwrap_or(false)
        });

        self.send_and_wait(54, payload, 0).await
    }
}

