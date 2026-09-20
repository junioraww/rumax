use crate::{errors::ClientResult, MaxClient};
use crate::models::Response;
use serde_json::{json, Value};
use tokio::{fs::File};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use reqwest::{Body, multipart};
//use futures_util::StreamExt;
use std::time::Duration;

//const CHUNK_SIZE: usize = 6 * 1024 * 1024;

impl MaxClient {
    /*
     * Returns url, file_id
     */
    pub async fn get_photo_upload(
        &self,
        count: i64,
        profile: bool,
    ) -> ClientResult<Response> {
        let payload = json!({
            "count": count,
            "profile": profile,
        });
        self.send_and_wait(80, payload, 0).await
    }

    pub async fn get_video_upload(
        &self,
        count: i64,
        profile: bool,
    ) -> ClientResult<Response> {
        let payload = json!({
            "count": count,
            "profile": profile,
        });
        self.send_and_wait(82, payload, 0).await
    }

    pub async fn get_file_upload(
        &self,
        count: i64,
        profile: bool,
    ) -> ClientResult<Response> {
        let payload = json!({
            "count": count,
            "profile": profile,
        });
        self.send_and_wait(87, payload, 0).await
    }

    pub async fn get_audio_upload(&self, count: i64) -> ClientResult<Response> {
        let payload = json!({
            "uploaderType": 1,
            "type": 2,
            "count": count,
        });
        self.send_and_wait(82, payload, 0).await
    }

    pub async fn get_video_note_upload(&self, count: i64) -> ClientResult<Response> {
        let payload = json!({
            "uploaderType": 1,
            "type": 1,
            "count": count,
        });
        self.send_and_wait(82, payload, 0).await
    }

    pub async fn upload_photo(
        &self,
        upload_url: String,
        mut file: File,
        file_name: String,
        mime: Option<String>,
    ) -> Value {
        let ext = file_name
        .split('.')
        .last()
        .map(|e| e.to_lowercase());

        let mime = if let Some(m) = mime {
            m
        } else if let Some(ext) = ext {
            match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg".to_string(),
                "png" => "image/png".to_string(),
                _ => {
                    return json!({ "error": "Unsupported file extension" });
                }
            }
        } else {
            return json!({ "error": "Failed to determine MIME type" });
        };

        let mut file_bytes = Vec::new();
        if let Err(_) = file.read_to_end(&mut file_bytes).await {
            return json!({ "error": "Failed to read photo" });
        }

        let ext = mime.split('/').last().unwrap_or("jpg");

        let form = multipart::Form::new().part(
            "file",
            multipart::Part::bytes(file_bytes)
            .file_name(format!("image.{}", ext))
            .mime_str(&mime)
            .unwrap(),
        );

        let client = crate::create_http_client();
        let response = match client.post(upload_url).multipart(form).send().await {
            Ok(r) => r,
            Err(e) => return json!({ "error": format!("Upload request failed: {}", e) }),
        };

        if !response.status().is_success() {
            return json!({ "error": format!("Upload failed with status {}", response.status()) });
        }

        let json_resp: serde_json::Value = match response.json().await {
            Ok(j) => j,
            Err(e) => return json!({ "error": format!("Failed to parse JSON: {}", e) }),
        };

        let token = match json_resp.get("photos")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.values().next())
        .and_then(|photo| photo.get("token"))
        .and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return json!({ "error": "Token not found in response" }),
        };

        json!({ "photoToken": token.to_string() })
    }

    pub async fn upload_video(
        &self,
        upload_url: String,
        video_id: u64,
        token: String,
        mut file: File,
        file_name: String,
    ) -> Value {
        let mut file_bytes = Vec::new();
        if let Err(e) = file.read_to_end(&mut file_bytes).await {
            return json!({ "error": format!("Failed to read video: {}", e) });
        }

        let file_size = file_bytes.len();
        if file_size == 0 {
            return json!({ "error": "Empty file" });
        }

        let client = match crate::http_client_builder()
        .timeout(Duration::from_secs(600))
        .build()
        {
            Ok(c) => c,
            Err(e) => return json!({ "error": format!("Failed to build client: {}", e) }),
        };

        let response = match client.post(upload_url)
        .header("Content-Disposition", format!("attachment; filename={}", file_name))
        .header("Content-Range", format!("0-{}/{}", file_size - 1, file_size))
        .header("Content-Length", file_size)
        .header("Connection", "keep-alive")
        .body(file_bytes)
        .send()
        .await
        {
            Ok(r) => r,
            Err(e) => return json!({ "error": format!("Upload failed: {}", e) }),
        };

        if !response.status().is_success() {
            return json!({ "error": format!("Upload failed with status {}", response.status()) });
        }

        json!({
            "videoId": video_id,
            "token": token
        })
    }

    pub async fn upload_file(
        &self,
        upload_url: String,
        file_id: u64,
        file: File,
        file_name: String,
    ) -> Value {
        let file_size = match file.metadata().await {
            Ok(m) => m.len(),
            Err(e) => return json!({ "error": format!("Failed to get metadata: {}", e) }),
        };

        if file_size == 0 {
            return json!({ "error": "Empty file" });
        }

        let stream = ReaderStream::new(file);
        let body = Body::wrap_stream(stream);

        let client = match crate::http_client_builder().build() {
            Ok(c) => c,
            Err(e) => return json!({ "error": format!("Client build failed: {}", e) }),
        };

        let response = match client
            .post(upload_url)
            .header("Content-Disposition", format!("attachment; filename={}", file_name))
            .header("Content-Length", file_size)
            .header("Content-Range", format!("0-{}/{}", file_size - 1, file_size))
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return json!({ "error": format!("Upload failed: {}", e) }),
        };

        if !response.status().is_success() {
            return json!({
                "error": format!("Upload failed with status {}", response.status())
            });
        }

        json!({
            "fileId": file_id
        })
    }
}
