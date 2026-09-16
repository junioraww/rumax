use super::{TransportFactory, TransportReader, TransportWriter};
use crate::models::{Request, Response};
use crate::errors::{ClientResult, Error};

use async_trait::async_trait;
use bytes::{BufMut, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{sync::Arc};
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::pki_types::pem::PemObject;
use rustls::{ClientConfig, RootCertStore};


use serde_json::{Map, Value as JsonValue};
use rmpv::{Value as MsgPackValue};
use rmpv::decode::read_value;

pub enum MobileStream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl AsyncRead for MobileStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Self::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MobileStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Self::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
            Self::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Self::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

fn msgpack_to_json(val: MsgPackValue) -> JsonValue {
    match val {
        MsgPackValue::Nil => JsonValue::Null,
        MsgPackValue::Boolean(b) => JsonValue::Bool(b),
        MsgPackValue::Integer(i) => {
            const MAX_SAFE_INT: u64 = 9_007_199_254_740_991;
            if let Some(v) = i.as_u64() {
                if v > MAX_SAFE_INT {
                    JsonValue::String(v.to_string())
                } else {
                    JsonValue::Number(v.into())
                }
            }
            else if let Some(v) = i.as_i64() {
                 if v < -(MAX_SAFE_INT as i64) {
                     JsonValue::String(v.to_string())
                 } else {
                     JsonValue::Number(v.into())
                 }
            } 
            else {
                JsonValue::Null
            }
        }
        MsgPackValue::F32(f) => JsonValue::Number(serde_json::Number::from_f64(f as f64).unwrap_or_else(|| 0.into())),
        MsgPackValue::F64(f) => JsonValue::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into())),
        MsgPackValue::String(s) => JsonValue::String(s.into_str().unwrap_or_default()),
        MsgPackValue::Binary(b) => JsonValue::String(String::from_utf8_lossy(&b).to_string()),
        MsgPackValue::Array(vec) => {
            JsonValue::Array(vec.into_iter().map(msgpack_to_json).collect())
        },
        MsgPackValue::Map(vec) => {
            let mut map = Map::new();
            for (k, v) in vec {
                let key_str = match k {
                    MsgPackValue::String(s) => s.into_str().unwrap_or_default(),
                    MsgPackValue::Integer(i) => i.to_string(),
                    MsgPackValue::Boolean(b) => b.to_string(),
                    _ => "unknown".to_string(),
                };
                map.insert(key_str, msgpack_to_json(v));
            }
            JsonValue::Object(map)
        }
        MsgPackValue::Ext(_, _) => JsonValue::Null,
    }
}

pub struct MobileTransport {
    stream: MobileStream,
}

const DEFAULT_ROOT_CA: &[u8] = include_bytes!("../../assets/rootca_ssl_rsa2022.crt");

impl MobileTransport {
    pub async fn connect_tls(
        host: &str,
        port: u16,
        use_custom_ca: bool,
        custom_cert_path: Option<&str>
    ) -> ClientResult<Self> {
        let addr = format!("{}:{}", host, port);
        let tcp = TcpStream::connect(&addr).await
            .map_err(|e| Error::ConnectionFailed(format!("TCP Error: {}", e)))?;
        tcp.set_nodelay(true).ok();

        let config = create_tls_config(use_custom_ca, custom_cert_path)
            .map_err(|e| Error::ConnectionFailed(format!("TLS config error: {}", e)))?;

        let connector = TlsConnector::from(Arc::new(config));
        let domain = ServerName::try_from(host.to_string())
            .map_err(|_| Error::ConnectionFailed("Invalid DNS name".into()))?;

        let tls_stream = connector.connect(domain, tcp).await
            .map_err(|e| Error::ConnectionFailed(format!("TLS Error: {}", e)))?;

        Ok(Self { stream: MobileStream::Tls(tls_stream) })
    }
}

/*fn trust_certificate(
    certs_file: &str,
    key_file: &str
) -> Result<Arc<ClientConfig>, String> {
    let certs: Vec<CertificateDer> =
    CertificateDer::pem_file_iter(certs_file)
        .unwrap()
        .map(|c| c.unwrap())
        .collect();

    let key = PrivateKeyDer::from_pem_file(key_file).unwrap();

    let mut root_store = RootCertStore::empty();
    root_store.add_parsable_certificates(certs.clone());

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(certs, key)
        .unwrap();

    Ok(Arc::new(config))
}*/

