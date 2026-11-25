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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, alias = "bypass_domains")]
    pub bypass_hosts: Vec<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            protocol: ProxyProtocol::Direct,
            host: String::new(),
            port: 0,
            username: None,
            password: None,
            bypass_hosts: Vec::new(),
        }
    }
}

fn strip_host_port(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        if let Some(end_bracket) = trimmed.find(']') {
            &trimmed[1..end_bracket]
        } else {
            trimmed.trim_start_matches('[').trim_end_matches(']')
        }
    } else if let Some(idx) = trimmed.rfind(':') {
        if trimmed.find(':') == Some(idx) {
            &trimmed[..idx]
        } else {
            trimmed
        }
    } else {
        trimmed
    }
}

fn normalize_bypass_pattern(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "*" {
        return Some("*".to_string());
    }
    if trimmed.contains(',') {
        return None;
    }
    let host_no_port = strip_host_port(trimmed);
    let stripped = host_no_port
        .strip_prefix("*.")
        .or_else(|| host_no_port.strip_prefix('.'))
        .unwrap_or(host_no_port)
        .trim();
    if stripped.is_empty() || stripped == "*" || stripped.starts_with('.') {
        None
    } else {
        Some(stripped.to_lowercase())
    }
}

fn format_host_port(host: &str, port: u16) -> String {
    let host_trimmed = host.trim();
    if host_trimmed.contains(':') && !host_trimmed.starts_with('[') {
        format!("[{}]:{}", host_trimmed, port)
    } else {
        format!("{}:{}", host_trimmed, port)
    }
}

impl ProxyConfig {
    pub fn is_enabled(&self) -> bool {
        self.protocol != ProxyProtocol::Direct && !self.host.trim().is_empty() && self.port > 0
    }

    pub fn bypasses_all(&self) -> bool {
        self.bypass_hosts.iter().any(|h| h.trim() == "*")
    }

    pub fn should_bypass(&self, target_host: &str) -> bool {
        let target = strip_host_port(target_host).to_lowercase();
        let target_ip: Option<std::net::IpAddr> = target.parse().ok();

        for pattern in &self.bypass_hosts {
            let p_trimmed = pattern.trim();
            if p_trimmed.is_empty() {
                continue;
            }
            if p_trimmed == "*" {
                return true;
            }
            let Some(p_final) = normalize_bypass_pattern(p_trimmed) else {
                continue;
            };

            if target == p_final {
                return true;
            }

            if let Some(t_ip) = target_ip {
                if let Ok(pattern_ip) = p_final.parse::<std::net::IpAddr>() {
                    if t_ip == pattern_ip {
                        return true;
                    }
                }
                if matches_cidr(t_ip, &p_final) {
                    return true;
                }
            }

            if target.ends_with(&format!(".{}", p_final)) {
                return true;
            }
        }
        false
    }

    pub fn no_proxy(&self) -> Option<reqwest::NoProxy> {
        if self.bypass_hosts.is_empty() {
            return None;
        }
        let normalized: Vec<String> = self
            .bypass_hosts
            .iter()
            .filter_map(|host| normalize_bypass_pattern(host))
            .collect();

        if normalized.is_empty() {
            None
        } else {
            let bypass_str = normalized.join(",");
            reqwest::NoProxy::from_string(&bypass_str)
        }
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
            url::form_urlencoded::byte_serialize(s.as_bytes())
                .collect::<String>()
                .replace('+', "%20")
        };

        let auth_part = match (&self.username, &self.password) {
            (Some(u), Some(p)) if !u.is_empty() => {
                format!("{}:{}@", encode_auth(u), encode_auth(p))
            }
            (Some(u), None) if !u.is_empty() => {
                format!("{}:@", encode_auth(u))
            }
            _ => String::new(),
        };

        let host = self.host.trim();
        if host.is_empty()
            || host.contains('/')
            || host.contains('?')
            || host.contains('#')
            || host.contains(' ')
        {
            return None;
        }

        let (is_bracketed, unbracketed) = if host.starts_with('[') && host.ends_with(']') && host.len() >= 2 {
            (true, &host[1..host.len() - 1])
        } else {
            (false, host)
        };

