use rumax::MaxClient;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn upload_headers(filename: &str) -> HashMap<String, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/upload", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.ends_with(b"sample") {
            let read = socket.read(&mut buffer).await.unwrap();
            assert_ne!(read, 0, "connection closed before the upload body arrived");
            request.extend_from_slice(&buffer[..read]);
        }
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
        request
    });

    let path = std::env::temp_dir().join(format!("rumax-upload-{}", rand::random::<u64>()));
    tokio::fs::write(&path, b"sample").await.unwrap();
    let file = tokio::fs::File::open(&path).await.unwrap();
    let uploaded = tokio::time::timeout(
        Duration::from_secs(5),
        MaxClient::new().upload_file(url, 42, file, filename.to_owned()),
    )
    .await;
    tokio::fs::remove_file(path).await.unwrap();

    assert_eq!(uploaded.unwrap(), serde_json::json!({"fileId": 42}));
    let request = String::from_utf8(server.await.unwrap()).unwrap();
    request
        .split_once("\r\n\r\n")
        .unwrap()
        .0
        .lines()
        .skip(1)
        .map(|line| {
            let (name, value) = line.split_once(':').unwrap();
            (name.to_ascii_lowercase(), value.trim_start().to_owned())
        })
        .collect()
}

#[tokio::test]
async fn upload_encodes_unicode_filename_in_request_headers() {
    let headers = upload_headers("тест файл.txt").await;
    let name = "%D1%82%D0%B5%D1%81%D1%82%20%D1%84%D0%B0%D0%B9%D0%BB.txt";
    assert_eq!(
        headers["content-disposition"],
        format!("attachment; filename=\"{name}\"")
    );
    assert!(!headers.contains_key("x-file-name"));
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert_eq!(headers["content-range"], "bytes 0-5/6");
}

#[tokio::test]
async fn upload_encodes_ascii_spaces_and_keeps_empty_name_fallback() {
    for (filename, expected) in [("report Q1.txt", "report%20Q1.txt"), ("", "file")] {
        let headers = upload_headers(filename).await;
        assert_eq!(
            headers["content-disposition"],
            format!("attachment; filename=\"{expected}\"")
        );
        assert!(!headers.contains_key("x-file-name"));
        assert_eq!(headers["content-range"], "bytes 0-5/6");
    }
}
