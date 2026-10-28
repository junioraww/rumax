use rumax::{models::Identity, MaxClient};
use std::fs;
use std::sync::Arc;
use log::{error, info};

mod identity;
use identity::generate_web_device;

const DEVICE_ID_FILE: &str = ".device.web.id";
const TOKEN_FILE: &str = ".session.token";

/// Отрисовка QR-кода в терминале (два символа на модуль).
fn render_qr(data: &str) {
    match qrcode::QrCode::new(data.as_bytes()) {
        Ok(code) => {
            let width = code.width();
            let colors = code.to_colors();
            let mut out = String::new();
            for y in 0..width {
                for x in 0..width {
                    let dark = colors[y * width + x] == qrcode::Color::Dark;
                    out.push_str(if dark { "██" } else { "  " });
                }
                out.push('\n');
            }
            println!("{}", out);
        }
        Err(e) => error!("Failed to render QR code: {}", e),
    }
}

fn get_device() -> Identity {
    if let Ok(content) = fs::read_to_string(DEVICE_ID_FILE) {
        if let Ok(identity) = serde_json::from_str::<Identity>(&content) {
            info!("Using existing device identity from {}", DEVICE_ID_FILE);
            return identity;
        }
    }

    info!("Generating new WEB device identity...");
    let identity = generate_web_device();

    let content = serde_json::to_string_pretty(&identity).unwrap();
    fs::write(DEVICE_ID_FILE, content).expect("Не удалось сохранить данные устройства");

    identity
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,rumax=debug"),
    )
    .init();

    let client = Arc::new(MaxClient::new());
    let identity = get_device();

    info!("Connecting to WebSocket (WEB)...");
    // QR auth is only available to the WEB client, so we use a WEB identity
    // together with the WebSocket transport (is_mobile = false).
    match client.connect(identity, false).await {
        Ok(resp) => info!("Handshake ok! {:?}", resp.payload),
        Err(e) => {
            error!("Connection failed: {}", e);
            return;
        }
    }

    // Step 1: request QR code (opcode 288)
    info!("Requesting QR code...");
    let session = match client.qr_login_request().await {
        Ok(s) => s,
        Err(e) => {
            error!("QR code request failed: {}", e);
            return;
        }
    };

    // Step 2: show the QR code to the user
    println!("\n=== Scan this QR code with the MAX app ===\n");
    render_qr(&session.qr_link);
    println!("Link: {}", session.qr_link);
    println!("Expires at (unix ms): {}\n", session.expires_at);
    println!("Waiting for scan...\n");

    // Step 3-4: wait for scan + LOGIN (opcode 19)
    match client.qr_login_confirm(&session).await {
        Ok((_login, _state)) => {
            info!("QR authorization succeeded!");

            if let Some(token) = client.get_token().await {
                let _ = fs::write(TOKEN_FILE, token);
                info!("Token saved to {}", TOKEN_FILE);
            }

            // All other client features are now available, e.g.:
            // client.get_chats(vec![]).await?;
            // client.fetch_history(chat_id, None).await?;
            // client.send_message(chat_id, "Hello".into(), None).await?;

            // Keep the process running and listen for incoming events (messages,
            // typing, presence, reads, ...). The connection is kept alive by the
            // background ping task; this loop just prevents the process from exiting.
            let mut events = client.subscribe();
            info!("Listening for events (press Ctrl+C to stop)...");

            loop {
                match events.recv().await {
                    Ok(value) => match value.get("type").and_then(|t| t.as_str()) {
                        Some("rx") => {
                            if let Some(resp) = value.get("response") {
                                let cmd = resp.get("cmd").and_then(|c| c.as_u64()).unwrap_or(0);
                                let opcode =
                                    resp.get("opcode").and_then(|o| o.as_u64()).unwrap_or(0);
                                let payload = resp
                                    .get("payload")
                                    .map(|p| p.to_string())
                                    .unwrap_or_default();
                                println!("[push] cmd={cmd} opcode={opcode} payload={payload}");
                            }
                        }
                        Some("log") => {
                            // Raw request/response traces (see RUST_LOG=debug).
                        }
                        _ => {}
                    },
                    Err(e) => {
                        error!("Event channel error: {e}");
                        break;
                    }
                }
            }
        }
        Err(e) => error!("QR authorization failed: {}", e),
    }
}
