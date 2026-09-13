use crate::{
    navigation::{self, Screen},
    MaxClient,
};
use chrono::Utc;
use log::{debug, error, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

const STARTUP_DELAY: (u64, u64) = (15, 90);
const SESSION_IDLE_DELAY: (u64, u64) = (900, 2700);
const RETURN_DELAY: (u64, u64) = (12, 45);
const RENDER_DELAY: (f64, f64) = (0.15, 1.2);

const RETURN_TO_BACKGROUND_CHANCE: f64 = 0.40;
const OPEN_CHATS_RENDER_CHANCE: f64 = 0.20;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryChat {
    pub chat_id: i64,
    pub chat_type: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TelemetryEvent {
    time: i64,
    user_id: u64,
    r#type: String,
    event: String,
    params: serde_json::Value,
    session_id: i64,
}

#[derive(Serialize, Debug)]
struct TelemetryPayload {
    events: Vec<TelemetryEvent>,
}

impl MaxClient {
    pub async fn set_telemetry_chats(&self, chats: Vec<TelemetryChat>) {
        let mut state = self.state.lock().await;
        state.telemetry_chats = chats;
        debug!("Телеметрия: загружено {} чатов", state.telemetry_chats.len());
    }

    async fn sleep_between(&self, range: (u64, u64)) {
        let secs = rand::thread_rng().gen_range(range.0..=range.1);
        sleep(Duration::from_secs(secs)).await;
    }

    async fn sleep_between_f64(&self, range: (f64, f64)) {
        let secs = rand::thread_rng().gen_range(range.0..=range.1);
        sleep(Duration::from_secs_f64(secs)).await;
    }

    fn now_ms() -> i64 {
        Utc::now().timestamp_millis()
    }

    async fn send_events(&self, events: Vec<TelemetryEvent>) {
        if events.is_empty() {
            return;
        }

        let count = events.len();
        let payload = TelemetryPayload { events };

        let payload_json = match serde_json::to_value(&payload) {
            Ok(val) => val,
            Err(e) => {
                error!("Ошибка сериализации TelemetryPayload: {}", e);
                return;
            }
        };

        match self.send_and_wait(5, payload_json, 0).await {
            Ok(data) => {
                if let Some(err) = data.payload.get("error") {
                    error!("API телеметрии вернуло ошибку: {}", err);
                } else {
                    debug!("Успешно отправлено {} событий телеметрии", count);
                }
            }
            Err(e) => warn!("Ошибка отправки телеметрии: {}", e),
        }
    }

    fn build_login_event(&self, user_id: u64, session_id: i64) -> TelemetryEvent {
        TelemetryEvent {
            time: Self::now_ms(),
            user_id,
            r#type: "PERF".to_string(),
            event: "login".to_string(),
            session_id,
            params: json!({
                "properties": {
                    "connection_type": 2,
                    "vpn": 0,
                    "class": 2,
                    "background": 1,
                    "warm_start": 1,
                },
                "errorType": 100,
            }),
        }
    }

    fn get_source_params(&self, screen_to: Screen, user_id: u64, chats: &[TelemetryChat]) -> serde_json::Value {
        if screen_to == Screen::Chats {
            return json!({ "source_type": 5, "source_id": 1, "tab_config": 2 });
        }

        if screen_to != Screen::Chat {
            return json!({});
        }

        if chats.is_empty() {
            return json!({
                "source_type": 1,
                "source_id": user_id,
            });
        }

        let mut rng = rand::thread_rng();
        let chat = &chats[rng.gen_range(0..chats.len())];

        let source_type = if chat.chat_type == "ChatType.DIALOG" || chat.chat_type == "DIALOG" {
            1
        } else {
            2
        };

        json!({
            "source_type": source_type,
            "source_id": chat.chat_id,
        })
    }

    fn build_nav_event(
        &self, user_id: u64, session_id: i64,
        screen_from: Screen, screen_to: Screen,
        prev_time: i64, action_id: u64, chats: &[TelemetryChat]
    ) -> TelemetryEvent {

        let mut params = json!({
            "prev_time": prev_time,
            "screen_to": screen_to.id(),
            "action_id": action_id,
            "screen_from": screen_from.id(),
        });

        let extra_params = self.get_source_params(screen_to, user_id, chats);
        if let (serde_json::Value::Object(main_map), serde_json::Value::Object(extra_map)) = (&mut params, extra_params) {
            for (k, v) in extra_map {
                main_map.insert(k, v);
            }
        }

        TelemetryEvent {
            time: Self::now_ms(),
            user_id,
            r#type: "NAV".to_string(),
            event: "GO".to_string(),
            session_id,
            params,
        }
    }

    fn build_open_chat_event(&self, user_id: u64, session_id: i64) -> TelemetryEvent {
        let mut rng = rand::thread_rng();
        let messages = rng.gen_range(60..=240);
        let render = rng.gen_range(50..=260);
        let duration = messages + render;

        TelemetryEvent {
            time: Self::now_ms(),
            user_id,
            r#type: "PERF".to_string(),
            event: "open_chat_to_render".to_string(),
            session_id,
            params: json!({
                "spans": [
                    { "duration": duration, "name": "open_chat_to_render" },
                    { "duration": messages, "name": "messages_list_created" },
                    { "duration": render, "name": "messages_render" },
                ],
                "properties": { "class": 2, "warm": 1, "flow": 1 }
            }),
        }
    }

    fn build_open_chats_event(&self, user_id: u64, session_id: i64) -> TelemetryEvent {
        let mut rng = rand::thread_rng();
        let created = rng.gen_range(50..=230);
        let rendered = rng.gen_range(180..=650);
        let duration = created + rendered;

        TelemetryEvent {
            time: Self::now_ms(),
            user_id,
            r#type: "PERF".to_string(),
            event: "open_chats_to_render".to_string(),
            session_id,
            params: json!({
                "spans": [
                    { "duration": duration, "name": "open_chats_to_render" },
                    { "duration": created, "name": "chats_tab_created" },
                    { "duration": rendered, "name": "chat_list_render" },
                ],
                "properties": { "class": 2 }
            }),
        }
    }

    pub async fn spawn_telemetry_task(&self) {
        let client = self.clone();

        let mut shutdown_rx = match self.state.lock().await.shutdown_tx.as_ref() {
            Some(tx) => tx.subscribe(),
            None => {
                error!("Не могу запустить телеметрию: shutdown_tx не инициализирован");
                return;
            }
        };

        tokio::spawn(async move {
            info!("Задача телеметрии ожидает подключения...");

            loop {
                let has_user_id = client.state.lock().await.user_id.is_some();
                if client.is_connected().await && has_user_id {
                    break;
                }
                tokio::select! {
                    _ = sleep(Duration::from_secs(1)) => {},
                     _ = shutdown_rx.recv() => return,
                }
            }

            info!("Телеметрия: старт (ожидание startup_delay)");

            tokio::select! {
                _ = client.sleep_between(STARTUP_DELAY) => {},
                     _ = shutdown_rx.recv() => return,
            }

            let (user_id, mut session_id) = {
                let state = client.state.lock().await;
                (state.user_id.unwrap_or(0), state.session_id)
            };

            client.send_events(vec![client.build_login_event(user_id, session_id)]).await;
            let mut last_nav_time = Self::now_ms();

            loop {
                session_id += 1;
                {
                    let mut state = client.state.lock().await;
                    state.session_id = session_id;
                }

                let profile = navigation::get_random_profile();
                let mut session_events = Vec::new();
                let mut is_aborted = false;

                for _ in 0..profile.steps {
                    tokio::select! {
                        _ = sleep(profile.get_pause_duration()) => {},
                     _ = shutdown_rx.recv() => {
                         is_aborted = true;
                         break;
                     }
                    }

                    if !client.is_connected().await {
                        warn!("Клиент отключен. Остановка телеметрии.");
                        return;
                    }

                    let (screen_from, screen_to, action_id, chats) = {
                        let mut state = client.state.lock().await;
                        let from = state.planner.current_screen;
                        let to = state.planner.next_screen(&profile);

                        state.action_id = state.action_id.wrapping_add(1) % 0xFFFFFFFF;

                        (from, to, state.action_id, state.telemetry_chats.clone())
                    };

                    let nav_event = client.build_nav_event(
                        user_id, session_id, screen_from, screen_to, last_nav_time, action_id, &chats
                    );
                    last_nav_time = nav_event.time;
                    session_events.push(nav_event);

                    if screen_to == Screen::Chat {
                        client.sleep_between_f64(RENDER_DELAY).await;
                        session_events.push(client.build_open_chat_event(user_id, session_id));
                    } else if screen_to == Screen::Chats {
                        if rand::thread_rng().gen_bool(OPEN_CHATS_RENDER_CHANCE) {
                            client.sleep_between_f64(RENDER_DELAY).await;
                            session_events.push(client.build_open_chats_event(user_id, session_id));
                        }
                    }
                }

                if is_aborted {
                    break;
                }

                let current_screen = client.state.lock().await.planner.current_screen;
                if current_screen != Screen::Background && rand::thread_rng().gen_bool(RETURN_TO_BACKGROUND_CHANCE) {
                    client.sleep_between(RETURN_DELAY).await;

                    let (action_id, screen_from, chats) = {
                        let mut state = client.state.lock().await;
                        state.action_id = state.action_id.wrapping_add(1) % 0xFFFFFFFF;
                        let from = state.planner.current_screen;
                        state.planner.reset_to_background(); // Уходим в фон
                        (state.action_id, from, state.telemetry_chats.clone())
                    };

                    let nav_event = client.build_nav_event(
                        user_id, session_id, screen_from, Screen::Background, last_nav_time, action_id, &chats
                    );
                    last_nav_time = nav_event.time;
                    session_events.push(nav_event);
                } else {
                    client.state.lock().await.planner.reset_to_background();
                }

                client.send_events(session_events).await;

                debug!("Сессия завершена, ожидание session_idle_delay");
                tokio::select! {
                    _ = client.sleep_between(SESSION_IDLE_DELAY) => {},
                     _ = shutdown_rx.recv() => return,
                }
            }
        });
    }
}
