use crate::errors::ClientResult;
use crate::models::Response;
use crate::MaxClient;
use chrono::Utc;
use serde_json::json;

impl MaxClient {
    pub async fn send_button_callback(
        &self,
        chat_id: i64,
        message_id: u64,
        callback_id: String,
        payload: Option<String>,
    ) -> ClientResult<Response> {
        let mut request_payload = json!({
            "chatId": chat_id,
            "messageId": message_id,
            "callbackId": callback_id,
        });

        if let Some(p) = payload {
            request_payload["payload"] = json!(p);
        }

        self.send_and_wait(118, request_payload, 0).await
    }

    pub async fn send_bot_start(
        &self,
        chat_id: i64,
        start_payload: Option<String>,
    ) -> ClientResult<Response> {
        let mut attach = json!({
            "_type": "CONTROL",
            "event": "botStarted",
        });

        if let Some(p) = start_payload {
            attach["startPayload"] = json!(p);
        }

        let payload = json!({
            "chatId": chat_id,
            "message": {
                "cid": -Utc::now().timestamp_millis(),
                "attaches": [attach],
            }
        });

        self.send_and_wait(64, payload, 0).await
    }

    pub async fn get_bot_info(&self, bot_id: u64) -> ClientResult<Response> {
        let payload = json!({
            "botId": bot_id,
        });
        self.send_and_wait(145, payload, 0).await
    }

    pub async fn get_chat_bot_commands(&self, chat_id: i64) -> ClientResult<Response> {
        let payload = json!({
            "chatId": chat_id,
        });
        self.send_and_wait(144, payload, 0).await
    }

    pub async fn suspend_bot(&self, bot_id: u64) -> ClientResult<Response> {
        let payload = json!({
            "botId": bot_id,
        });
        self.send_and_wait(119, payload, 0).await
    }
}
