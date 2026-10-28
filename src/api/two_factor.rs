use crate::{errors::ClientResult, MaxClient};
use crate::models::Response;
use serde_json::{json, Value};

trait ValueExt {
    fn str(&self, key: &str) -> Option<&str>;
    fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str;
}

impl ValueExt for Value {
    fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.str(key).unwrap_or(default)
    }
}

impl MaxClient {
    pub async fn create_2fa_track(&self) -> ClientResult<Response> {
        let payload = json!({
            "type": 0,
        });

        self.send_and_wait(112, payload, 0).await
    }

    pub async fn validate_2fa_password(
        &self,
        track_id: &str,
        password: &str,
    ) -> ClientResult<Response> {
        let payload = json!({
            "trackId": track_id,
            "password": password,
        });

        self.send_and_wait(107, payload, 0).await
    }

    pub async fn validate_2fa_hint(
        &self,
        track_id: &str,
        hint: &str,
    ) -> ClientResult<Response> {
        let payload = json!({
            "trackId": track_id,
            "hint": hint,
        });

        self.send_and_wait(108, payload, 0).await
    }

    pub async fn request_2fa_email(
        &self,
        track_id: &str,
        email: &str,
    ) -> ClientResult<Response> {
        let payload = json!({
            "trackId": track_id,
            "email": email,
        });

        self.send_and_wait(109, payload, 0).await
    }

    pub async fn verify_2fa_email(
        &self,
        track_id: &str,
        code: &str,
    ) -> ClientResult<Response> {
        let payload = json!({
            "trackId": track_id,
            "verifyCode": code,
        });

        self.send_and_wait(110, payload, 0).await
    }

    pub async fn get_2fa_status(&self, track_id: &str) -> ClientResult<Response> {
        let payload = json!({
            "trackId": track_id,
        });

        self.send_and_wait(104, payload, 0).await
    }

    pub async fn verify_2fa_current_password(
        &self,
        track_id: &str,
        password: &str,
    ) -> ClientResult<Response> {
        let payload = json!({
            "trackId": track_id,
            "password": password,
        });

        self.send_and_wait(113, payload, 0).await
    }

    pub async fn apply_2fa_config(&self, payload: Value) -> ClientResult<Response> {
        self.send_and_wait(111, payload, 0).await
    }

    pub async fn fetch_2fa_status(&self) -> ClientResult<Response> {
        let track_resp = self.create_2fa_track().await?;
        let track_id = track_resp.payload.str_or("trackId", "").to_string();

        let mut status_resp = self.get_2fa_status(&track_id).await?;
        if let Some(obj) = status_resp.payload.as_object_mut() {
            obj.insert("trackId".to_string(), json!(track_id));
        }

        Ok(status_resp)
    }

    pub async fn request_2fa_email_code(
        &self,
        track_id: Option<String>,
        email: &str,
    ) -> ClientResult<Response> {
        let track = match track_id {
            Some(t) => t,
            None => {
                let track_resp = self.create_2fa_track().await?;
                track_resp.payload.str_or("trackId", "").to_string()
            }
        };

        let mut resp = self.request_2fa_email(&track, email).await?;
        if let Some(obj) = resp.payload.as_object_mut() {
            obj.insert("trackId".to_string(), json!(track));
        }

        Ok(resp)
    }

    pub async fn setup_2fa(
        &self,
        password: &str,
        hint: Option<&str>,
        email: Option<&str>,
        code: Option<&str>,
        track_id: Option<String>,
    ) -> ClientResult<Response> {
        let track = match track_id {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                let track_resp = self.create_2fa_track().await?;
                track_resp.payload.str_or("trackId", "").to_string()
            }
        };

        let val_resp = self.validate_2fa_password(&track, password).await?;
        if val_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(val_resp.payload));
        }

        let mut caps = vec![0];

        if let Some(h) = hint {
            let trimmed = h.trim();
            if !trimmed.is_empty() {
                let hint_resp = self.validate_2fa_hint(&track, trimmed).await?;
                if hint_resp.payload.get("error").is_some() {
                    return Err(crate::Error::ApiResponse(hint_resp.payload));
                }
                caps.push(3);
            }
        }

        if let (Some(e), Some(c)) = (email, code) {
            let email_trimmed = e.trim();
            let code_trimmed = c.trim();
            if !email_trimmed.is_empty() && !code_trimmed.is_empty() {
                let code_resp = self.verify_2fa_email(&track, code_trimmed).await?;
                if code_resp.payload.get("error").is_some() {
                    return Err(crate::Error::ApiResponse(code_resp.payload));
                }
                caps.push(4);
            }
        }

        let mut payload = json!({
            "trackId": track,
            "expectedCapabilities": caps,
            "password": password,
        });

        if let Some(h) = hint {
            let trimmed = h.trim();
            if !trimmed.is_empty() {
                payload["hint"] = json!(trimmed);
            }
        }

        let apply_resp = self.apply_2fa_config(payload).await?;
        if apply_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(apply_resp.payload));
        }

        Ok(apply_resp)
    }

    pub async fn change_2fa(
        &self,
        current_password: &str,
        new_password: &str,
        new_hint: Option<&str>,
    ) -> ClientResult<Response> {
        let track_resp = self.create_2fa_track().await?;
        let track = track_resp.payload.str_or("trackId", "").to_string();

        let check_resp = self.verify_2fa_current_password(&track, current_password).await?;
        if check_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(check_resp.payload));
        }

        let val_resp = self.validate_2fa_password(&track, new_password).await?;
        if val_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(val_resp.payload));
        }

        let mut caps = vec![1];

        if let Some(h) = new_hint {
            let trimmed = h.trim();
            if !trimmed.is_empty() {
                let hint_resp = self.validate_2fa_hint(&track, trimmed).await?;
                if hint_resp.payload.get("error").is_some() {
                    return Err(crate::Error::ApiResponse(hint_resp.payload));
                }
                caps.push(3);
            }
        }

        let mut payload = json!({
            "trackId": track,
            "expectedCapabilities": caps,
            "password": new_password,
        });

        if let Some(h) = new_hint {
            let trimmed = h.trim();
            if !trimmed.is_empty() {
                payload["hint"] = json!(trimmed);
            }
        }

        let apply_resp = self.apply_2fa_config(payload).await?;
        if apply_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(apply_resp.payload));
        }

        Ok(apply_resp)
    }

    pub async fn disable_2fa(&self, current_password: &str) -> ClientResult<Response> {
        let track_resp = self.create_2fa_track().await?;
        let track = track_resp.payload.str_or("trackId", "").to_string();

        let check_resp = self.verify_2fa_current_password(&track, current_password).await?;
        if check_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(check_resp.payload));
        }

        let payload = json!({
            "trackId": track,
            "expectedCapabilities": [5],
            "remove2fa": true,
        });

        let apply_resp = self.apply_2fa_config(payload).await?;
        if apply_resp.payload.get("error").is_some() {
            return Err(crate::Error::ApiResponse(apply_resp.payload));
        }

        Ok(apply_resp)
    }
}
