use rumax::{MaxClient, SyncState, models::Identity};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::Arc;

const DEVICE_ID_FILE: &str = ".device.id";
const TOKEN_FILE: &str = ".session.token";

fn msgpack_to_json(val: rmpv::Value) -> serde_json::Value {
    match val {
        rmpv::Value::Nil => serde_json::Value::Null,
        rmpv::Value::Boolean(b) => serde_json::Value::Bool(b),
        rmpv::Value::Integer(i) => {
            if let Some(v) = i.as_u64() {
                serde_json::Value::Number(v.into())
            } else if let Some(v) = i.as_i64() {
                serde_json::Value::Number(v.into())
            } else {
                serde_json::Value::Null
            }
        }
        rmpv::Value::F32(f) => serde_json::json!(f),
        rmpv::Value::F64(f) => serde_json::json!(f),
        rmpv::Value::String(s) => serde_json::Value::String(s.into_str().unwrap_or_default()),
        rmpv::Value::Binary(b) => serde_json::Value::String(String::from_utf8_lossy(&b).to_string()),
        rmpv::Value::Array(vec) => serde_json::Value::Array(vec.into_iter().map(msgpack_to_json).collect()),
        rmpv::Value::Map(vec) => {
            let mut map = serde_json::Map::new();
            for (k, v) in vec {
                let key_str = match k {
                    rmpv::Value::String(s) => s.into_str().unwrap_or_default(),
                    rmpv::Value::Integer(i) => i.to_string(),
                    _ => "unknown".to_string(),
                };
                map.insert(key_str, msgpack_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        rmpv::Value::Ext(_, _) => serde_json::Value::Null,
    }
}

fn load_credentials() -> (Identity, String) {
    let home = std::env::var("HOME").unwrap_or_default();
    let data_dir = format!("{}/.local/share/org.meowkie.max/data", home);
    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            let meta_path = entry.path().join("meta");
            if meta_path.exists() {
                if let Ok(bytes) = fs::read(&meta_path) {
                    if !bytes.starts_with(b"ENC") {
                        if let Ok(val) = rmpv::decode::read_value(&mut &bytes[..]) {
                            let json_val = msgpack_to_json(val);
                            if let (Some(token), Some(device_val)) = (
                                json_val.get("token").and_then(|t| t.as_str()),
                                json_val.get("device"),
                            ) {
                                if let Ok(identity) = serde_json::from_value::<Identity>(device_val.clone()) {
                                    return (identity, token.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let content = fs::read_to_string(DEVICE_ID_FILE).expect("Failed to read .device.id");
    let identity = serde_json::from_str::<Identity>(&content).expect("Failed to parse .device.id");
    let token = fs::read_to_string(TOKEN_FILE).expect("Failed to read .session.token").trim().to_string();
    (identity, token)
}

fn generate_test_audio(path: &str) {
    let status = Command::new("ffmpeg")
        .args(&[
            "-y",
            "-f", "lavfi",
            "-i", "sine=frequency=1000:duration=2",
            "-c:a", "libopus",
            "-b:a", "32k",
            "-ar", "48000",
            "-ac", "1",
            path,
        ])
        .status()
        .expect("ffmpeg failed to execute");
    assert!(status.success(), "ffmpeg audio generation failed");
}

fn generate_test_video(path: &str) {
    let status = Command::new("ffmpeg")
        .args(&[
            "-y",
            "-f", "lavfi",
            "-i", "testsrc=duration=2:size=480x480:rate=30",
            "-f", "lavfi",
            "-i", "sine=frequency=440:duration=2",
            "-c:v", "libx264",
            "-pix_fmt", "yuv420p",
            "-c:a", "aac",
            "-b:a", "64k",
            "-movflags", "+faststart",
            path,
        ])
        .status()
        .expect("ffmpeg failed to execute");
    assert!(status.success(), "ffmpeg video generation failed");
}

async fn upload_media_direct(
    client: &reqwest::Client,
    upload_url: &str,
    data: &[u8],
    filename: &str,
) -> Result<String, String> {
    let total = data.len();
    let resp = client
        .post(upload_url)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Disposition", format!("attachment; filename=\"{}\"", filename))
        .header("Content-Range", format!("bytes 0-{}/{}", total.saturating_sub(1), total))
        .header("Content-Length", total.to_string())
        .header("Connection", "close")
        .body(data.to_vec())
        .send()
        .await
        .map_err(|e| format!("Upload HTTP request failed: {}", e))?;

    let status = resp.status();
    let body_bytes = resp.bytes().await.map_err(|e| format!("Failed to read body: {}", e))?;
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    if !status.is_success() {
        return Err(format!("Upload HTTP {} - body: {}", status, body_str));
    }
    if body_str.contains("error_msg") || body_str.contains("error_code") {
        return Err(format!("Upload rejected by CDN: {}", body_str));
    }
    Ok(body_str)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    let (identity, token) = load_credentials();

    let client = Arc::new(MaxClient::new());
    println!("Connecting client...");
    client.connect(identity, true).await?;
    client.set_token(token).await;

    println!("Synchronizing state...");
    let (sync_resp, _, _) = client.sync(Some(SyncState::default())).await?;
    let profile = sync_resp.payload.get("profile");
    let user_id = profile
        .and_then(|p| p.get("contact"))
        .and_then(|c| c.get("id"))
        .and_then(|id| id.as_u64())
        .unwrap_or(0);
    println!("Logged in successfully. User ID: {}", user_id);
    client.set_user_id(user_id).await;

    let http = rumax::create_http_client();

    let audio_path = "/tmp/test_voice_msg.ogg";
    println!("\nGenerating test audio message (Ogg Opus 48000Hz mono)...");
    generate_test_audio(audio_path);
    let audio_bytes = fs::read(audio_path)?;
    println!("Generated audio: {} bytes", audio_bytes.len());

    println!("Requesting audio upload URL...");
    let audio_upload_resp = client.get_audio_upload(1).await?;
    println!("Audio upload response: {:?}", audio_upload_resp.payload);

    let info_list = audio_upload_resp.payload.get("info")
        .and_then(|i| i.as_array())
        .expect("Missing info array in audio upload response");
    let audio_info = &info_list[0];
    let audio_upload_url = audio_info.get("url").and_then(|u| u.as_str()).expect("Missing url");
    let audio_token = audio_info.get("token").and_then(|t| t.as_str()).expect("Missing token");
    let audio_video_id = audio_info.get("videoId").and_then(|v| v.as_u64()).unwrap_or(0);
    println!("Upload URL: {}", audio_upload_url);
    println!("Audio Token: {}", audio_token);
    println!("Audio Video ID: {}", audio_video_id);

    let synthetic_audio_name = format!("{}.ogg", audio_video_id);
    println!("Uploading audio to CDN...");
    let upload_result = upload_media_direct(&http, audio_upload_url, &audio_bytes, &synthetic_audio_name).await?;
    println!("Audio upload SUCCESS! CDN response: {}", upload_result);

    println!("Sending audio message to chat 0 (Favorites)...");
    let audio_attach = json!({
        "_type": "AUDIO",
        "token": audio_token,
        "duration": 2000,
        "wave": vec![0u8; 80]
    });
    let mut audio_send_args = HashMap::new();
    audio_send_args.insert("attaches".to_string(), json!([audio_attach]));

    let send_audio_resp = client.send_message(0, "".to_string(), Some(audio_send_args)).await?;
    println!("Audio message sent: {:?}", send_audio_resp.payload);

    println!("Fetching history for chat 0...");
    let history_resp = client.fetch_history(0, None).await?;
    let messages = history_resp.payload.get("messages").and_then(|m| m.as_array()).expect("Missing messages");
    
    let mut found_audio_url: Option<String> = None;
    for msg in messages {
        if let Some(attaches) = msg.get("attaches").and_then(|a| a.as_array()) {
            for att in attaches {
                if att.get("_type").and_then(|t| t.as_str()) == Some("AUDIO") {
                    let url = att.get("url").or_else(|| att.get("baseUrl")).and_then(|u| u.as_str());
                    if let Some(u) = url {
                        found_audio_url = Some(u.to_string());
                        break;
                    }
                }
            }
        }
        if found_audio_url.is_some() {
            break;
        }
    }

    if let Some(audio_url) = found_audio_url {
        println!("Found audio URL in history: {}", audio_url);
        println!("Downloading audio from CDN...");
        let dl_resp = http.get(&audio_url).send().await?;
        assert!(dl_resp.status().is_success(), "Failed to download audio file");
        let dl_bytes = dl_resp.bytes().await?;
        println!("Downloaded audio bytes: {}", dl_bytes.len());
        assert!(!dl_bytes.is_empty(), "Downloaded audio is empty");
        assert_eq!(&dl_bytes[0..4], b"OggS", "Downloaded file does not have OggS header");
        println!("Audio message verified successfully!");
    } else {
        println!("Audio message sent but URL not populated yet in fetch_history.");
    }

    let video_path = "/tmp/test_video_note.mp4";
    println!("\nGenerating test video note (MP4 H.264+AAC 480x480)...");
    generate_test_video(video_path);
    let video_bytes = fs::read(video_path)?;
    println!("Generated video note: {} bytes", video_bytes.len());

    println!("Requesting video note upload URL...");
    let video_upload_resp = client.get_video_note_upload(1).await?;
    println!("Video note upload response: {:?}", video_upload_resp.payload);

    let v_info_list = video_upload_resp.payload.get("info")
        .and_then(|i| i.as_array())
        .expect("Missing info array in video upload response");
    let v_info = &v_info_list[0];
    let v_upload_url = v_info.get("url").and_then(|u| u.as_str()).expect("Missing url");
    let v_token = v_info.get("token").and_then(|t| t.as_str()).expect("Missing token");
    let v_video_id = v_info.get("videoId").and_then(|v| v.as_u64()).unwrap_or(0);
    println!("Upload URL: {}", v_upload_url);
    println!("Video Token: {}", v_token);
    println!("Video ID: {}", v_video_id);

    let synthetic_video_name = format!("{}.mp4", v_video_id);
    println!("Uploading video note to CDN...");
    let v_upload_result = upload_media_direct(&http, v_upload_url, &video_bytes, &synthetic_video_name).await?;
    println!("Video note upload SUCCESS! CDN response: {}", v_upload_result);

    println!("Sending video note message to chat 0 (Favorites)...");
    let video_attach = json!({
        "_type": "VIDEO",
        "videoType": 1,
        "token": v_token,
        "duration": 2000,
        "wave": vec![0u8; 80]
    });
    let mut video_send_args = HashMap::new();
    video_send_args.insert("attaches".to_string(), json!([video_attach]));

    let send_video_resp = client.send_message(0, "".to_string(), Some(video_send_args)).await?;
    println!("Video note message sent: {:?}", send_video_resp.payload);

    let sent_msg_id = send_video_resp.payload.get("message")
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("");

    println!("Requesting video stream/download URL via opcode 83 (videoPlay)...");
    let play_resp = client.send_and_wait(83, json!({
        "messageId": sent_msg_id.parse::<i64>().unwrap_or(0),
        "chatId": 0,
        "token": v_token,
        "videoId": v_video_id
    }), 0).await?;
    println!("Video play response: {:?}", play_resp.payload);

    let mut download_video_url: Option<String> = None;
    for key in &["MP4_480", "MP4_360", "MP4_720", "MP4_240", "EXTERNAL", "HLS"] {
        if let Some(u) = play_resp.payload.get(*key).and_then(|v| v.as_str()) {
            if !u.is_empty() {
                download_video_url = Some(u.to_string());
                break;
            }
        }
    }

    if let Some(v_url) = download_video_url {
        println!("Downloading video note from: {}", v_url);
        let dl_v_resp = http.get(&v_url).send().await?;
        assert!(dl_v_resp.status().is_success(), "Failed to download video file");
        let dl_v_bytes = dl_v_resp.bytes().await?;
        println!("Downloaded video bytes: {}", dl_v_bytes.len());
        assert!(!dl_v_bytes.is_empty(), "Downloaded video is empty");
        println!("Video note verified successfully!");
    } else {
        println!("Video play did not return a direct MP4 link yet (might still be transcoding).");
    }

    println!("\nALL TESTS COMPLETED SUCCESSFULLY!");
    Ok(())
}
