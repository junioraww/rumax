use rumax::{MaxClient, SyncState, models::{Identity}};
use std::io::{self, Write};
use std::fs;
use log::{info, error, debug};
use std::sync::Arc;

mod identity;
use identity::generate_device;

const DEVICE_ID_FILE: &str = ".device.id";

fn read_line(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn get_device() -> Identity {
    if let Ok(content) = fs::read_to_string(DEVICE_ID_FILE) {
        if let Ok(identity) = serde_json::from_str::<Identity>(&content) {
            info!("{}", DEVICE_ID_FILE);
            return identity;
        }
    }

    let identity = generate_device();

    let content = serde_json::to_string_pretty(&identity).unwrap();
    fs::write(DEVICE_ID_FILE, content).expect("");

    identity
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,max_client_lib=debug")
    ).init();

    let client = Arc::new(MaxClient::new());

    let identity = get_device();

    match client.connect(identity, true).await {
        Ok(resp) => {
            debug!("{:?}", resp.payload);
        }
        Err(e) => {
            error!("{}", e);
            return;
        }
    }

    let phone = read_line("Телефон: ");

    match client.start_auth(phone).await {
        Ok(resp) => {
            debug!("{:?}", resp.payload);
        }
        Err(e) => {
            error!("{}", e);
            return;
        }
    }

    let code = read_line("Код: ");

    match client.check_code(code).await {
        Ok(resp) => {
            debug!("{:?}", resp.payload);

            if let Some(challenge) = resp.payload.get("passwordChallenge") {
                let track_id = challenge.get("trackId").and_then(|t| t.as_str()).unwrap_or("").to_string();
                let hint = challenge.get("hint").and_then(|t| t.as_str()).unwrap_or("нет подсказки");

                println!("\nТребуется облачный пароль (Подсказка: {})", hint);
                let password = read_line("Пароль: ");

                match client.check_password(password, track_id).await {
                    Ok(pass_resp) => {
                        debug!("{:?}", pass_resp.payload);

                        let success = pass_resp.payload.get("tokenAttrs")
                            .and_then(|t| t.get("LOGIN"))
                            .and_then(|l| l.get("token"))
                            .is_some();

                        if !success {
                            error!("Неверный облачный пароль или не удалось получить токен!");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        error!("Ошибка отправки облачного пароля: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                let success = resp.payload.get("tokenAttrs")
                    .and_then(|t| t.get("LOGIN"))
                    .and_then(|l| l.get("token"))
                    .is_some();

                if !success {
                    error!("Токен LOGIN не получен!");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            error!("{}", e);
            return;
        }
    }

    let initial_sync_state = SyncState::default();

    match client.sync(Some(initial_sync_state)).await {
        Ok((sync_resp, sync2_opt, _new_sync_state)) => {
            let profile_json = sync2_opt
                .as_ref()
                .and_then(|r| r.payload.get("profile"))
                .or_else(|| sync_resp.payload.get("profile"));

            let user_id = profile_json
                .and_then(|s| s.get("contact"))
                .and_then(|s| s.get("id"))
                .and_then(|id| id.as_u64());

            if let Some(id) = user_id {
                client.set_user_id(id).await;
                client.spawn_telemetry_task().await;
                info!("Успешный вход! User ID: {}", id);

                // в полноценном приложении тут стоит сохранить _new_sync_state в JSON-файл
                // чтобы при следующем запуске скормить его в sync() вместо SyncState::default()
            } else {
                error!("Не удалось найти профиль пользователя в ответе sync");
            }
        }
        Err(e) => {
            log::error!("{}", e);
        }
    }

    let chat_id_str = read_line("Chat ID: ");

    let chat_id: i64 = match chat_id_str.parse() {
        Ok(num) => num,
        Err(_) => {
            return;
        }
    };

    let message = read_line("Текст: ");

    match client.send_message(chat_id, message, None).await {
        Ok(resp) => {
            info!("{:?}", resp.payload);
        }
        Err(e) => {
            error!("{}", e);
        }
    }

    match client.fetch_history(chat_id, None).await {
        Ok(resp) => {
            info!("{:?}", resp.payload);
        }
        Err(e) => {
            error!("{}", e);
        }
    }

    read_line("");
}
