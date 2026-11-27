use log::{debug, error, info, trace, warn};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use chrono::Utc;
use http::header::HeaderValue;
use serde_json::{json, Value};
use tokio::{
    sync::{broadcast, mpsc, oneshot, Mutex as TokioMutex},
    time::timeout,
};
use yawc::{CompressionLevel, HttpRequestBuilder, Options, WebSocket};
use rustls::crypto::aws_lc_rs;
use url::Url;
use rustls_pki_types::ServerName;
use tokio_rustls::TlsConnector;

pub fn ensure_tls_provider() {
    let _ = aws_lc_rs::default_provider().install_default();
}

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static GLOBAL_PROXY_VERSION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

pub mod api;
pub mod constants;
pub mod errors;
pub mod models;
pub mod navigation;
pub mod fingerprint;

pub mod transport;

use transport::{
    TransportFactory, 
    TransportReader, 
    TransportWriter,
    web::WebTransport,
    mobile::MobileTransport
};

use fingerprint::{VersionDataProvider};
use constants::Constants;
pub use errors::{ClientResult, Error};
use models::{Request, Response};
pub use models::{Identity, UserAgent};

pub use api::telemetry::TelemetryChat;
use navigation::NavigationPlanner;

pub use api::auth::{SyncState, QrLoginSession};
pub use transport::proxy::{ProxyConfig, ProxyProtocol};

pub const MINTSIFRY_ROOT_CA: &[u8] = include_bytes!("../assets/rootca_ssl_rsa2022.crt");

struct GlobalHttpState {
    proxy: Option<ProxyConfig>,
    client: Option<reqwest::Client>,
}

static GLOBAL_HTTP_STATE: std::sync::RwLock<GlobalHttpState> = std::sync::RwLock::new(GlobalHttpState {
    proxy: None,
    client: None,
});

pub fn set_global_proxy_config(settings: Option<ProxyConfig>) {
    let mut lock = match GLOBAL_HTTP_STATE.write() {
        Ok(l) => l,
        Err(poisoned) => poisoned.into_inner(),
    };
    lock.proxy = settings;
    lock.client = None;
    GLOBAL_PROXY_VERSION.fetch_add(1, Ordering::Release);
}

pub fn get_global_proxy_config() -> Option<ProxyConfig> {
    match GLOBAL_HTTP_STATE.read() {
        Ok(lock) => lock.proxy.clone(),
        Err(poisoned) => poisoned.into_inner().proxy.clone(),
    }
}

pub fn reset_shared_http_client() {
    let mut lock = match GLOBAL_HTTP_STATE.write() {
        Ok(l) => l,
        Err(poisoned) => poisoned.into_inner(),
    };
    lock.client = None;
    GLOBAL_PROXY_VERSION.fetch_add(1, Ordering::Release);
}

pub fn http_client_builder_for(proxy_cfg: Option<&ProxyConfig>) -> reqwest::ClientBuilder {
    let mut builder = reqwest::Client::builder()
        .user_agent(Constants::WEB_USER_AGENT);
    if let Ok(cert) = reqwest::Certificate::from_pem(MINTSIFRY_ROOT_CA) {
        builder = builder.add_root_certificate(cert);
    }
    if let Some(proxy_cfg) = proxy_cfg {
        if proxy_cfg.protocol != ProxyProtocol::Direct && !proxy_cfg.bypasses_all() {
            let mut configured = false;
            if let Some(url_str) = proxy_cfg.to_connection_uri() {
                if url_str.parse::<http::Uri>().is_ok() {
                    match reqwest::Proxy::all(&url_str) {
                        Ok(mut proxy) => {
                            if let Some(no_proxy) = proxy_cfg.no_proxy() {
                                proxy = proxy.no_proxy(Some(no_proxy));
                            }
                            builder = builder.proxy(proxy);
                            configured = true;
                        }
                        Err(e) => {
                            error!("Failed to configure proxy (protocol: {:?}, host: {}, port: {}): {}", proxy_cfg.protocol, proxy_cfg.host, proxy_cfg.port, e);
                        }
                    }
                } else {
                    error!("Proxy URI is not a valid http::Uri for hyper-util: {}", url_str);
                }
            } else {
                error!("Proxy protocol is {:?} but to_connection_uri returned None (host: '{}', port: {})", proxy_cfg.protocol, proxy_cfg.host, proxy_cfg.port);
            }
            if !configured {
                warn!("Proxy configuration failed; routing through blackhole proxy to prevent direct IP leak");
                let mut blackhole = reqwest::Proxy::all("http://127.0.0.1:0")
                    .expect("static blackhole URL is always valid");
                if let Some(no_proxy) = proxy_cfg.no_proxy() {
                    blackhole = blackhole.no_proxy(Some(no_proxy));
                }
                builder = builder.proxy(blackhole);
            }
        } else {
            builder = builder.no_proxy();
        }
    }
    builder
}

