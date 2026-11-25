use crate::{errors::ClientResult, MaxClient};
use crate::models::Response;
use serde_json::json;

impl MaxClient {
    pub async fn get_calls(
        &self,
        forward: bool,
        count: i64,
    ) -> ClientResult<Response> {
        let payload = json!({ "forward": forward, "count": count });
        self.send_and_wait(79, payload, 0).await
    }

    pub async fn start_outgoing_call(
        &self,
        callee_id: u64,
        is_video: bool,
        conversation_id: String,
        internal_params: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "conversationId": conversation_id,
            "calleeIds": [callee_id],
            "internalParams": internal_params,
            "isVideo": is_video,
        });
        self.send_and_wait(78, payload, 0).await
    }

    pub async fn start_group_call(
        &self,
        chat_id: i64,
        is_video: bool,
        conversation_id: String,
        internal_params: String,
    ) -> ClientResult<Response> {
        self.start_group_call_with_callees(chat_id, is_video, conversation_id, internal_params, Vec::new()).await
    }

    pub async fn start_group_call_with_callees(
        &self,
        chat_id: i64,
        is_video: bool,
        conversation_id: String,
        internal_params: String,
        callee_ids: Vec<u64>,
    ) -> ClientResult<Response> {
        let mut payload = json!({
            "conversationId": conversation_id,
            "chatId": chat_id,
            "internalParams": internal_params,
            "isVideo": is_video,
        });
        if !callee_ids.is_empty() {
            payload["calleeIds"] = json!(callee_ids);
        }
        self.send_and_wait(78, payload, 0).await
    }

    pub async fn open_conference(
        &self,
        conversation_id: String,
    ) -> ClientResult<Response> {
        let payload = json!({ "conversationId": conversation_id });
        self.send_and_wait(76, payload, 0).await
    }

    pub async fn enter_by_link(
        &self,
        join_link: String,
        is_video: bool,
        internal_params: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "joinLink": join_link,
            "internalParams": internal_params,
            "isVideo": is_video,
        });
        self.send_and_wait(166, payload, 0).await
    }

    pub async fn make_invite_link(
        &self,
        conversation_id: String,
    ) -> ClientResult<Response> {
        let payload = json!({ "conversationId": conversation_id });
        self.send_and_wait(84, payload, 0).await
    }

    pub async fn erase_call_records(
        &self,
        history_ids: Vec<i64>,
    ) -> ClientResult<Response> {
        let payload = json!({ "historyIds": history_ids });
        self.send_and_wait(164, payload, 0).await
    }
}
