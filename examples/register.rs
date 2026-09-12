use rumax::{MaxClient, SyncState, models::Identity};
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
            info!("Используем существующие данные устройства из {}", DEVICE_ID_FILE);
            return identity;
        }
    }

    info!("Генерируем новые данные устройства...");
    let identity = generate_device();

    let content = serde_json::to_string_pretty(&identity).unwrap();
    fs::write(DEVICE_ID_FILE, content).expect("Не удалось сохранить данные устройства");

    identity
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,max_client_lib=debug")
    ).init();

    let client = Arc::new(MaxClient::new());

    let identity = get_device();

    info!("Подключение к MobileSocket...");
    match client.connect(identity, true).await {
        Ok(resp) => {
            info!("Handshake успешен!");
            debug!("Ответ Handshake: {:?}", resp.payload);
        }
        Err(e) => {
            error!("Ошибка подключения: {}", e);
            return;
        }
    }

    let phone = read_line("Введите номер телефона (+7...): ");
    info!("Отправляем запрос на номер {}", phone);

    match client.start_auth(phone).await {
        Ok(resp) => {
            info!("Запрос кода успешен.");
            debug!("Ответ start_auth: {:?}", resp.payload);
        }
        Err(e) => {
            error!("Ошибка запроса кода: {}", e);
            return;
        }
    }

    let code = read_line("Введите код из СМС: ");
    info!("Проверяем код...");

    let token: String;

    match client.check_code(code).await {
        Ok(resp) => {
            info!("Верный код! Регистрируемся...");
            token = resp.payload.get("tokenAttrs")
            .and_then(|t| t.get("REGISTER"))
            .and_then(|l| l.get("token"))
            .and_then(|t| t.as_str())
            .map(|t| t.to_string())
            .unwrap_or_else(|| {
                log::error!("token отсутствует в ответе сервера!");
                std::process::exit(1);
            })
        }
        Err(e) => {
            info!("Ошибка проверки кода! {}", e);
            return;
        }
    }

    log::info!("token {:?}", token);

    let first_name = read_line("Введите имя: ");

    let reg_resp = client.submit_register(first_name, Option::None).await;

    log::info!("reg_resp {:?}", reg_resp);

    info!("Выполняем синхронизацию (sync)...");
    let initial_sync_state = SyncState::default();

    match client.sync(Some(initial_sync_state)).await {
        Ok((sync_resp, sync2_opt, _new_sync_state)) => {
            log::info!("Синхронизация успешна. {:?}", sync_resp.payload);

            let profile_json = sync2_opt
            .as_ref()
            .and_then(|r| r.payload.get("profile"))
            .or_else(|| sync_resp.payload.get("profile"));

            let user_id = profile_json
            .and_then(|s| s.get("contact"))
            .and_then(|s| s.get("id"))
            .and_then(|id| id.as_u64());

            log::info!("test {:?}", user_id);

            if let Some(id) = user_id {
                log::info!("Установка user_id: {}", id);
                client.set_user_id(id).await;

                log::info!("Запуск фоновой задачи телеметрии...");
                client.spawn_telemetry_task().await;
            } else {
                log::warn!("Не удалось найти user_id в ответе sync. Телеметрия не запущена.");
            }
        }
        Err(e) => {
            log::error!("Ошибка sync: {}", e);
            return;
        }
    }

    info!("\nУспешная регистрация!");

    read_line("");
    info!("Завершение работы...");
}
