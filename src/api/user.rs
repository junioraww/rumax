use crate::{errors::ClientResult, MaxClient};
use crate::models::Response;
use serde_json::{json, Map, Value};

impl MaxClient {
    pub async fn logout(&self) -> ClientResult<Response> {
        let resp = self.send_and_wait(20, json!({}), 0).await;
        self.disconnect().await;
        resp
    }

    /*
     * Список сессий
     */
    pub async fn get_sessions(&self) -> ClientResult<Response> {
        self.send_and_wait(96, json!({}), 0).await
    }

    /*
     * Закрыть все сессии, кроме текущей
     */
    pub async fn close_all_sessions(&self) -> ClientResult<Response> {
        self.send_and_wait(97, json!({}), 0).await
    }

    /*
     * Обновить профиль
     */
    pub async fn update_profile(
        &self,
        first_name: String,
        last_name: String,
        description: Option<String>,
        avatar_token: Option<String>,
    ) -> ClientResult<Response> {
        let mut payload = Map::new();

        payload.insert("firstName".into(), json!(first_name));
        payload.insert("lastName".into(), json!(last_name));

        if let Some(d) = description {
            payload.insert("description".into(), json!(d));
        }

        if let Some(t) = avatar_token {
            payload.insert("photoToken".into(), json!(t));
            payload.insert("avatarType".into(), json!("USER_AVATAR"));
        }

        self.send_and_wait(16, Value::Object(payload), 0).await
    }

    pub async fn set_profile_photo(
        &self,
        photo_token: String,
    ) -> ClientResult<Response> {
        let payload = json!({
            "photoToken": photo_token,
            "avatarType": "USER_AVATAR",
        });
        self.send_and_wait(16, payload, 0).await
    }
}