        let formatted_host = if let Ok(ip) = unbracketed.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(v4) => {
                    if is_bracketed {
                        return None;
                    }
                    v4.to_string()
                }
                std::net::IpAddr::V6(v6) => format!("[{}]", v6),
            }
        } else {
            if is_bracketed || host.contains(':') || host.contains('[') || host.contains(']') {
                return None;
            }
            match url::Host::parse(host) {
                Ok(url::Host::Domain(domain)) => domain,
                _ => return None,
            }
        };

        Some(format!("{}://{}{}:{}", scheme, auth_part, formatted_host, self.port))
    }
}

fn matches_cidr(target_ip: std::net::IpAddr, cidr: &str) -> bool {
    let Some((addr_str, prefix_str)) = cidr.split_once('/') else {
        return false;
    };
    let Ok(net_ip) = addr_str.trim().parse::<std::net::IpAddr>() else {
        return false;
    };
    let Ok(prefix) = prefix_str.trim().parse::<u32>() else {
        return false;
    };
    match (target_ip, net_ip) {
        (std::net::IpAddr::V4(t), std::net::IpAddr::V4(n)) if prefix <= 32 => {
            let mask = if prefix == 0 { 0u32 } else { !0u32 << (32 - prefix) };
            (u32::from(t) & mask) == (u32::from(n) & mask)
        }
        (std::net::IpAddr::V6(t), std::net::IpAddr::V6(n)) if prefix <= 128 => {
            let mask = if prefix == 0 { 0u128 } else { !0u128 << (128 - prefix) };
            (u128::from(t) & mask) == (u128::from(n) & mask)
        }
        _ => false,
    }
}

pub async fn open_proxied_stream(
    proxy: Option<&ProxyConfig>,
    target_host: &str,
    target_port: u16,
) -> ClientResult<TcpStream> {
    let target_host = target_host.trim();
    if target_host.is_empty()
        || target_host.contains('\r')
        || target_host.contains('\n')
        || target_host.contains(' ')
    {
        return Err(Error::ConnectionFailed("Invalid target host".into()));
    }

    timeout(Duration::from_secs(20), async {
        match proxy {
            Some(p) if p.protocol != ProxyProtocol::Direct && !p.should_bypass(target_host) => {
                if !p.is_enabled() {
                    return Err(Error::ConnectionFailed("Invalid proxy configuration: host is empty or port is 0".into()));
                }
                match p.protocol {
                    ProxyProtocol::Direct => unreachable!(),
                    ProxyProtocol::HttpConnect => connect_http_connect(p, target_host, target_port).await,
                    ProxyProtocol::Socks5 => connect_socks5(p, target_host, target_port).await,
                }
            }
            _ => connect_direct(target_host, target_port).await,
        }
    })
    .await
    .map_err(|_| Error::ConnectionFailed("Proxy tunnel connection/handshake timed out".into()))?
}