pub fn http_client_builder() -> reqwest::ClientBuilder {
    let proxy = get_global_proxy_config();
    http_client_builder_for(proxy.as_ref())
}

fn build_safe_http_client(builder: reqwest::ClientBuilder) -> reqwest::Client {
    match builder.build() {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to build HTTP client ({}); falling back to blackhole proxy client", e);
            let blackhole = reqwest::Proxy::all("http://127.0.0.1:0")
                .expect("static blackhole URL is always valid");
            reqwest::Client::builder()
                .proxy(blackhole)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new())
        }
    }
}

pub fn create_http_client() -> reqwest::Client {
    build_safe_http_client(http_client_builder())
}

pub fn shared_http_client() -> reqwest::Client {
    if let Ok(lock) = GLOBAL_HTTP_STATE.read() {
        if let Some(ref client) = lock.client {
            return client.clone();
        }
    }
    let mut lock = match GLOBAL_HTTP_STATE.write() {
        Ok(l) => l,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(ref client) = lock.client {
        return client.clone();
    }
    let new_client = build_safe_http_client(http_client_builder_for(lock.proxy.as_ref()));
    lock.client = Some(new_client.clone());
    new_client
}

struct WriteCommand {
    request: Request,
    generation: u64,
    done: oneshot::Sender<ClientResult<()>>,
}

struct ClientState {
    writer_tx: Option<mpsc::Sender<WriteCommand>>,
    seq: u64,
    temp_token: Option<String>,
    token: Option<String>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<ClientResult<Response>>>>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
    user_id: Option<u64>,
    action_id: u64,
    generation: u64,
    session_id: i64,
    identity: Option<Identity>,
    #[allow(dead_code)]
    current_screen: String,
    is_closed: bool,
    is_synced: bool,
    mobile_port: u16,
    mobile_host: String,
    use_custom_ca: bool,
    custom_ca_path: Option<String>,
    calls_seed: Option<i64>,
    version_provider: VersionDataProvider,
    planner: NavigationPlanner,
    telemetry_chats: Vec<TelemetryChat>,
    telemetry_mode: String,
    interactive: bool,
    is_mobile: bool,
    custom_http_user_agent: Option<String>,
    proxy: Option<ProxyConfig>,
    http_client: Option<reqwest::Client>,
    http_client_generation: u64,
    http_client_proxy_version: u64,
    is_vpn: Option<bool>,
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
        let (event_tx, _) = broadcast::channel(64);
        MaxClient {
            state: Arc::new(TokioMutex::new(ClientState {
                writer_tx: None,
                seq: 0,
                temp_token: None,
                token: None,
                pending: Arc::new(Mutex::new(HashMap::new())),
                shutdown_tx: Some(shutdown_tx),
                user_id: None,
                action_id: 0,
                generation: next_generation(),
                session_id: Utc::now().timestamp_millis(),
                identity: None,
                current_screen: "chats_list_tab".to_string(),
                is_closed: true,
                is_synced: false,
                mobile_host: Constants::MOBILE_HOST.to_string(),
                mobile_port: Constants::MOBILE_PORT,
                use_custom_ca: true,
                custom_ca_path: None,
                calls_seed: None,
                version_provider: VersionDataProvider::new(),
                planner: NavigationPlanner::new(),
                telemetry_chats: Vec::new(),
                telemetry_mode: "fake".to_string(),
                interactive: true,
                is_mobile: true,
                custom_http_user_agent: None,
                proxy: None,
                http_client: None,
                http_client_generation: 0,
                http_client_proxy_version: 0,
                is_vpn: None,
            })),
            event_tx,
        }
    }

    pub async fn set_proxy(&self, proxy: Option<ProxyConfig>) {
        let mut state = self.state.lock().await;
        state.proxy = proxy;
        state.http_client = None;
        state.http_client_generation = state.http_client_generation.wrapping_add(1);
    }

    pub async fn get_proxy(&self) -> Option<ProxyConfig> {
        self.state.lock().await.proxy.clone().or_else(get_global_proxy_config)
    }

    pub async fn http_client_builder(&self) -> reqwest::ClientBuilder {
        let (proxy, ua) = {
            let state = self.state.lock().await;
            let proxy = state.proxy.clone().or_else(get_global_proxy_config);
            let ua = Self::effective_http_user_agent_from_state(&state);
            (proxy, ua)
        };
        http_client_builder_for(proxy.as_ref()).user_agent(ua)
    }

    pub async fn create_http_client(&self) -> reqwest::Client {
        build_safe_http_client(self.http_client_builder().await)
    }

    pub async fn http_client(&self) -> reqwest::Client {
        let global_ver = GLOBAL_PROXY_VERSION.load(Ordering::Acquire);
        let start_gen = {
            let state = self.state.lock().await;
            let global_ok = state.proxy.is_some() || state.http_client_proxy_version == global_ver;
            if global_ok {
                if let Some(ref client) = state.http_client {
                    return client.clone();
                }
            }
            state.http_client_generation
        };

        let client = self.create_http_client().await;
        let mut state = self.state.lock().await;
        let current_global_ver = GLOBAL_PROXY_VERSION.load(Ordering::Acquire);
        if state.http_client_generation == start_gen
            && (state.proxy.is_some() || current_global_ver == global_ver)
        {
            state.http_client = Some(client.clone());
            state.http_client_proxy_version = global_ver;
        }
        client
    }

    pub async fn is_vpn(&self) -> bool {
        let state = self.state.lock().await;
        if state.writer_tx.is_none() || state.is_closed {
            return false;
        }
        state.is_vpn.unwrap_or(false)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.event_tx.subscribe()
    }
    
    pub async fn is_connected(&self) -> bool {
        let state = self.state.lock().await;
        state.writer_tx.is_some() && !state.is_closed
    }
    
    pub async fn set_user_id(&self, user_id: u64) {
        self.state.lock().await.user_id = Some(user_id);
    }

    pub async fn set_telemetry_mode(&self, mode: String) {
        self.state.lock().await.telemetry_mode = mode;
    }

    pub async fn set_presence(&self, online: bool) {
        self.state.lock().await.interactive = online;
    }

    pub async fn get_token(&self) -> Option<String> {
        self.state.lock().await.token.clone()
    }

    pub async fn get_user_id(&self) -> Option<u64> {
        self.state.lock().await.user_id
    }

    pub async fn is_synced(&self) -> bool {
        self.state.lock().await.is_synced
    }

    pub async fn get_identity(&self) -> Option<(Identity, bool)> {
        let s = self.state.lock().await;
        s.identity.clone().map(|id| (id, s.is_mobile))
    }

    /// Sends an immediate PING with a short timeout to verify the socket is alive.
    /// Disconnects and returns `false` if the socket is half-open or unresponsive.
    pub async fn probe_liveness(&self) -> bool {
        let (connected, interactive, generation) = {
            let state = self.state.lock().await;
            (
                state.writer_tx.is_some() && !state.is_closed,
                state.interactive,
                state.generation,
            )
        };
        if !connected {
            return false;
        }
        match timeout(
            Duration::from_secs(5),
            self.send_and_wait_with_generation(
                1,
                json!({ "interactive": interactive }),
                0,
                Some(generation),
            ),
        )
        .await
        {
            Ok(Ok(_)) => true,
            _ => {
                self.invalidate_connection(generation, "Liveness probe failed")
                    .await;
                false
            }
        }
    }

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

    pub async fn set_custom_http_user_agent(&self, ua: Option<String>) -> ClientResult<()> {
        if let Some(ref s) = ua {
            HeaderValue::from_str(s)
                .map_err(|e| Error::Other(format!("Invalid User-Agent header value: {}", e)))?;
        }
        let mut state = self.state.lock().await;
        state.custom_http_user_agent = ua;
        state.http_client = None;
        state.http_client_generation = state.http_client_generation.wrapping_add(1);
        Ok(())
    }

    pub async fn get_custom_http_user_agent(&self) -> Option<String> {
        self.state.lock().await.custom_http_user_agent.clone()
    }

    pub async fn set_user_agent(&self, ua: UserAgent) {
        let mut state = self.state.lock().await;
        if let Some(ref mut identity) = state.identity {
            identity.user_agent = ua;
        }
        state.http_client = None;
        state.http_client_generation = state.http_client_generation.wrapping_add(1);
    }

    pub async fn get_user_agent(&self) -> Option<UserAgent> {
        self.state.lock().await.identity.as_ref().map(|id| id.user_agent.clone())
    }

    fn effective_http_user_agent_from_state(state: &ClientState) -> String {
        if let Some(custom) = &state.custom_http_user_agent {
            return custom.clone();
        }
        let is_web_identity = state
            .identity
            .as_ref()
            .is_some_and(|id| id.user_agent.device_type.eq_ignore_ascii_case("web"));
        if state.is_mobile && !is_web_identity {
            if let Some(ref id) = state.identity {
                return id.user_agent.to_http_user_agent();
            }
            Constants::MOBILE_HTTP_USER_AGENT.to_string()
        } else {
            if let Some(ref id) = state.identity {
                if let Some(ref hua) = id.user_agent.header_user_agent {
                    if HeaderValue::from_str(hua).is_ok() {
                        return hua.clone();
                    }
                }
            }
            Constants::WEB_USER_AGENT.to_string()
        }
    }

    pub async fn get_effective_http_user_agent(&self) -> String {
        let state = self.state.lock().await;
        Self::effective_http_user_agent_from_state(&state)
    }
    
    pub async fn connect(&self, identity: Identity, is_mobile: bool) -> ClientResult<Response> {
        ensure_tls_provider();

        let is_web_identity = identity.user_agent.device_type.eq_ignore_ascii_case("web");
        if !is_mobile && !is_web_identity {
            return Err(Error::ConnectionFailed(
                "WebSocket transport requires a WEB identity (deviceType = \"WEB\")".into(),
            ));
        }

        let (expected_generation, mobile_host, mobile_port, use_custom_ca, custom_ca_path, proxy, custom_ua) = {
            let state_lock = self.state.lock().await;
            (
                state_lock.generation,
                state_lock.mobile_host.clone(),
                state_lock.mobile_port,
                state_lock.use_custom_ca,
                state_lock.custom_ca_path.clone(),
                state_lock.proxy.clone().or_else(get_global_proxy_config),
                state_lock.custom_http_user_agent.clone(),
            )
        };

        let (writer, reader): (Box<dyn TransportWriter>, Box<dyn TransportReader>) = timeout(Duration::from_secs(30), async {
            if is_mobile {
                info!("Подключение Mobile TCP/TLS...");

                let transport = MobileTransport::connect_tls_with_proxy(
                    &mobile_host,
                    mobile_port,
                    use_custom_ca,
                    custom_ca_path.as_deref(),
                    proxy.as_ref(),
                ).await?;

                let (w, r) = transport.split();
                Ok::<_, Error>((Box::new(w) as Box<dyn TransportWriter>, Box::new(r) as Box<dyn TransportReader>))
            } else {
                info!("Подключение к WebSocket...");
                let ws_ua = custom_ua
                    .as_deref()
                    .or_else(|| {
                        identity
                            .user_agent
                            .header_user_agent
                            .as_deref()
                            .filter(|s| HeaderValue::from_str(s).is_ok())
                    })
                    .unwrap_or(Constants::WEB_USER_AGENT);
                let req_builder = HttpRequestBuilder::new()
                    .header("Origin", HeaderValue::from_static(Constants::ORIGIN_HEADER))
                    .header("User-Agent", HeaderValue::from_str(ws_ua).unwrap_or_else(|_| HeaderValue::from_static(Constants::WEB_USER_AGENT)));

                let ws_url = Url::parse(Constants::WEBSOCKET_URI)
                    .map_err(|e| Error::ConnectionFailed(e.to_string()))?;
                let ws_options = Options::default().with_compression_level(CompressionLevel::fast());

                let host = ws_url
                    .host_str()
                    .ok_or_else(|| Error::ConnectionFailed("No host in WEBSOCKET_URI".into()))?
                    .to_string();
                let port = ws_url.port_or_known_default().unwrap_or(443);
                let is_ws = ws_url.scheme() == "ws";

                let tcp = transport::proxy::open_proxied_stream(proxy.as_ref(), &host, port).await?;

                let ws = if is_ws {
                    WebSocket::handshake_with_request(ws_url, tcp, ws_options, req_builder)
                        .await
                        .map_err(|e| Error::ConnectionFailed(e.to_string()))?
                } else {
                    let ca_path = custom_ca_path.clone();
                    let tls_config = tokio::task::spawn_blocking(move || {
                        transport::mobile::create_tls_config(
                            use_custom_ca,
                            ca_path.as_deref(),
                        )
                    })
                    .await
                    .map_err(|e| Error::ConnectionFailed(format!("TLS config task error: {}", e)))?
                    .map_err(|e| Error::ConnectionFailed(format!("TLS config error: {}", e)))?;

                    let connector = TlsConnector::from(Arc::new(tls_config));
                    let domain = ServerName::try_from(host)
                        .map_err(|_| Error::ConnectionFailed("Invalid DNS name".into()))?;

                    let tls_stream = connector.connect(domain, tcp).await
                        .map_err(|e| Error::ConnectionFailed(format!("WebSocket TLS Error: {}", e)))?;

                    WebSocket::handshake_with_request(ws_url, tls_stream, ws_options, req_builder)
                        .await
                        .map_err(|e| Error::ConnectionFailed(e.to_string()))?
                };

                info!("WebSocket подключен.");
                let transport = WebTransport::new(ws);
                let (w, r) = transport.split();
                Ok::<_, Error>((Box::new(w) as Box<dyn TransportWriter>, Box::new(r) as Box<dyn TransportReader>))
            }
        })
        .await
        .map_err(|_| Error::ConnectionFailed("Connection setup timed out after 30 seconds".into()))??;

        info!("Разделение потоков и запуск задач...");

        let generation = next_generation();
        let (shutdown_rx_ping, handshake_response) = {
            let mut state_lock = self.state.lock().await;
            if state_lock.generation != expected_generation {
                info!("Соединение отменено (поколение изменилось во время подключения)");
                return Err(Error::ConnectionClosed("Подключение отменено".to_string()));
            }

            state_lock.identity = Some(identity.clone());
            state_lock.is_mobile = is_mobile;
            state_lock.http_client = None;
            state_lock.http_client_generation = state_lock.http_client_generation.wrapping_add(1);
            state_lock.seq = 0;
            state_lock.is_closed = false;
            state_lock.is_synced = false;
            state_lock.is_vpn = None;
            state_lock.calls_seed = None;
            state_lock.generation = generation;
            state_lock.session_id = Utc::now().timestamp_millis();

            if let Some(old_shutdown) = state_lock.shutdown_tx.take() {
                let _ = old_shutdown.send(());
            }

            let old_pending = std::mem::replace(&mut state_lock.pending, Arc::new(Mutex::new(HashMap::new())));
            {
                let mut guard = old_pending.lock().unwrap_or_else(|e| e.into_inner());
                for (_, sender) in guard.drain() {
                    let _ = sender.send(Err(Error::ConnectionClosed("Соединение заменено новым".to_string())));
                }
            }

            let pending_clone = Arc::clone(&state_lock.pending);
            let (shutdown_tx, shutdown_rx_read) = broadcast::channel(1);
            let shutdown_rx_ping = shutdown_tx.subscribe();
            let shutdown_rx_write = shutdown_tx.subscribe();
            let event_tx = self.event_tx.clone();

            tokio::spawn(Self::read_task(reader, pending_clone, event_tx, shutdown_rx_read, Arc::clone(&self.state), generation));
            debug!("Задача read_task запущена.");

            let (writer_tx, writer_rx) = mpsc::channel::<WriteCommand>(64);
            let write_client = self.clone();
            tokio::spawn(Self::write_task(writer, writer_rx, shutdown_rx_write, write_client, generation));
            debug!("Задача write_task запущена.");

            state_lock.writer_tx = Some(writer_tx);
            state_lock.shutdown_tx = Some(shutdown_tx);
            (shutdown_rx_ping, ())
        };
        let _ = handshake_response;

        debug!("Отправка Handshake");

        let is_web = !is_mobile || is_web_identity;
        let handshake_payload = if is_web {
            json!({
                "deviceId": identity.device_id,
                "userAgent": identity.user_agent.to_web_payload(),
            })
        } else {
            json!({
                "clientSessionId": 1,
                "mt_instanceid": identity.mt_instance,
                "userAgent": identity.user_agent,
                "deviceId": identity.device_id,
            })
        };

        struct HandshakeGuard {
            client: MaxClient,
            generation: u64,
            active: bool,
        }

        impl Drop for HandshakeGuard {
            fn drop(&mut self) {
                if self.active {
                    let client = self.client.clone();
                    let generation = self.generation;
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.spawn(async move {
                            client
                                .invalidate_connection(generation, "Подключение отменено")
                                .await;
                        });
                    }
                }
            }
        }

        let mut handshake_guard = HandshakeGuard {
            client: self.clone(),
            generation,
            active: true,
        };

        let handshake_response = match self.send_and_wait_with_generation(6, handshake_payload, 0, Some(generation)).await {
            Ok(resp) => {
                handshake_guard.active = false;
                resp
            }
            Err(e) => {
                handshake_guard.active = false;
                self.invalidate_connection(generation, &e.to_string()).await;
                return Err(e);
            }
        };

        let mut state = self.state.lock().await;
        if state.generation != generation || state.writer_tx.is_none() {
            return Err(Error::ConnectionClosed("Соединение закрыто до завершения рукопожатия".to_string()));
        }

        if let Some(seed) = handshake_response.payload.get("callsSeed").and_then(|v| {
            v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        }) {
            debug!("Получен calls_seed: {}", seed);
            state.calls_seed = Some(seed);
        }

        if let Some(is_vpn) = handshake_response.payload.get("isVpn").and_then(|v| v.as_bool()) {
            state.is_vpn = Some(is_vpn);
        }

        drop(state);

        let ping_client = self.clone();
        tokio::spawn(Self::ping_task(ping_client, shutdown_rx_ping, generation));
        debug!("Задача ping_task запущена.");

        Ok(handshake_response)
    }
    
    pub async fn disconnect(&self) {
        let mut state = self.state.lock().await;
        state.is_closed = true;
        state.is_synced = false;
        state.generation = next_generation();
        state.is_vpn = None;
        state.calls_seed = None;

        if let Some(shutdown_tx) = state.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        state.writer_tx = None;

        let old_pending = std::mem::replace(&mut state.pending, Arc::new(Mutex::new(HashMap::new())));
        {
            let mut pending_guard = old_pending.lock().unwrap_or_else(|e| e.into_inner());
            for (_, sender) in pending_guard.drain() {
                let _ = sender.send(Err(Error::ConnectionClosed("Соединение закрыто".to_string())));
            }
        }

        state.token = None;
        state.user_id = None;
        state.seq = 0;
        drop(state);

        info!("Клиент отключен, состояние сброшено.");
    }

    fn close_generation_state(
        s: &mut ClientState,
        event_tx: &broadcast::Sender<Value>,
        generation: u64,
    ) -> bool {
        if s.generation == generation && !s.is_closed {
            s.writer_tx = None;
            s.is_closed = true;
            s.is_synced = false;
            s.is_vpn = None;
            if let Some(shutdown) = s.shutdown_tx.take() {
                let _ = shutdown.send(());
            }
            let _ = event_tx.send(json!({
                "type": "log",
                "response": "closed"
            }));
            true
        } else {
            false
        }
    }

    async fn invalidate_connection(&self, generation: u64, reason: &str) {
        let mut s = self.state.lock().await;
        if Self::close_generation_state(&mut s, &self.event_tx, generation) {
            let mut pending_guard = s.pending.lock().unwrap_or_else(|e| e.into_inner());
            for (_, sender) in pending_guard.drain() {
                let _ = sender.send(Err(Error::ConnectionClosed(reason.to_string())));
            }
        }
    }

    fn is_pre_write_error(err: &Error) -> bool {
        match err {
            Error::SendFailed(msg) => {
                msg.starts_with("Payload too large")
                    || msg.starts_with("MsgPack encode error")
                    || msg.starts_with("JSON serialization error")
            }
            _ => false,
        }
    }

    async fn write_task(
        mut writer: Box<dyn TransportWriter>,
        mut rx: mpsc::Receiver<WriteCommand>,
        mut shutdown_rx: broadcast::Receiver<()>,
        client: MaxClient,
        generation: u64,
    ) {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => break,
                maybe_cmd = rx.recv() => {
                    let Some(cmd) = maybe_cmd else {
                        break;
                    };
                    if shutdown_rx.try_recv().is_ok() {
                        let _ = cmd.done.send(Err(Error::ConnectionClosed("Соединение закрыто".to_string())));
                        break;
                    }
                    if cmd.generation != generation {
                        let _ = cmd.done.send(Err(Error::ConnectionClosed("Соединение закрыто".to_string())));
                        continue;
                    }
                    if cmd.done.is_closed() {
                        debug!("Запись отменена вызывающим кодом до начала отправки в сокет");
                        continue;
                    }
                    match timeout(Constants::DEFAULT_TIMEOUT, writer.send(cmd.request)).await {
                        Ok(Ok(())) => {
                            let _ = cmd.done.send(Ok(()));
                        }
                        Ok(Err(e)) => {
                            if Self::is_pre_write_error(&e) {
                                warn!("Ошибка подготовки фрейма до записи в сокет: {}. Соединение не закрывается.", e);
                                let _ = cmd.done.send(Err(e));
                            } else {
                                error!("Ошибка отправки фрейма: {}. Закрываем соединение во избежание рассинхронизации потока.", e);
                                client.invalidate_connection(generation, &e.to_string()).await;
                                let _ = cmd.done.send(Err(e));
                                break;
                            }
                        }
                        Err(_) => {
                            error!("Таймаут отправки фрейма. Закрываем соединение во избежание рассинхронизации потока.");
                            client.invalidate_connection(generation, "Таймаут отправки фрейма").await;
                            let _ = cmd.done.send(Err(Error::RequestTimeout(Constants::DEFAULT_TIMEOUT)));
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn send_frame(&self, request: Request, expected_generation: u64) -> ClientResult<()> {
        let writer_tx = {
            let state = self.state.lock().await;
            if state.generation != expected_generation || state.is_closed {
                return Err(Error::ConnectionClosed("Соединение закрыто".to_string()));
            }
            match &state.writer_tx {
                Some(tx) => tx.clone(),
                None => return Err(Error::NotConnected),
            }
        };

        let (done_tx, done_rx) = oneshot::channel();
        let cmd = WriteCommand {
            request,
            generation: expected_generation,
            done: done_tx,
        };

        match timeout(Constants::DEFAULT_TIMEOUT, writer_tx.send(cmd)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(Error::ConnectionClosed("Соединение закрыто".to_string())),
            Err(_) => {
                warn!("Таймаут ожидания очереди writer");
                return Err(Error::LockTimeout(Constants::DEFAULT_TIMEOUT));
            }
        }

        match done_rx.await {
            Ok(res) => res,
            Err(_) => Err(Error::ConnectionClosed("Соединение закрыто".to_string())),
        }
    }

    pub async fn send_and_wait(
        &self,
        opcode: u16,
        payload: serde_json::Value,
        cmd: u8,
    ) -> ClientResult<Response> {
        self.send_and_wait_with_generation(opcode, payload, cmd, None).await
    }

    async fn send_and_wait_with_generation(
        &self,
        opcode: u16,
        payload: serde_json::Value,
        cmd: u8,
        expected_generation: Option<u64>,
    ) -> ClientResult<Response> {
        struct PendingGuard {
            pending: Arc<Mutex<HashMap<u64, oneshot::Sender<ClientResult<Response>>>>>,
            seq: u64,
            active: bool,
        }

        impl Drop for PendingGuard {
            fn drop(&mut self) {
                if self.active {
                    self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.seq);
                }
            }
        }

        let (tx, rx) = oneshot::channel();
        
        let (request, generation, mut pending_guard) = {
            let mut state = self.state.lock().await;
            if let Some(expected) = expected_generation {
                if state.generation != expected {
                    return Err(Error::ConnectionClosed("Сессия изменилась до отправки запроса".to_string()));
                }
            }
            if state.writer_tx.is_none() {
                return Err(Error::NotConnected);
            }
            let pending_arc = Arc::clone(&state.pending);
            let current_seq = {
                let mut pending_guard = pending_arc.lock().unwrap_or_else(|e| e.into_inner());
                let mut candidate = state.seq;
                let mut chosen = None;
                for _ in 0..=u16::MAX {
                    candidate = (candidate + 1) & 0xFFFF;
                    if !pending_guard.contains_key(&candidate) {
                        chosen = Some(candidate);
                        break;
                    }
                }
                let seq = chosen.ok_or_else(|| {
                    Error::Other("No free request sequence numbers".to_string())
                })?;
                state.seq = seq;
                pending_guard.insert(seq, tx);
                seq
            };
            let generation = state.generation;
            let guard = PendingGuard {
                pending: pending_arc,
                seq: current_seq,
                active: true,
            };

            (Request {
                ver: 10,
                cmd,
                seq: current_seq,
                opcode,
                payload,
            }, generation, guard)
        };
        
        debug!("Отправка: {:?}", request);
        let _ = self.event_tx.send(json!({
            "type": "log",
            "request": request
        })); 

        self.send_frame(request.clone(), generation).await?;
        
        match timeout(Constants::DEFAULT_TIMEOUT, rx).await {
            Ok(Ok(Ok(response))) => {
                pending_guard.active = false;
                trace!("Получен ответ для seq: {}", response.seq);
                if response.payload.get("error").is_some() {
                    Err(Error::ApiResponse(response.payload))
                } else {
                    Ok(response)
                }
            }
            Ok(Ok(Err(e))) => {
                pending_guard.active = false;
                Err(e)
            }
            Ok(Err(_)) => {
                pending_guard.active = false;
                error!("Ошибка получения ответа (канал закрыт) для seq: {}", request.seq);
                Err(Error::ConnectionClosed("Соединение закрыто".to_string()))
            }
            Err(_) => {
                warn!("Таймаут запроса для seq: {}", request.seq);
                Err(Error::RequestTimeout(Constants::DEFAULT_TIMEOUT))
            }
        }
    }
    
    fn build_push_ack(resp: &Response) -> Option<Request> {
        if resp.cmd != 0 {
            return None;
        }
        let payload = match resp.opcode {
            1 => json!({}),
            128 => {
                let chat_id = resp.payload.get("chatId")?;
                let msg = resp.payload.get("message")?;
                let msg_id = msg.get("id")?;
                let mut map = serde_json::Map::new();
                map.insert("chatId".to_string(), chat_id.clone());
                map.insert("messageId".to_string(), msg_id.clone());
                if let Some(post_id) = resp.payload.get("postId").filter(|v| v.as_i64().unwrap_or(0) != 0) {
                    map.insert("postId".to_string(), post_id.clone());
                }
                if msg.get("type").and_then(|v| v.as_str()) == Some("GROUP")
                    || resp.payload.pointer("/chat/type").and_then(|v| v.as_str()) == Some("CHAT")
                    || chat_id.as_i64().map(|id| id < 0).unwrap_or(false)
                {
                    map.insert("chatType".to_string(), json!("GROUP_CHAT"));
                }
                Value::Object(map)
            }
            137 => {
                let conv_id = resp.payload.get("conversationId")?;
                let caller_id = resp.payload.get("callerId")?;
                json!({
                    "conversationId": conv_id,
                    "callerId": caller_id,
                })
            }
            _ => return None,
        };
        Some(Request {
            ver: resp.ver,
            cmd: 1,
            seq: resp.seq,
            opcode: resp.opcode,
            payload,
        })
    }

    async fn read_task(
        mut reader: Box<dyn TransportReader>,
        pending: Arc<Mutex<HashMap<u64, oneshot::Sender<ClientResult<Response>>>>>,
        event_sender: broadcast::Sender<Value>,
        mut shutdown_rx: broadcast::Receiver<()>,
        state: Arc<TokioMutex<ClientState>>,
        generation: u64,
    ) {
        loop {
            tokio::select! {
                msg_result = reader.next_message() => {
                    match msg_result {
                        Ok(Some(resp)) => {
                            let seq = resp.seq;
                            let waiting_sender = {
                                let mut guard = pending.lock().unwrap_or_else(|e| e.into_inner());
                                guard.remove(&seq)
                            };
                            
                            if let Some(sender) = waiting_sender {
                                let _ = event_sender.send(json!({
                                    "type": "log",
                                    "response": &resp
                                }));
                                let _ = sender.send(Ok(resp));
                            } else {
                                let state_guard = state.lock().await;
                                if state_guard.generation == generation && state_guard.writer_tx.is_some() {
                                    if let (Some(tx), Some(ack_req)) = (state_guard.writer_tx.clone(), Self::build_push_ack(&resp)) {
                                        let (done_tx, done_rx) = oneshot::channel();
                                        let cmd = WriteCommand {
                                            request: ack_req,
                                            generation,
                                            done: done_tx,
                                        };
                                        match tx.try_send(cmd) {
                                            Ok(()) => {
                                                tokio::spawn(async move {
                                                    let _ = done_rx.await;
                                                });
                                            }
                                            Err(e) => {
                                                warn!("Не удалось поставить ACK (opcode={}) в очередь записи: {}", resp.opcode, e);
                                            }
                                        }
                                    }
                                    let _ = event_sender.send(json!({
                                        "type": "rx",
                                        "response": resp
                                    }));
                                } else {
                                    debug!("Пропускаем устаревший ответ как rx для старого поколения {}", generation);
                                }
                            }
                        },
                        Ok(None) => {
                            info!("Соединение закрыто (EOF)");
                            {
                                let mut s = state.lock().await;
                                Self::close_generation_state(&mut s, &event_sender, generation);
                            }
                            let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
                            for (_, sender) in pending_guard.drain() {
                                let _ = sender.send(Err(Error::ConnectionClosed("Соединение закрыто".to_string())));
                            }
                            break;
                        },
                        Err(e) => {
                            error!("Ошибка чтения транспорта:\n{}", e);
                            {
                                let mut s = state.lock().await;
                                Self::close_generation_state(&mut s, &event_sender, generation);
                            }
                            let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
                            for (_, sender) in pending_guard.drain() {
                                let _ = sender.send(Err(Error::ConnectionClosed(e.to_string()))); 
                            }
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
                    for (_, sender) in pending_guard.drain() {
                        let _ = sender.send(Err(Error::ConnectionClosed("Соединение закрыто".to_string())));
                    }
                    break;
                }
            }
        }
    }
    
    async fn ping_task(
        client: MaxClient,
        mut shutdown_rx: broadcast::Receiver<()>,
        generation: u64,
    ) {
        info!("Ping task started");
        let start = tokio::time::Instant::now() + Constants::PING_INTERVAL;
        let mut interval = tokio::time::interval_at(start, Constants::PING_INTERVAL);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let (interactive, current_generation) = {
                        let state = client.state.lock().await;
                        (state.interactive, state.generation)
                    };
                    if current_generation != generation {
                        debug!("Сессия изменилась, останавливаем устаревший ping_task");
                        break;
                    }
                    debug!("Отправка Ping (interactive: {})...", interactive);
                    match client.send_and_wait_with_generation(1, json!({ "interactive": interactive }), 0, Some(generation)).await {
                        Ok(_) => {
                            info!("Pong получен");
                        }
                        Err(Error::LockTimeout(_)) => {
                            warn!("Ping отложен: writer занят другой операцией, повторим на следующем интервале");
                        }
                        Err(e) => {
                            error!("Ping failed: {}. Stopping ping_task", e);
                            client.invalidate_connection(generation, &e.to_string()).await;
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
