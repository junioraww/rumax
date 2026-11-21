use crate::errors::{ClientResult, Error};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocol {
    #[serde(alias = "none")]
    Direct,
    Socks5,
    #[serde(alias = "http")]
    HttpConnect,
}

impl Default for ProxyProtocol {
    fn default() -> Self {
        Self::Direct
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    #[serde(default, alias = "proxy_type")]
    pub protocol: ProxyProtocol,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub proxy_maxplus_site: bool,
}

fn default_true() -> bool {
    true
}

impl ProxyConfig {
    pub fn is_enabled(&self) -> bool {
        self.protocol != ProxyProtocol::Direct && !self.host.trim().is_empty() && self.port > 0
    }

    pub fn to_connection_uri(&self) -> Option<String> {
        if !self.is_enabled() {
            return None;
        }

        let scheme = match self.protocol {
            ProxyProtocol::Direct => return None,
            ProxyProtocol::Socks5 => "socks5h",
            ProxyProtocol::HttpConnect => "http",
        };

        let encode_auth = |s: &str| -> String {
            url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
        };

        let auth_part = match (&self.username, &self.password) {
            (Some(u), Some(p)) if !u.is_empty() => {
                format!("{}:{}@", encode_auth(u), encode_auth(p))
            }
            (Some(u), None) if !u.is_empty() => {
                format!("{}@", encode_auth(u))
            }
            _ => String::new(),
        };

        Some(format!("{}://{}{}:{}", scheme, auth_part, self.host.trim(), self.port))
    }

