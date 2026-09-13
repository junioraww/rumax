use crate::{errors::ClientResult, MaxClient};
use crate::models::Response;
use serde_json::json;

impl MaxClient {
    pub async fn sync_contacts(&self) -> ClientResult<Response> {
        self.send_and_wait(8, json!({
            "contactsSync": 0
        }), 0).await
    }

    pub async fn get_by_phone(
        &self,
        phone: String
    ) -> ClientResult<Response> {
        let payload = json!({
            "phone": phone
        });
        self.send_and_wait(46, payload, 0).await
    }
    
    pub async fn fetch_contacts(
        &self,
        user_ids: Vec<u64>
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactIds": user_ids
        });
        self.send_and_wait(32, payload, 0).await
    }
    
    pub async fn add_contact(
        &self,
        user_id: u64,
        first_name: String
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactId": user_id,
            "firstName": first_name,
            "action": "ADD"
        });
        self.send_and_wait(34, payload, 0).await
    }
    
    pub async fn delete_contact(
        &self,
        user_id: u64
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactId": user_id,
            "action": "REMOVE",
        });
        self.send_and_wait(34, payload, 0).await
    }
}

