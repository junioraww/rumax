use log::{debug, error, info, trace, warn};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use chrono::Utc;
use http::header::HeaderValue;
use serde_json::{json, Value};
use tokio::{
    sync::{broadcast, oneshot, Mutex as TokioMutex},
    time::timeout,
};
use yawc::{CompressionLevel, HttpRequestBuilder, Options, WebSocket};
use rustls::crypto::ring;

pub mod api;
pub mod constants;
pub mod errors;
pub mod models;
pub mod navigation;

pub mod transport;

use transport::{
    TransportFactory, 
    TransportReader, 
    TransportWriter,
    web::WebTransport,
    mobile::MobileTransport
};

use constants::Constants;
pub use errors::{ClientResult, Error};
use models::{Request, Response, Identity};

struct ClientState {
    writer: Option<Box<dyn TransportWriter>>,
    seq: u64,
    temp_token: Option<String>,
    token: Option<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<ClientResult<Response>>>>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
    user_id: Option<u64>,
    action_id: u64,
    session_id: i64,
    identity: Option<Identity>,
    current_screen: String,
    is_closed: bool,
    mobile_port: u16,
    mobile_host: String,
    use_custom_ca: bool,
    custom_ca_path: Option<String>,
}

pub enum ClientMode {
    Web,
    Mobile,
}

#[derive(Clone)]
pub struct MaxClient {
    state: Arc<TokioMutex<ClientState>>,
    event_tx: broadcast::Sender<Value>,
}

impl MaxClient {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (event_tx, _) = broadcast::channel(20);
        MaxClient {
            state: Arc::new(TokioMutex::new(ClientState {
                writer: None,
                seq: 0,
                temp_token: None,
                token: None,
                pending: Arc::new(Mutex::new(HashMap::new())),
                shutdown_tx: Some(shutdown_tx),
                user_id: None,
                action_id: 0,
                session_id: Utc::now().timestamp_millis(),
                identity: None,
                current_screen: "chats_list_tab".to_string(),
                is_closed: true,
                mobile_host: Constants::MOBILE_HOST.to_string(),
                mobile_port: Constants::MOBILE_PORT,
                use_custom_ca: true,
                custom_ca_path: None,
            })),
            event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.event_tx.subscribe()
    }
    
    pub async fn is_connected(&self) -> bool {
        self.state.lock().await.writer.is_some()
    }
    
    pub async fn set_user_id(&self, user_id: u64) {
        self.state.lock().await.user_id = Some(user_id);
    }
    
    pub async fn get_token(&self) -> Option<String> {
        self.state.lock().await.token.clone()
    }

    /*
     * TODO move to other place
     */

    pub async fn set_host(&self, address: String, port: u16) {
        let mut state = self.state.lock().await;
        state.mobile_host = address;
        state.mobile_port = port;
    }

    pub async fn set_custom_ca(&self, enabled: bool, custom_path: Option<String>) {
        let mut state = self.state.lock().await;
        state.use_custom_ca = enabled;
        state.custom_ca_path = custom_path;
    }

    pub async fn enable_custom_ca(&self, enabled: bool) {
        self.state.lock().await.use_custom_ca = enabled;
    }
    