async fn connect_direct(host: &str, port: u16) -> ClientResult<TcpStream> {
    let addr = format_host_port(host, port);
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
    let proxy_addr = format_host_port(&proxy.host, proxy.port);
    let mut stream = timeout(Duration::from_secs(15), TcpStream::connect(&proxy_addr))
        .await
        .map_err(|_| Error::ConnectionFailed("HTTP proxy connection timeout".into()))?
        .map_err(|e| Error::ConnectionFailed(format!("HTTP proxy failed: {}", e)))?;
    let _ = stream.set_nodelay(true);

    let target_host_header = if target_host.contains(':') && !target_host.starts_with('[') {
        format!("[{}]", target_host)
    } else {
        target_host.to_string()
    };
    let mut request = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\nProxy-Connection: Keep-Alive\r\n",
        target_host_header, target_port, target_host_header, target_port
    );

    if let Some(ref u) = proxy.username {
        if !u.is_empty() {
            let p = proxy.password.as_deref().unwrap_or("");
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

    let mut total_headers_read = 0usize;
    let mut response_buf = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        let end_pos = loop {
            if let Some(pos) = response_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos;
            }
            if total_headers_read >= 8192 {
                return Err(Error::ConnectionFailed(
                    "HTTP proxy response headers exceeded buffer limit without end-of-headers".into(),
                ));
            }
            let n = stream
                .read(&mut chunk)
                .await
                .map_err(|e| Error::ConnectionFailed(format!("Failed to read proxy response: {}", e)))?;
            if n == 0 {
                return Err(Error::ConnectionFailed("Proxy closed connection unexpectedly".into()));
            }
            total_headers_read += n;
            response_buf.extend_from_slice(&chunk[..n]);
        };

        let header_block = &response_buf[..end_pos + 4];
        let response_str = String::from_utf8_lossy(header_block);
        let first_line = response_str.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let http_version = parts.next().unwrap_or("");
        let status_code = parts
            .next()
            .and_then(|c| if c.len() == 3 { c.parse::<u16>().ok() } else { None })
            .unwrap_or(0);

        if !http_version.starts_with("HTTP/") || !(100..=599).contains(&status_code) {
            return Err(Error::ConnectionFailed(format!(
                "Invalid HTTP proxy response status line: {}",
                first_line
            )));
        }

        if (100..=199).contains(&status_code) {
            response_buf.drain(..end_pos + 4);
            continue;
        }

        if !(200..=299).contains(&status_code) {
            return Err(Error::ConnectionFailed(format!(
                "HTTP proxy error: {}",
                first_line
            )));
        }

        if response_buf.len() > end_pos + 4 {
            log::warn!(
                "HTTP proxy sent {} unexpected trailing bytes after CONNECT response",
                response_buf.len() - (end_pos + 4)
            );
        }

        break;
    }

    Ok(stream)
}

async fn connect_socks5(
    proxy: &ProxyConfig,
    target_host: &str,
    target_port: u16,
) -> ClientResult<TcpStream> {
    let proxy_addr = format_host_port(&proxy.host, proxy.port);
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
        stream.write_all(&[0x05, 0x01, 0x02]).await
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
            if user.is_empty() || user.len() > 255 || pass.len() > 255 {
                return Err(Error::ConnectionFailed(
                    "SOCKS5 username must be 1..=255 bytes and password at most 255 bytes".into(),
                ));
            }
            let mut auth_req = Vec::with_capacity(3 + user.len() + pass.len());
            auth_req.push(0x01);
            auth_req.push(user.len() as u8);
            auth_req.extend_from_slice(user.as_bytes());
            auth_req.push(pass.len() as u8);
            auth_req.extend_from_slice(pass.as_bytes());

            stream
                .write_all(&auth_req)
                .await
                .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 auth write failed: {}", e)))?;

            let mut auth_resp = [0u8; 2];
            stream
                .read_exact(&mut auth_resp)
                .await
                .map_err(|e| Error::ConnectionFailed(format!("SOCKS5 auth read failed: {}", e)))?;

            if auth_resp[0] != 0x01 || auth_resp[1] != 0x00 {
                return Err(Error::ConnectionFailed(format!(
                    "SOCKS5 authentication failed (ver: {:#x}, status: {:#x})",
                    auth_resp[0], auth_resp[1]
                )));
            }
        }
        0xFF => return Err(Error::ConnectionFailed("SOCKS5 no acceptable authentication methods".into())),
        other => return Err(Error::ConnectionFailed(format!("Unsupported SOCKS5 method: {:#x}", other))),
    }

    let clean_host = target_host.trim().trim_start_matches('[').trim_end_matches(']');
    let mut connect_req = Vec::with_capacity(22);
    connect_req.extend_from_slice(&[0x05, 0x01, 0x00]);

    if let Ok(ip) = clean_host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                connect_req.push(0x01);
                connect_req.extend_from_slice(&v4.octets());
            }
            std::net::IpAddr::V6(v6) => {
                connect_req.push(0x04);
                connect_req.extend_from_slice(&v6.octets());
            }
        }
    } else {
        let host_bytes = clean_host.as_bytes();
        if host_bytes.is_empty() || host_bytes.len() > 255 {
            return Err(Error::ConnectionFailed(
                "SOCKS5 target host domain name must be between 1 and 255 bytes".into(),
            ));
        }
        connect_req.push(0x03);
        connect_req.push(host_bytes.len() as u8);
        connect_req.extend_from_slice(host_bytes);
    }
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
