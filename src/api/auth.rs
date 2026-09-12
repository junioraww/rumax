use crate::{errors::{ClientResult, Error}, MaxClient};
use crate::models::{Response};
use crate::fingerprint::FingerprintGenerator;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde::Serialize;

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

/// Флаги для необходимости вызова Login2
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

impl MaxClient {
    /**
     * Начало логина
     */
    pub async fn start_auth(&self, phone: String) -> ClientResult<Response> {
        info!("Запрос SMS-кода: phone_set={}", !phone.is_empty());

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
            error!("Запрос start_auth вызван без установленного соединения / handshake");
            return Err(Error::ConnectionFailed(
                "No handshake response available for request_code".into(),
            ));
        }

        let identity = identity.unwrap();
        let is_web = identity.user_agent.device_type.eq_ignore_ascii_case("web");

        let mode = if !is_web {
            let seed = calls_seed.ok_or_else(|| {
                error!("handshake_response.calls_seed отсутствует при device_type != WEB");
                Error::ConnectionFailed("handshake_response.calls_seed is missing".into())
            })?;

            let app_ver = if identity.user_agent.app_version.is_empty() {
                "2.25.0"
            } else {
                &identity.user_agent.app_version
            };

            let version_data = version_provider.get_version(app_ver).await.ok_or_else(|| {
                error!("Версия {} не найдена", app_ver);
                Error::ConnectionFailed("fingerprint_generator is missing and DeviceType != WEB".into())
            })?;

            let arch = identity.user_agent.arch.as_deref().unwrap_or("arm64-v8a");
            debug!("Генерация отпечатка: device_id={}, arch={}, seed={}", identity.device_id, arch, seed);

            let fp = FingerprintGenerator::new(version_data)
            .generate_fingerprint(&identity.device_id, seed, Some(arch));

            if let Some(ref bytes) = fp {
                debug!("Отпечаток сгенерирован (размер: {}): {}", bytes.len(), hex::encode(bytes));
            } else {
                warn!("Не удалось сгенерировать отпечаток для arch={}", arch);
            }

            fp
        } else {
            None
        };

        let mut payload = json!({
            "phone": phone,
            "type": "START_AUTH"
        });

        if let Some(fp_bytes) = mode {
            payload["mode"] = json!(fp_bytes);
        }

        let resp = self.send_and_wait(17, payload, 0).await?;
        // ----------------------------------------

        if let Some(map) = resp.payload.as_object() {
            debug!("sms code request accepted payload_keys={:?}", map.keys().collect::<Vec<_>>());
        }

        if let Some(token) = resp.payload.get("token").and_then(|t| t.as_str()) {
            info!("Получен temp token: {}", token);
            self.set_temp_token(token.to_string()).await;
        }

        Ok(resp)
    }
    
    /**
     * Завершение логина
     */
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

    /**
     * Проверка облачного пароля (2FA)
     */
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

    /**
     * Регистрация
     */
    pub async fn submit_register(
        &self,
        first_name: String,
        last_name: Option<String>,
    ) -> ClientResult<Response> {
        let payload = json!({
            "firstName": first_name,
            "lastName": last_name,
            "photoId": 2981369,
            "avatarType": "PRESET_AVATAR",
            "tokenType": "REGISTER",
        });
        
        let resp = self.send_and_wait(23, payload, 0).await?;
        
        if let Some(token) = resp
            .payload
            .get("token")
            .and_then(|t| t.as_str())
        {
            log::info!("Token received! {:?}", token.to_string());
            self.set_token(token.to_string()).await;
        }
        
        Ok(resp)
    }
    
    /**
     * Перезаход в мессенджер
     * TODO стоит переименовать методы как в pymax (sync -> login, start_auth -> start?)
     */
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
            let seed = calls_seed.ok_or_else(|| {
                Error::ConnectionFailed("handshake_response.calls_seed is missing".into())
            })?;

            let app_ver = if identity.user_agent.app_version.is_empty() {
                "2.25.0"
            } else {
                &identity.user_agent.app_version
            };

            let version_data = version_provider.get_version(app_ver).await.ok_or_else(|| {
                Error::ConnectionFailed(format!("Версия {} не найдена", app_ver))
            })?;

            let arch = identity.user_agent.arch.as_deref().unwrap_or("arm64-v8a");

            FingerprintGenerator::new(version_data)
            .generate_fingerprint(&identity.device_id, seed, Some(arch))
        } else {
            None
        };

        let mut payload = json!({
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
            payload["chatCacheFingerprint"] = json!(fp);
        }

        if let Some(hash) = &sync_state.config_hash {
            payload["configHash"] = hash.clone();
        } else {
            payload["configHash"] = json!(DEFAULT_CONFIG_HASH);
        }

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

        if let Some(flags_val) = login_response.payload.get("login2Flags") {
            let flags: Login2Flags = serde_json::from_value(flags_val.clone()).unwrap_or_default();

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

        Ok((login_response, login2_response_opt, sync_state))
    }
}
