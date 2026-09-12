use rumax::{MaxClient, SyncState, models::{Identity, Response}};
use serde_json::Value;
use std::io::{self, Write};
use std::fs;
use std::sync::Arc;
use log::{info, error, warn};

mod identity;
use identity::generate_device;

const DEVICE_ID_FILE: &str = ".device.id";
const TOKEN_FILE: &str = ".session.token";

fn read_line(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn extract_keys_structure(val: &Value) -> Value {
    match val {
        Value::Object(map) => {
            let filtered: serde_json::Map<String, Value> = map
            .iter()
            .map(|(k, v)| (k.clone(), extract_keys_structure(v)))
            .collect();
            Value::Object(filtered)
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                Value::String("[empty array]".to_string())
            } else {
                let elem_keys = extract_keys_structure(&arr[0]);
                Value::Array(vec![elem_keys])
            }
        }
        _ => Value::Null,
    }
}

fn print_keys_tree(val: &Value, indent: usize) {
    let spaces = "  ".repeat(indent);
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Object(_) => {
                        println!("{}{}:", spaces, k);
                        print_keys_tree(v, indent + 1);
                    }
                    Value::Array(arr) => {
                        let len = arr.len();
                        if len == 0 {
                            println!("{}{} [len: 0]", spaces, k);
                        } else {
                            // Проверяем тип первого элемента: если объект, выводим его структуру
                            match &arr[0] {
                                Value::Object(_) | Value::Array(_) => {
                                    println!("{}{} [len: {}]:", spaces, k, len);
                                    print_keys_tree(&arr[0], indent + 1);
                                }
                                _ => {
                                    println!("{}{} [primitives, len: {}]", spaces, k, len);
                                }
                            }
                        }
                    }
                    _ => {
                        println!("{}{}", spaces, k);
                    }
                }
            }
        }
        Value::Array(arr) => {
            let len = arr.len();
            if let Some(first) = arr.first() {
                println!("{}[item schema, len: {}]:", spaces, len);
                print_keys_tree(first, indent + 1);
            } else {
                println!("{}[empty array]", spaces);
            }
        }
        _ => {}
    }
}

fn log_response(label: &str, resp: &Response) {
    info!("[{}] Структура ключей:", label);
    print_keys_tree(&resp.payload, 1);
}

fn get_device() -> Identity {
    if let Ok(content) = fs::read_to_string(DEVICE_ID_FILE) {
        if let Ok(identity) = serde_json::from_str::<Identity>(&content) {
            info!("Используем существующие identity из файла {}", DEVICE_ID_FILE);
            return identity;
        }
    }

    info!("Создаем новую identity устройства...");
    let identity = generate_device();

    let content = serde_json::to_string(&identity).unwrap();
    fs::write(DEVICE_ID_FILE, content).expect("Не удалось записать .device.id");

    info!("Новая identity сохранена в {}", DEVICE_ID_FILE);
    identity
}

fn load_token() -> Option<String> {
    match fs::read_to_string(TOKEN_FILE) {
        Ok(token) if !token.is_empty() => {
            info!("Токен сессии загружен из {}", TOKEN_FILE);
            Some(token)
        }
        _ => {
            info!("Файл токена {} не найден", TOKEN_FILE);
            None
        }
    }
}

fn save_token(token: &str) {
    if let Err(e) = fs::write(TOKEN_FILE, token) {
        error!("Не удалось сохранить токен в {}: {}", TOKEN_FILE, e);
    } else {
        info!("Токен сессии сохранен в {}", TOKEN_FILE);
    }
}

fn delete_token() {
    if fs::remove_file(TOKEN_FILE).is_ok() {
        info!("Файл токена {} удален", TOKEN_FILE);
    }
}

async fn set_user_id_and_spawn_telemetry(
    client: &MaxClient,
    sync_resp: &Response,
    sync2_opt: &Option<Response>,
) {
    let profile_json = sync2_opt
    .as_ref()
    .and_then(|r| r.payload.get("profile"))
    .or_else(|| sync_resp.payload.get("profile"));

    let user_id = profile_json
    .and_then(|s| s.get("contact"))
    .and_then(|s| s.get("id"))
    .and_then(|id| id.as_u64());

    if let Some(id) = user_id {
        info!("Установка user_id: {}", id);
        client.set_user_id(id).await;

        info!("Запуск фоновой задачи телеметрии...");
        client.spawn_telemetry_task().await;
    } else {
        warn!("Не удалось найти user_id в ответе sync. Телеметрия не запущена!");
    }
}

