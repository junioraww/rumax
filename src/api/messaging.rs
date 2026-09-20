use crate::{errors::ClientResult, MaxClient};
use crate::models::{Response, FetchHistoryOptions};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Duration;
use chrono::Utc;

impl MaxClient {
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: String,
        args: Option<HashMap<String, serde_json::Value>>,
    ) -> ClientResult<Response> {
        let args_map = args.unwrap_or_default();

        let mut message = Map::new();

        if !text.is_empty() {
            message.insert("text".into(), json!(text));
        }
        let cid = args_map
            .get("cid")
            .and_then(|c| c.as_i64())
            .unwrap_or_else(|| -(Utc::now().timestamp_millis() as i64));
        message.insert("cid".into(), json!(cid));
        message.insert("isLive".into(), json!(false));
        message.insert("detectShare".into(), json!(false));
        message.insert(
            "elements".into(),
            args_map.get("elements").cloned().unwrap_or(json!([])),
        );
        message.insert(
            "attaches".into(),
            args_map.get("attaches").cloned().unwrap_or(json!([])),
        );

        if let Some(link) = args_map.get("replyTo").and_then(|id| {
            id.as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .map(|num| {
                json!({
                    "type": "REPLY",
                    "messageId": num
                })
            })
        }) {
            message.insert("link".into(), link);
        }

        let payload = json!({
            "chatId": chat_id,
            "message": message,
            "notify": args_map.get("notify").cloned().unwrap_or(json!(true)),
        });

        for attempt in 0..60 {
            let res = self.send_and_wait(64, payload.clone(), 0).await;
            match res {
                Ok(response) => return Ok(response),
                Err(crate::errors::Error::ApiResponse(ref err_val)) => {
                    let err_str = err_val.to_string();
                    if (err_str.contains("not.ready") || err_str.contains("not_ready")) && attempt < 59 {
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                        continue;
                    }
                    return Err(crate::errors::Error::ApiResponse(err_val.clone()));
                }
                Err(e) => return Err(e),
            }
        }

        self.send_and_wait(64, payload, 0).await
    }
    
    
    pub async fn add_reaction(
        &self,
        chat_id: i64,
        message_id: u64,
        reaction: String
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "reaction": {
                "reactionType": "EMOJI",
                "id": reaction,
            }
        });
        self.send_and_wait(178, payload, 0).await
    }
    
    pub async fn remove_reaction(
        &self,
        chat_id: i64,
        message_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
        });
        self.send_and_wait(179, payload, 0).await
    }

    pub async fn read_message(
        &self,
        chat_id: i64,
        message_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "type": "READ_MESSAGE",
            "chatId": chat_id,
            "messageId": message_id,
            "mark": Utc::now().timestamp_millis() as u64,
        });
        self.send_and_wait(50, payload, 0).await
    }

    pub async fn pin_message(
        &self,
        chat_id: i64,
        message_id: u64,
        notify_pin: bool,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "notifyPin": notify_pin,
            "pinMessageId": message_id,
        });
        self.send_and_wait(55, payload, 0).await
    }

    pub async fn delete_message(
        &self,
        chat_id: i64,
        message_id: u64,
        for_me: bool,
    ) -> ClientResult<Response> {
        self.delete_messages(chat_id, vec![message_id], for_me).await
    }

    pub async fn delete_messages(
        &self,
        chat_id: i64,
        message_ids: Vec<u64>,
        for_me: bool,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageIds": message_ids,
            "forMe": for_me,
        });

        self.send_and_wait(66, payload, 0).await
    }

    pub async fn edit_message(
        &self,
        chat_id: i64,
        message_id: u64,
        text: String,
        attaches: Option<Vec<Value>>,
        elements: Option<Vec<Value>>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "text": text,
            "elements": elements.unwrap_or_default(),
            "attaches": attaches.unwrap_or_default(),
        });
        self.send_and_wait(67, payload, 0).await
    }

    pub async fn fetch_history(
        &self,
        chat_id: i64,
        opts: Option<FetchHistoryOptions>,
    ) -> ClientResult<Response> {
        let opts = opts.unwrap_or_default();

        let payload = serde_json::json!({
            "chatId": chat_id,
            "forward": opts.forward,
            "backward": opts.backward,
            "backwardTime": opts.backward_time,
            "forwardTime": opts.forward_time,
            "getChat": opts.get_chat,
            "from": opts.from_time.unwrap_or_else(|| Utc::now().timestamp_millis() as u64),
            "itemType": opts.item_type,
            "getMessages": opts.get_messages,
            "interactive": opts.interactive,
        });

        self.send_and_wait(49, payload, 0).await
    }

    pub async fn get_video_by_id(
        &self,
        chat_id: i64,
        message_id: u64,
        video_id: i64,
        token: Option<String>,
    ) -> ClientResult<Response> {
        let mut payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "videoId": video_id
        });
        if let Some(t) = token {
            payload["token"] = json!(t);
        }
        self.send_and_wait(83, payload, 0).await
    }

    pub async fn get_file_by_id(
        &self,
        chat_id: i64,
        message_id: u64,
        file_id: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "fileId": file_id
        });
        self.send_and_wait(88, payload, 0).await
    }
    
    pub async fn request_transcription(
        &self,
        chat_id: i64,
        message_id: u64,
        media_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "mediaId": media_id,
        });
        self.send_and_wait(202, payload, 0).await
    }
}
