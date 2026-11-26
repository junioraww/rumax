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

    pub async fn fetch_presence(
        &self,
        user_ids: Vec<u64>
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactIds": user_ids
        });
        self.send_and_wait(35, payload, 0).await
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
    
    /// Updates the name of an existing contact.
    pub async fn update_contact_name(
        &self,
        user_id: u64,
        first_name: &str,
        last_name: Option<&str>,
    ) -> ClientResult<Response> {
        let mut payload = json!({
            "contactId": user_id,
            "firstName": first_name,
            "action": "UPDATE",
        });
        if let Some(last) = last_name.filter(|s| !s.is_empty()) {
            payload["lastName"] = json!(last);
        }
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

    pub async fn get_contact_photos(
        &self,
        contact_id: u64,
        from: i64,
        count: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactId": contact_id,
            "from": from,
            "count": count,
        });
        self.send_and_wait(39, payload, 0).await
    }

    pub async fn remove_contact_photo(
        &self,
        photo_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "photoId": photo_id,
        });
        self.send_and_wait(43, payload, 0).await
    }

    pub async fn get_blacklist(
        &self,
        from: i64,
        count: i64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "status": "BLOCKED",
            "from": from,
            "count": count,
        });
        self.send_and_wait(36, payload, 0).await
    }

    pub async fn add_to_blacklist(
        &self,
        contact_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactId": contact_id,
            "action": "BLOCK",
        });
        self.send_and_wait(34, payload, 0).await
    }

    pub async fn remove_from_blacklist(
        &self,
        contact_id: u64,
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactId": contact_id,
            "action": "UNBLOCK",
        });
        self.send_and_wait(34, payload, 0).await
    }

    /// Syncs a batch of phonebook entries with the server (opcode 21).
    pub async fn upload_phonebook_contacts(
        &self,
        contact_list: std::collections::HashMap<String, serde_json::Value>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "contactList": contact_list,
        });
        self.send_and_wait(21, payload, 0).await
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::{errors::Error, WriteCommand};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn updating_contact_name_sends_update_action_and_both_name_fields() {
        let client = MaxClient::new();
        let (writer, mut commands) = mpsc::channel::<WriteCommand>(1);
        {
            let mut state = client.state.lock().await;
            state.is_closed = false;
            state.writer_tx = Some(writer);
        }

        for (last_name, expected) in [
            (
                Some("Example"),
                json!({
                    "contactId": 42,
                    "firstName": "Alex",
                    "lastName": "Example",
                    "action": "UPDATE",
                }),
            ),
            (
                None,
                json!({
                    "contactId": 42,
                    "firstName": "Alex",
                    "action": "UPDATE",
                }),
            ),
            (
                Some(""),
                json!({
                    "contactId": 42,
                    "firstName": "Alex",
                    "action": "UPDATE",
                }),
            ),
        ] {
            let (result, request) = tokio::join!(
                client.update_contact_name(42, "Alex", last_name),
                async {
                    let command = commands.recv().await.unwrap();
                    command.done.send(Err(Error::NotConnected)).unwrap();
                    command.request
                },
            );
            assert!(matches!(result, Err(Error::NotConnected)));
            assert_eq!(request.opcode, 34);
            assert_eq!(request.payload, expected);
        }
    }
}
