use crate::{errors::{ClientResult, Error}, MaxClient};
use crate::models::Response;
use crate::fingerprint::FingerprintGenerator;
use chrono::Utc;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
    pub chats_sync: i64,
    pub contacts_sync: i64,
    pub drafts_sync: i64,
    pub presence_sync: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<serde_json::Value>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            chats_sync: -1,
            contacts_sync: -1,
            drafts_sync: -1,
            presence_sync: -1,
            config_hash: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Login2Flags {
    #[serde(default)]
    pub config_enabled: bool,
    #[serde(default)]
    pub contact_enabled: bool,
    #[serde(default)]
    pub profile_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrLoginSession {
    pub qr_link: String,
    pub track_id: String,
    #[serde(default = "default_qr_polling_interval")]
    pub polling_interval: u64,
    #[serde(default)]
    pub ttl: i64,
    #[serde(default)]
    pub expires_at: i64,
}

fn default_qr_polling_interval() -> u64 {
    5000
}

impl MaxClient {
    async fn request_code_internal(
        &self,
        phone: String,
        auth_type: &str,
        language: Option<&str>,
    ) -> ClientResult<Response> {
        let (identity, calls_seed, version_provider, is_connected) = {
            let state = self.state.lock().await;
            (
                state.identity.clone(),
                state.calls_seed,
                state.version_provider.clone(),
                state.writer.is_some(),
            )
        };

        if !is_connected || identity.is_none() {
            error!("Auth request failed: no handshake response available");
            return Err(Error::ConnectionFailed(
                "No handshake response available for request_code".into(),
            ));
        }

        let identity = identity.unwrap();
        let is_web = identity.user_agent.device_type.eq_ignore_ascii_case("web");

        let mode = if !is_web {
            if let Some(seed) = calls_seed {
                let app_ver = if identity.user_agent.app_version.is_empty() {
                    "2.25.0"
                } else {
                    &identity.user_agent.app_version
                };

                if let Some(version_data) = version_provider.get_version(app_ver).await {
                    let arch = identity.user_agent.arch.as_deref().unwrap_or("arm64-v8a");
                    FingerprintGenerator::new(version_data)
                        .generate_fingerprint(&identity.device_id, seed, Some(arch))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut payload = json!({
            "phone": phone,
            "type": auth_type,
            "language": language.unwrap_or("ru"),
        });

        if let Some(fp_bytes) = mode {
            payload["mode"] = json!(fp_bytes);
        }

        let resp = self.send_and_wait(17, payload, 0).await?;

        if let Some(map) = resp.payload.as_object() {
            debug!("Auth code request accepted: keys={:?}", map.keys().collect::<Vec<_>>());
        }

        if let Some(token) = resp.payload.get("token").and_then(|t| t.as_str()) {
            info!("Received temp token: {}", token);
            self.set_temp_token(token.to_string()).await;
        }

        Ok(resp)
    }

    pub async fn start_auth(&self, phone: String) -> ClientResult<Response> {
        info!("Requesting auth code: phone_set={}", !phone.is_empty());
        self.request_code_internal(phone, "START_AUTH", Some("ru")).await
    }

    pub async fn resend_auth(&self, phone: String) -> ClientResult<Response> {
        info!("Resending auth code: phone_set={}", !phone.is_empty());
        self.request_code_internal(phone, "RESEND", Some("ru")).await
    }
    
    pub async fn check_code(&self, code: String) -> ClientResult<Response> {
        let state = self.state.lock().await;
        let token = state.temp_token.as_ref().ok_or("No temporary token found".to_string())?;
        
        let payload = json!({ "token": token, "verifyCode": code, "authTokenType": "CHECK_CODE" });
        
        drop(state);
        
        let resp = self.send_and_wait(18, payload, 0).await?;
        
        log::debug!("check_code response {:?}", resp);
        
        if let Some(token_attrs) = resp.payload.get("tokenAttrs").and_then(|t| t.as_object()) {
            for (token_type, value) in token_attrs {
                if let Some(token) = value.get("token").and_then(|t| t.as_str()) {
                    match token_type.as_str() {
                        "REGISTER" => {
                            self.set_temp_token(token.to_string()).await;
                        }
                        "LOGIN" => {
                            self.set_token(token.to_string()).await;
                        }
                        _ => {
                            eprintln!("Unknown token type: {}", token_type);
                        }
                    }
                }
            }
        }
        
        Ok(resp)
    }

    pub async fn check_password(&self, password: String, track_id: String) -> ClientResult<Response> {
        let payload = json!({
            "password": password,
            "trackId": track_id,
        });

        let resp = self.send_and_wait(115, payload, 0).await?;

        log::debug!("check_password response {:?}", resp);

        if let Some(token_attrs) = resp.payload.get("tokenAttrs").and_then(|t| t.as_object()) {
            for (token_type, value) in token_attrs {
                if let Some(token) = value.get("token").and_then(|t| t.as_str()) {
                    match token_type.as_str() {
                        "LOGIN" => {
                            self.set_token(token.to_string()).await;
                        }
                        _ => {
                            eprintln!("Unknown token type in check_password: {}", token_type);
                        }
                    }
                }
            }
        }

        Ok(resp)
    }

    pub async fn submit_register_with_avatar(
        &self,
        first_name: String,
        last_name: Option<String>,
        photo_id: Option<i64>,
        avatar_type: Option<String>,
    ) -> ClientResult<Response> {
        let reg_token = {
            let state = self.state.lock().await;
            state.temp_token.clone()
        };

        let mut payload = json!({
            "firstName": first_name,
            "tokenType": "REGISTER",
        });

        if let Some(pid) = photo_id {
            payload["photoId"] = json!(pid);
            payload["avatarType"] = json!(avatar_type.unwrap_or_else(|| "PRESET_AVATAR".to_string()));
        }

        if let Some(ref token) = reg_token {
            payload["token"] = json!(token);
        }

        if let Some(ref last) = last_name {
            payload["lastName"] = json!(last);
        }

        let mut resp = self.send_and_wait(23, payload, 0).await?;

        let final_token = resp
            .payload
            .get("token")
            .and_then(|t| t.as_str().map(|s| s.to_string()))
            .or(reg_token);

        if let Some(ref token) = final_token {
            self.set_token(token.clone()).await;
            if resp.payload.get("token").is_none() {
                if let Some(obj) = resp.payload.as_object_mut() {
                    obj.insert("token".to_string(), json!(token));
                }
            }
        }

        Ok(resp)
    }

    pub async fn submit_register(
        &self,
        first_name: String,
        last_name: Option<String>,
    ) -> ClientResult<Response> {
        self.submit_register_with_avatar(first_name, last_name, None, None).await
    }

    pub async fn check_call_status(&self, track_id: String) -> ClientResult<Response> {
        let resp = self.send_and_wait(294, json!({ "trackId": track_id }), 0).await?;
        self.update_tokens_from_payload(&resp.payload).await;
        Ok(resp)
    }

    pub async fn auth_by_track(&self, track_id: String) -> ClientResult<Response> {
        let resp = self.send_and_wait(291, json!({ "trackId": track_id }), 0).await?;
        self.update_tokens_from_payload(&resp.payload).await;
        Ok(resp)
    }

    async fn update_tokens_from_payload(&self, payload: &Value) {
        if let Some(token_attrs) = payload.get("tokenAttrs").and_then(|t| t.as_object()) {
            for (token_type, value) in token_attrs {
                if let Some(token) = value.get("token").and_then(|t| t.as_str()) {
                    match token_type.as_str() {
                        "REGISTER" => {
                            self.set_temp_token(token.to_string()).await;
                        }
                        "LOGIN" => {
                            self.set_token(token.to_string()).await;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    
    pub async fn sync(
        &self,
        sync_state: Option<SyncState>,
    ) -> ClientResult<(Response, Option<Response>, SyncState)> {
        const DEFAULT_CONFIG_HASH: &str = "00000000-0000000000000000-00000000-0000000000000000-0000000000000000-0-0000000000000000-00000000";

        let mut sync_state = sync_state.unwrap_or_default();

        let (token, identity, calls_seed, version_provider) = {
            let state = self.state.lock().await;
            (
                state.token.clone().ok_or_else(|| Error::ConnectionFailed("No token set".into()))?,
             state.identity.clone().ok_or_else(|| Error::ConnectionFailed("No identity set".into()))?,
             state.calls_seed,
             state.version_provider.clone(),
            )
        };

        let is_web = identity.user_agent.device_type.eq_ignore_ascii_case("web");
        let chat_cache_fingerprint = if !is_web {
            if let Some(seed) = calls_seed {
                let app_ver = if identity.user_agent.app_version.is_empty() {
                    "2.25.0"
                } else {
                    &identity.user_agent.app_version
                };

                if let Some(version_data) = version_provider.get_version(app_ver).await {
                    let arch = identity.user_agent.arch.as_deref().unwrap_or("arm64-v8a");
                    FingerprintGenerator::new(version_data)
                        .generate_fingerprint(&identity.device_id, seed, Some(arch))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let payload = if is_web {
            json!({
                "token": token,
                "chatsCount": 40,
                "interactive": true,
                "chatsSync": sync_state.chats_sync,
                "contactsSync": sync_state.contacts_sync,
                "presenceSync": sync_state.presence_sync,
                "draftsSync": sync_state.drafts_sync,
            })
        } else {
            let mut p = json!({
                "userAgent": identity.user_agent,
                "interactive": true,
                "token": token,
                "chatsSync": sync_state.chats_sync,
                "contactsSync": sync_state.contacts_sync,
                "presenceSync": sync_state.presence_sync,
                "draftsSync": sync_state.drafts_sync,
                "exp": {
                    "chatsCountGroups": vec![0x0a, 0x32]
                }
            });

            if let Some(fp) = chat_cache_fingerprint {
                p["chatCacheFingerprint"] = json!(fp);
            }

            if let Some(hash) = &sync_state.config_hash {
                p["configHash"] = hash.clone();
            } else {
                p["configHash"] = json!(DEFAULT_CONFIG_HASH);
            }

            p
        };

        let login_response = self.send_and_wait(19, payload, 0).await?;

        if let Some(new_token) = login_response.payload.get("token").and_then(|t| t.as_str()) {
            if new_token != token {
                self.set_token(new_token.to_string()).await;
            }
        }

        if let Some(time) = login_response.payload.get("time").and_then(|t| t.as_i64()) {
            sync_state.chats_sync = time;
            sync_state.contacts_sync = time;
            sync_state.drafts_sync = time;
            sync_state.presence_sync = time;
        }

        if let Some(config) = login_response.payload.get("config") {
            if let Some(hash) = config.get("hash") {
                sync_state.config_hash = Some(hash.clone());
            }
        }

        let mut login2_response_opt = None;

        if !is_web {
            if let Some(flags_val) = login_response.payload.get("login2Flags") {
                let flags: Login2Flags =
                    serde_json::from_value(flags_val.clone()).unwrap_or_default();

                if flags.config_enabled || flags.contact_enabled || flags.profile_enabled {
                    let login2_payload = json!({
                        "needProfile": flags.profile_enabled,
                        "contactsSync": if flags.contact_enabled { sync_state.contacts_sync } else { -1 },
                        "configHash": sync_state.config_hash,
                    });

                    let login2_response = self.send_and_wait(8, login2_payload, 0).await?;

                    if let Some(config) = login2_response.payload.get("config") {
                        if let Some(hash) = config.get("hash") {
                            sync_state.config_hash = Some(hash.clone());
                        }
                    }

                    login2_response_opt = Some(login2_response);
                }
            }
        }

        Ok((login_response, login2_response_opt, sync_state))
    }

    fn extract_login_token(payload: &Value) -> Option<String> {
        if let Some(token) = payload
            .get("tokenAttrs")
            .and_then(|attrs| attrs.get("LOGIN"))
            .and_then(|login| login.get("token"))
            .and_then(|t| t.as_str())
        {
            return Some(token.to_string());
        }
        payload
            .get("token")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    }

    pub async fn qr_login_request(&self) -> ClientResult<QrLoginSession> {
        let resp = self.send_and_wait(288, json!({}), 0).await?;
        let session: QrLoginSession = serde_json::from_value(resp.payload.clone()).map_err(|e| {
            Error::Other(format!("qr_login_request: failed to parse response: {}", e))
        })?;
        info!(
            "QR: request ok (trackId={}, pollInterval={}ms, ttl={}ms, expiresAt={})",
            session.track_id, session.polling_interval, session.ttl, session.expires_at
        );
        Ok(session)
    }

    pub async fn qr_login_poll(&self, track_id: &str) -> ClientResult<Response> {
        self.send_and_wait(289, json!({ "trackId": track_id }), 0)
            .await
    }

    pub async fn qr_login_wait(&self, session: &QrLoginSession) -> ClientResult<String> {
        let interval_ms = if session.polling_interval == 0 {
            5000
        } else if session.polling_interval < 100 {
            session.polling_interval * 1000
        } else {
            session.polling_interval
        }
        .max(1000);

        let now_ms = Utc::now().timestamp_millis();
        let deadline_ms = if session.expires_at > 0 {
            if session.expires_at < 100_000_000_000 {
                session.expires_at * 1000
            } else {
                session.expires_at
            }
        } else if session.ttl > 0 {
            let ttl_ms = if session.ttl < 100_000 {
                session.ttl * 1000
            } else {
                session.ttl
            };
            now_ms + ttl_ms
        } else {
            now_ms + 120_000
        };
        let total_wait = (deadline_ms - now_ms).max(1000) as u64;

        let mut attempt: u64 = 0;
        info!(
            "QR: waiting for scan (trackId={}, pollInterval={}ms, expiresAt={})",
            session.track_id, interval_ms, deadline_ms
        );

        loop {
            if Utc::now().timestamp_millis() >= deadline_ms {
                warn!("QR: timed out after {}ms without confirmation", total_wait);
                return Err(Error::RequestTimeout(Duration::from_millis(total_wait)));
            }

            attempt += 1;
            match self.qr_login_poll(&session.track_id).await {
                Ok(resp) => {
                    debug!(
                        "QR poll #{}: cmd={} opcode={}",
                        attempt, resp.cmd, resp.opcode
                    );

                    if let Some(token) = Self::extract_login_token(&resp.payload) {
                        self.set_token(token.clone()).await;
                        info!(
                            "QR: authorization succeeded (token in poll response), poll #{}",
                            attempt
                        );
                        return Ok(token);
                    }

                    let login_available = resp
                        .payload
                        .get("status")
                        .and_then(|s| s.get("loginAvailable"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if login_available {
                        info!(
                            "QR poll #{}: scan confirmed (loginAvailable=true), completing login via LOGIN_BY_QR (291)",
                            attempt
                        );
                        return self.qr_login_submit(&session.track_id).await;
                    }

                    debug!("QR poll #{}: still pending", attempt);
                }
                Err(Error::ConnectionClosed) => {
                    warn!("QR poll #{}: connection closed", attempt);
                    return Err(Error::ConnectionClosed);
                }
                Err(Error::NotConnected) => {
                    warn!("QR poll #{}: not connected", attempt);
                    return Err(Error::NotConnected);
                }
                Err(Error::ApiResponse(value)) => {
                    warn!("QR poll #{}: api error: {}", attempt, value);
                    return Err(Error::ApiResponse(value));
                }
                Err(e) => {
                    warn!("QR poll #{}: request failed: {}", attempt, e);
                }
            }

            let remaining = deadline_ms - Utc::now().timestamp_millis();
            if remaining <= 0 {
                continue;
            }
            tokio::time::sleep(Duration::from_millis(interval_ms.min(remaining as u64))).await;
        }
    }

    pub async fn qr_login_submit(&self, track_id: &str) -> ClientResult<String> {
        let resp = self
            .send_and_wait(291, json!({ "trackId": track_id }), 0)
            .await?;

        debug!("QR LOGIN_BY_QR: cmd={} opcode={}", resp.cmd, resp.opcode);

        if let Some(token) = Self::extract_login_token(&resp.payload) {
            self.set_token(token.clone()).await;
            info!("QR: authorization succeeded (LOGIN_BY_QR token)");
            return Ok(token);
        }

        if let Some(challenge) = resp.payload.get("passwordChallenge") {
            warn!(
                "QR: 2FA password required; call check_password with challenge trackId: {:?}",
                challenge.get("trackId")
            );
            return Err(Error::ApiResponse(resp.payload));
        }

        warn!("QR: no login token in LOGIN_BY_QR response");
        Err(Error::ApiResponse(resp.payload))
    }

    pub async fn qr_login_confirm(
        &self,
        session: &QrLoginSession,
    ) -> ClientResult<(Response, SyncState)> {
        let _token = self.qr_login_wait(session).await?;
        let (login, _login2, state) = self.sync(None).await?;
        Ok((login, state))
    }
}