    pub async fn connect(&self, identity: Identity, is_mobile: bool) -> ClientResult<Response> {
        let _ = ring::default_provider().install_default();

        let state_clone = Arc::clone(&self.state);
        let mut state_lock = state_clone.lock().await;

        state_lock.identity = Some(identity.clone());

        let (writer, reader): (Box<dyn TransportWriter>, Box<dyn TransportReader>) = if is_mobile {
            info!("Подключение Mobile TCP/TLS...");

            let transport = MobileTransport::connect_tls(
                &state_lock.mobile_host,
                state_lock.mobile_port,
                state_lock.use_custom_ca,
                state_lock.custom_ca_path.as_deref()
            ).await?;

            let (w, r) = transport.split();
            (Box::new(w), Box::new(r))
        } else {
            info!("Подключение к WebSocket...");
            let req_builder = HttpRequestBuilder::new()
                .header("Origin", HeaderValue::from_static(Constants::ORIGIN_HEADER))
                .header("User-Agent", HeaderValue::from_static(Constants::USER_AGENT));

            let ws = WebSocket::connect(Constants::WEBSOCKET_URI.parse().unwrap())
                .with_options(Options::default().with_compression_level(CompressionLevel::fast()))
                .with_request(req_builder)
                .await
                .map_err(|e| Error::ConnectionFailed(e.to_string()))?;

            info!("WebSocket подключен.");
            let transport = WebTransport::new(ws);
            let (w, r) = transport.split();
            (Box::new(w), Box::new(r))
        };

        info!("Разделение потоков и запуск задач...");

        state_lock.is_closed = false;
        
        let pending_clone = Arc::clone(&state_lock.pending);
        let (shutdown_tx, shutdown_rx_read) = broadcast::channel(1);
        let shutdown_rx_ping = shutdown_tx.subscribe();
        let event_tx = self.event_tx.clone();

        state_lock.session_id = Utc::now().timestamp_millis();

        tokio::spawn(Self::read_task(reader, pending_clone, event_tx, shutdown_rx_read, Arc::clone(&self.state)));
        debug!("Задача чтения (read_task) запущена.");
        
        let ping_client = self.clone();
        tokio::spawn(Self::ping_task(ping_client, shutdown_rx_ping));
        debug!("Задача пинга (ping_task) запущена.");
        
        state_lock.writer = Some(writer);
        state_lock.shutdown_tx = Some(shutdown_tx);
        
        drop(state_lock);
        
        debug!("Отправка Handshake");
        
        let handshake_payload = if is_mobile {
            json!({
                "clientSessionId": 1,
                "mt_instanceid": identity.mt_instance,
                "userAgent": identity.user_agent,
                "deviceId": identity.device_id,
            })
        } else {
            json!({
                "deviceId": identity.device_id,
                "userAgent": identity.user_agent,
            })
        };

        self.send_and_wait(6, handshake_payload, 0).await
    }
    
    pub async fn disconnect(&self) {
        let mut state = self.state.lock().await;
        state.is_closed = true;
        
        if let Some(shutdown_tx) = state.shutdown_tx.take() {
            let _ = shutdown_tx.send(()); 
        }
        
        state.writer = None;
        
        state.pending.lock().unwrap().clear();
        
        state.token = None;
        state.user_id = None;
        state.seq = 0;
        
        info!("Клиент отключен, состояние сброшено.");
    }
    
    fn send_frame(&self, request: Request) -> std::pin::Pin<Box<dyn std::future::Future<Output = ClientResult<()>> + Send>> {
        let this = self.clone();
        
        Box::pin(async move {
            /*let (should_reconnect, identity) = {
                let state = this.state.lock().await;

                if state.is_closed {
                    return Err(Error::NotConnected)
                }

                if state.writer.is_none() {
                    (true, state.identity.clone())
                } else {
                    (false, None)
                }
            };

            if should_reconnect {
                if let Some(i) = identity {
                    debug!("Реконнект внутри send_frame...");
                    this.connect(i, true).await?;
                } else {
                    return Err(Error::ConnectionFailed("Device ID not set".to_string()));
                }
            }*/

            let mut state = this.state.lock().await;
            if let Some(writer) = &mut state.writer {
                writer.send(request).await
            } else {
                Err(Error::NotConnected)
            }
        })
    }
    