#[tokio::main]
async fn main() -> Result<(), rumax::errors::Error> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,max_client_lib=debug")
    ).init();

    let client = Arc::new(MaxClient::new());
    let identity = get_device();

    info!("Подключение к MobileSocket...");
    match client.connect(identity, true).await {
        Ok(resp) => {
            info!("Handshake успешен!");
            log_response("Handshake", &resp);
        }
        Err(e) => {
            error!("Ошибка подключения: {}", e);
            return Err(e.into());
        }
    }

    if let Some(token) = load_token() {
        info!("Попытка входа по сохраненному токену...");
        client.set_token(token).await;

        let initial_sync_state = SyncState::default();
        match client.sync(Some(initial_sync_state)).await {
            Ok((sync_resp, sync2_opt, _new_sync_state)) => {
                info!("Вход по токену успешен!");
                log_response("Sync 1", &sync_resp);
                if let Some(ref sync2) = sync2_opt {
                    log_response("Sync 2 (Profile)", sync2);
                }

                set_user_id_and_spawn_telemetry(&client, &sync_resp, &sync2_opt).await;
            }
            Err(e) => {
                warn!("Ошибка входа по токену (возможно, истек): {}. Удаляем токен", e);
                delete_token();
                info!("Перезапустите скрипт для входа по номеру телефона");
                return Ok(());
            }
        }
    } else {
        info!("Токен не найден, запуск входа по номеру телефона...");

        let phone = read_line("Введите номер телефона (+7...): ");
        match client.start_auth(phone).await {
            Ok(resp) => {
                log_response("Start Auth", &resp);
            }
            Err(e) => {
                error!("Ошибка запроса кода: {}", e);
                return Err(e.into());
            }
        }

        let code = read_line("Введите код из СМС/звонка: ");
        let resp = match client.check_code(code).await {
            Ok(resp) => {
                log_response("Check Code", &resp);
                resp
            }
            Err(e) => {
                error!("Ошибка проверки кода: {}", e);
                return Err(e.into());
            }
        };

        if let Some(challenge) = resp.payload.get("passwordChallenge") {
            let track_id = challenge.get("trackId").and_then(|t| t.as_str()).unwrap_or("").to_string();
            let hint = challenge.get("hint").and_then(|t| t.as_str()).unwrap_or("нет подсказки");

            println!("\nТребуется облачный пароль (Подсказка: {})", hint);
            let password = read_line("Пароль: ");

            let pass_resp = client.check_password(password, track_id).await?;
            log_response("Check Password", &pass_resp);

            let success = pass_resp.payload.get("tokenAttrs")
            .and_then(|t| t.get("LOGIN"))
            .and_then(|l| l.get("token"))
            .is_some();

            if !success {
                error!("Неверный облачный пароль или не удалось получить токен!");
                return Ok(());
            }
        }

        let initial_sync_state = SyncState::default();
        match client.sync(Some(initial_sync_state)).await {
            Ok((sync_resp, sync2_opt, _new_sync_state)) => {
                info!("Вход по коду и телефону успешен");
                log_response("Sync 1", &sync_resp);
                if let Some(ref sync2) = sync2_opt {
                    log_response("Sync 2 (Profile)", sync2);
                }

                if let Some(new_token) = client.get_token().await {
                    save_token(&new_token);
                } else {
                    warn!("Не удалось получить токен из клиента для сохранения");
                }

                set_user_id_and_spawn_telemetry(&client, &sync_resp, &sync2_opt).await;
            }
            Err(e) => {
                error!("Ошибка синхронизации: {}", e);
                return Err(e.into());
            }
        }
    }

    info!("Успешный вход!");

    let chat_id_str = read_line("Введите Chat ID для тестового сообщения: ");
    let chat_id: i64 = match chat_id_str.parse() {
        Ok(num) => num,
        Err(_) => {
            error!("Это не похоже на число (i64). Выходим");
            return Ok(());
        }
    };

    let message = read_line("Введите текст сообщения: ");
    match client.send_message(chat_id, message, None).await {
        Ok(resp) => {
            info!("Сообщение успешно отправлено!");
            log_response("Send Message", &resp);
        }
        Err(e) => {
            error!("Ошибка отправки сообщения: {}", e);
        }
    }

    match client.fetch_history(chat_id, None).await {
        Ok(resp) => {
            log_response("Fetch History", &resp);
        }
        Err(e) => {
            error!("Ошибка получения истории сообщений: {}", e);
        }
    }

    info!("\nКлиент остается подключенным. Нажмите Enter для выхода");
    read_line("");
    info!("Завершение работы...");

    Ok(())
}
