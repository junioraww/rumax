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

    pub async fn open_web_app(
        &self,
        bot_id: u64,
        start_param: Option<String>,
        chat_id: Option<i64>,
    ) -> ClientResult<Response> {
        let mut payload = json!({
            "botId": bot_id,
        });
        if let Some(param) = start_param {
            if !param.trim().is_empty() {
                payload["startParam"] = json!(param);
            }
        }
        if let Some(cid) = chat_id {
            payload["chatId"] = json!(cid);
        }
        self.send_and_wait(160, payload, 0).await
    }

    pub async fn share_phone_with_bot(&self, bot_id: u64) -> ClientResult<Response> {
        let payload = json!({
            "botId": bot_id,
        });
        self.send_and_wait(106, payload, 0).await
    }

    pub async fn submit_external_callback(&self, url: String) -> ClientResult<Response> {
        let payload = json!({
            "url": url,
        });
        self.send_and_wait(105, payload, 0).await
    }
}