    pub async fn send_and_wait(
        &self,
        opcode: u16,
        payload: serde_json::Value,
        cmd: u8,
    ) -> ClientResult<Response> {
        let (tx, rx) = oneshot::channel();
        
        let request = {
            let mut state = self.state.lock().await;
            state.seq += 1;
            let current_seq = state.seq;
            
            state.pending.lock().unwrap().insert(current_seq, tx);
            
            Request {
                ver: 10,
                cmd,
                seq: current_seq,
                opcode,
                payload,
            }
        };
        
        debug!("Отправка: {:?}", request);
        let _ = self.event_tx.send(json!({
            "type": "log",
            "request": request
        })); 
        
        self.send_frame(request.clone()).await?;
        
        match timeout(Constants::DEFAULT_TIMEOUT, rx).await {
            Ok(Ok(Ok(response))) => {
                trace!("Получен ответ для seq: {}", response.seq);
                if response.payload.get("error").is_some() {
                    Err(Error::ApiResponse(response.payload))
                } else {
                    Ok(response)
                }
            }
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(e)) => {
                error!("Ошибка получения ответа (канал закрыт) для seq: {}", request.seq);
                Err(e.into())
            }
            Err(_) => {
                warn!("Таймаут запроса для seq: {}", request.seq);
                self.state.lock().await.pending.lock().unwrap().remove(&request.seq);
                Err(Error::RequestTimeout(Constants::DEFAULT_TIMEOUT))
            }
        }
    }
    
    async fn read_task(
        mut reader: Box<dyn TransportReader>,
        pending: Arc<Mutex<HashMap<u64, oneshot::Sender<ClientResult<Response>>>>>,
        event_sender: broadcast::Sender<Value>,
        mut shutdown_rx: broadcast::Receiver<()>,
        state: Arc<TokioMutex<ClientState>>,
    ) {
        loop {
            tokio::select! {
                msg_result = reader.next_message() => {
                    match msg_result {
                        Ok(Some(resp)) => {
                            let seq = resp.seq;
                            let waiting_sender = {
                                let mut guard = pending.lock().unwrap();
                                guard.remove(&seq)
                            };
                            
                            if let Some(sender) = waiting_sender {
                                let _ = event_sender.send(json!({
                                    "type": "log",
                                    "response": &resp
                                }));
                                let _ = sender.send(Ok(resp));
                            } else {
                                let _ = event_sender.send(json!({
                                    "type": "rx",
                                    "response": resp
                                }));
                            }
                        },
                        Ok(None) => {
                            info!("Соединение закрыто (EOF)");
                            let _ = event_sender.send(json!({
                                "type": "log",
                                "response": "closed"
                            })); 
                            let mut s = state.lock().await;
                            s.writer = None;
                            let mut pending_guard = pending.lock().unwrap();
                            for (_, sender) in pending_guard.drain() {
                                let _ = sender.send(Err(Error::ConnectionClosed("Соединение закрыто".to_string())));
                            }
                            break;
                        },
                        Err(e) => {
                            error!("Ошибка чтения транспорта:\n{}", e);
                            let mut s = state.lock().await;
                            s.writer = None;
                            let mut pending_guard = pending.lock().unwrap();
                            for (_, sender) in pending_guard.drain() {
                                let _ = sender.send(Err(Error::ConnectionClosed(e.to_string()))); 
                            }
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => break,
            }
        }
    }
    
    async fn ping_task(
        client: MaxClient,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        info!("Ping task started");
        let mut interval = tokio::time::interval(Constants::PING_INTERVAL);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    debug!("Отправка Ping...");
                    match client.send_and_wait(1, json!({ "interactive": true }), 0).await {
                        Ok(_) => {
                            info!("Pong получен");
                        }
                        Err(e) => {
                            error!("Ошибка Ping: {}. Остановка ping_task", e);
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Получен сигнал завершения для ping_task");
                    break;
                }
            }
        }
        info!("Ping task finished");
    }

    pub async fn call(
        &self,
        action_id: u16,
        payload: serde_json::Value,
    ) -> ClientResult<Response> {
        self.send_and_wait(action_id, payload, 0).await
    }
    
    pub async fn set_token(&self, token: String) {
        self.state.lock().await.token = Some(token);
    }
    
    pub async fn set_temp_token(&self, token: String) {
        self.state.lock().await.temp_token = Some(token);
    }
}
