use rumax::{MaxClient, models::{Identity, UserAgent}};
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

            resp.payload.get("tokenAttrs")
            .and_then(|t| t.get("LOGIN"))
            .and_then(|l| l.get("token"))
            .and_then(|t| t.as_str())
            .map(|t| t.to_string())
            .unwrap_or_else(|| {
                std::process::exit(1);
            });
        }
        Err(e) => {
            error!("{}", e);
            return;
        }
    }

    match client.sync().await {
        Ok(sync_resp) => {
            let user_id = sync_resp.payload
            .get("profile")
            .and_then(|s| s.get("contact"))
            .and_then(|s| s.get("id"))
            .and_then(|id| id.as_u64());

            if let Some(id) = user_id {
                client.set_user_id(id).await;
                client.spawn_telemetry_task().await;
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