fn create_tls_config(use_custom_ca: bool, cert_path: Option<&str>) -> Result<ClientConfig, String> {
    let mut root_store = RootCertStore::empty();

    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if use_custom_ca {
        let certs: Vec<CertificateDer> = if let Some(path) = cert_path {
            CertificateDer::pem_file_iter(path)
                .map_err(|e| format!("Не удалось открыть файл {}: {}", path, e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Ошибка парсинга PEM файла: {}", e))?
        } else {
            rustls_pki_types::CertificateDer::pem_slice_iter(DEFAULT_ROOT_CA)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Ошибка парсинга встроенного PEM: {}", e))?
        };

        for cert in certs {
            root_store.add(cert).map_err(|e| format!("Ошибка добавления CA: {}", e))?;
        }
    }

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(config)
}

impl TransportFactory for MobileTransport {
    type Reader = MobileReader;
    type Writer = MobileWriter;

    fn split(self) -> (Self::Writer, Self::Reader) {
        let (reader, writer) = tokio::io::split(self.stream);
        (MobileWriter { writer }, MobileReader { reader })
    }
}

pub struct MobileWriter {
    writer: tokio::io::WriteHalf<MobileStream>,
}

fn json_to_msgpack(val: &JsonValue) -> MsgPackValue {
    match val {
        JsonValue::Null => MsgPackValue::Nil,
        JsonValue::Bool(b) => MsgPackValue::Boolean(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                MsgPackValue::Integer(i.into())
            } else if let Some(u) = n.as_u64() {
                MsgPackValue::Integer(u.into())
            } else if let Some(f) = n.as_f64() {
                MsgPackValue::F64(f)
            } else {
                MsgPackValue::Nil
            }
        }
        JsonValue::String(s) => MsgPackValue::String(s.as_str().into()),
        JsonValue::Array(arr) => {
            MsgPackValue::Array(arr.iter().map(json_to_msgpack).collect())
        }
        JsonValue::Object(obj) => {
            let mut map = Vec::new();
            for (k, v) in obj {
                // TODO this sucks
                if (k == "mode" || k == "chatCacheFingerprint" || k == "chatsCountGroups") && v.is_array() {
                    let arr = v.as_array().unwrap();
                    let bytes: Vec<u8> = arr
                        .iter()
                        .filter_map(|x| x.as_u64().map(|n| n as u8))
                        .collect();

                    map.push((
                        MsgPackValue::String(k.as_str().into()),
                        MsgPackValue::Binary(bytes),
                    ));
                    continue;
                }

                map.push((
                    MsgPackValue::String(k.as_str().into()),
                          json_to_msgpack(v),
                ));
            }
            MsgPackValue::Map(map)
        }
    }
}

#[async_trait]
impl TransportWriter for MobileWriter {
    async fn send(&mut self, request: Request) -> ClientResult<()> {
        let mp_payload = json_to_msgpack(&request.payload);

        let mut payload_bytes = Vec::new();
            rmpv::encode::write_value(&mut payload_bytes, &mp_payload)
            .map_err(|e| Error::SendFailed(format!("MsgPack encode error: {}", e)))?;

        let payload_len = payload_bytes.len();
        if payload_len > 0xFFFFFF {
            return Err(Error::SendFailed("Payload too large".into()));
        }

        let mut buffer = BytesMut::with_capacity(10 + payload_len);
        buffer.put_u8(request.ver as u8);
        buffer.put_u8(request.cmd as u8);
        buffer.put_u16(request.seq as u16);
        buffer.put_u16(request.opcode);
        
        let packed_len = (payload_len as u32) & 0xFFFFFF; 
        buffer.put_u32(packed_len);
        buffer.put_slice(&payload_bytes);

        self.writer.write_all(&buffer).await
            .map_err(|e| Error::SendFailed(e.to_string()))?;
        Ok(())
    }
}

pub struct MobileReader {
    reader: tokio::io::ReadHalf<MobileStream>,
}

#[async_trait]
impl TransportReader for MobileReader {
    async fn next_message(&mut self) -> ClientResult<Option<Response>> {
        let mut header = [0u8; 10];
        match self.reader.read_exact(&mut header).await {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(Error::ConnectionFailed(e.to_string())),
        };

        let ver = header[0];
        let cmd = header[1] as u16;
        let seq = u16::from_be_bytes([header[2], header[3]]) as u64;
        let opcode = u16::from_be_bytes([header[4], header[5]]);
        let packed_len_raw = u32::from_be_bytes([header[6], header[7], header[8], header[9]]);

        let comp_flag = packed_len_raw >> 24;
        let payload_len = (packed_len_raw & 0xFFFFFF) as usize;

        if payload_len == 0 {
            return Ok(Some(Response {
                ver: ver, cmd: cmd as u8, seq, opcode, payload: JsonValue::Null,
            }));
        }
        
        let mut payload_buf = vec![0u8; payload_len];
        self.reader.read_exact(&mut payload_buf).await
            .map_err(|e| Error::ConnectionFailed(format!("Payload read err: {}", e)))?;
        
        let final_payload_bytes = if comp_flag != 0 {
            let max_size = 5 * 1024 * 1024; 
            lz4_flex::block::decompress(&payload_buf, max_size)
                .map_err(|e| Error::ApiResponse(serde_json::json!({"error": "LZ4 error", "details": e.to_string()})))?
        } else {
            payload_buf
        };
        
        if final_payload_bytes.is_empty() {
             return Ok(Some(Response {
                ver: ver, cmd: cmd as u8, seq, opcode, payload: JsonValue::Null,
            }));
        }
        
        let mut buf = &final_payload_bytes[..];
        
        let mp_value = read_value(&mut buf)
            .map_err(|e| Error::ApiResponse(serde_json::json!({
                "error": "MsgPack decode error (rmpv)", 
                "details": e.to_string()
            })))?;
        
        let payload_value = msgpack_to_json(mp_value);
        
        Ok(Some(Response {
            ver: ver,
            cmd: cmd as u8,
            seq,
            opcode,
            payload: payload_value,
        }))
    }
}