    pub fn to_url(&self) -> Option<String> {
        self.to_connection_uri()
    }
}

pub async fn open_proxied_stream(
    proxy: Option<&ProxyConfig>,
    target_host: &str,
    target_port: u16,
) -> ClientResult<TcpStream> {
    match proxy {
        Some(p) if p.is_enabled() => match p.protocol {
            ProxyProtocol::Direct => connect_direct(target_host, target_port).await,
            ProxyProtocol::HttpConnect => connect_http_connect(p, target_host, target_port).await,
            ProxyProtocol::Socks5 => connect_socks5(p, target_host, target_port).await,
        },
        _ => connect_direct(target_host, target_port).await,
    }
}

pub async fn connect_stream(
    proxy: Option<&ProxyConfig>,
    target_host: &str,
    target_port: u16,
) -> ClientResult<TcpStream> {
    open_proxied_stream(proxy, target_host, target_port).await
}

async fn connect_direct(host: &str, port: u16) -> ClientResult<TcpStream> {
    let addr = format!("{}:{}", host, port);
    let stream = timeout(Duration::from_secs(15), TcpStream::connect(&addr))
        .await
        .map_err(|_| Error::ConnectionFailed("Direct TCP connection timeout".into()))?
        .map_err(|e| Error::ConnectionFailed(format!("Direct TCP failed: {}", e)))?;
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

async fn connect_http_connect(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> ClientResult<TcpStream> {
    let proxy_addr = format!("{}:{}", proxy.host.trim(), proxy.port);
    let mut stream = timeout(Duration::from_secs(15), TcpStream::connect(&proxy_addr))
        .await
        .map_err(|_| Error::ConnectionFailed("HTTP proxy connection timeout".into()))?
        .map_err(|e| Error::ConnectionFailed(format!("HTTP proxy failed: {}", e)))?;
    let _ = stream.set_nodelay(true);

    let mut request = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\nProxy-Connection: Keep-Alive\r\n",
        target_host, target_port, target_host, target_port
    );

    if let (Some(u), Some(p)) = (&proxy.username, &proxy.password) {
        if !u.is_empty() {
            let creds = format!("{}:{}", u, p);
            let encoded = BASE64_STANDARD.encode(creds.as_bytes());
            request.push_str(&format!("Proxy-Authorization: Basic {}\r\n", encoded));
        }
    }
    request.push_str("\r\n");

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| Error::ConnectionFailed(format!("Failed to write CONNECT request: {}", e)))?;

    let mut response_buf = [0u8; 1024];
    let mut total_read = 0;
    while total_read < response_buf.len() {
        let n = stream
            .read(&mut response_buf[total_read..])
            .await
            .map_err(|e| Error::ConnectionFailed(format!("Failed to read proxy response: {}", e)))?;
        if n == 0 {
            return Err(Error::ConnectionFailed("Proxy closed connection unexpectedly".into()));
        }
        total_read += n;
        if response_buf[..total_read].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let response_str = String::from_utf8_lossy(&response_buf[..total_read]);
    let first_line = response_str.lines().next().unwrap_or("");
    let status_code = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);

    if status_code != 200 {
        return Err(Error::ConnectionFailed(format!(
            "HTTP proxy error: {}",
            first_line
        )));
    }

    Ok(stream)
}

async fn connect_socks5(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> ClientResult<TcpStream> {
    let proxy_addr = format!("{}:{}", proxy.host.trim(), proxy.port);
    let mut stream = timeout(Duration::from_secs(15), TcpStream::connect(&proxy_addr))
        .await
        .map_err(|_| Error::ConnectionFailed("SOCKS5 proxy connection timeout".into()))?
        .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 proxy failed: {}", e)))?;
    let _ = stream.set_nodelay(true);

    let has_auth = proxy
        .username
        .as_ref()
        .map(|u| !u.is_empty())
        .unwrap_or(false);

    if has_auth {
        stream.write_all(&[0x05, 0x02, 0x00, 0x02]).await
    } else {
        stream.write_all(&[0x05, 0x01, 0x00]).await
    }
    .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 handshake failed: {}", e)))?;

    let mut method_buf = [0u8; 2];
    stream
        .read_exact(&mut method_buf)
        .await
        .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 method selection read failed: {}", e)))?;

    if method_buf[0] != 0x05 {
        return Err(Error::ConnectionFailed("Invalid SOCKS version".into()));
    }

    match method_buf[1] {
        0x00 => {}
        0x02 => {
            let user = proxy.username.as_deref().unwrap_or("");
            let pass = proxy.password.as_deref().unwrap_or("");
            let mut auth_req = Vec::with_capacity(3 + user.len() + pass.len());
            auth_req.push(0x01);
            auth_req.push(user.len().min(255) as u8);
            auth_req.extend_from_slice(&user.as_bytes()[..user.len().min(255)]);
            auth_req.push(pass.len().min(255) as u8);
            auth_req.extend_from_slice(&pass.as_bytes()[..pass.len().min(255)]);

            stream
                .write_all(&auth_req)
                .await
                .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 auth write failed: {}", e)))?;

            let mut auth_resp = [0u8; 2];
            stream
                .read_exact(&mut auth_resp)
                .await
                .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 auth read failed: {}", e)))?;

            if auth_resp[1] != 0x00 {
                return Err(Error::ConnectionFailed("SOCKS5 authentication failed".into()));
            }
        }
        0xFF => return Err(Error::ConnectionFailed("SOCKS5 no acceptable authentication methods".into())),
        other => return Err(Error::ConnectionFailed(format!("Unsupported SOCKS5 method: {:#x}", other))),
    }

    let host_bytes = target_host.as_bytes();
    let host_len = host_bytes.len().min(255) as u8;
    let mut connect_req = Vec::with_capacity(7 + host_len as usize);
    connect_req.extend_from_slice(&[0x05, 0x01, 0x00, 0x03, host_len]);
    connect_req.extend_from_slice(&host_bytes[..host_len as usize]);
    connect_req.extend_from_slice(&target_port.to_be_bytes());

    stream
        .write_all(&connect_req)
        .await
        .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 connect write failed: {}", e)))?;

    let mut resp_header = [0u8; 4];
    stream
        .read_exact(&mut resp_header)
        .await
        .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 connect response failed: {}", e)))?;

    if resp_header[0] != 0x05 {
        return Err(Error::ConnectionFailed("Invalid SOCKS version in connect response".into()));
    }

    if resp_header[1] != 0x00 {
        return Err(Error::ConnectionFailed(format!(
            "SOCKS5 server returned error code: {}",
            resp_header[1]
        )));
    }

    match resp_header[3] {
        0x01 => {
            let mut buf = [0u8; 6];
            stream.read_exact(&mut buf).await.map_err(|e| {
                Error::ConnectionFailed(format!("Failed to read IPv4 bound address: {}", e))
            })?;
        }
        0x03 => {
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await.map_err(|e| {
                Error::ConnectionFailed(format!("Failed to read domain bound len: {}", e))
            })?;
            let mut domain_buf = vec![0u8; len_buf[0] as usize + 2];
            stream.read_exact(&mut domain_buf).await.map_err(|e| {
                Error::ConnectionFailed(format!("Failed to read domain bound address: {}", e))
            })?;
        }
        0x04 => {
            let mut buf = [0u8; 18];
            stream.read_exact(&mut buf).await.map_err(|e| {
                Error::ConnectionFailed(format!("Failed to read IPv6 bound address: {}", e))
            })?;
        }
        other => {
            return Err(Error::ConnectionFailed(format!(
                "Unknown address type in SOCKS5 response: {}",
                other
            )));
        }
    }

    Ok(stream)
}
