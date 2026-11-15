use std::time::Duration;

pub struct Constants;
impl Constants {
    pub const WEBSOCKET_URI: &'static str = "wss://ws-api.oneme.ru/websocket";
    pub const MOBILE_HOST: &'static str = "api2.oneme.ru";
    pub const MOBILE_PORT: u16 = 443;
    pub const ORIGIN_HEADER: &'static str = "https://web.max.ru";
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(10000);
    pub const PING_INTERVAL: Duration = Duration::from_secs(30);
    pub const WEB_USER_AGENT: &'static str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
    pub const USER_AGENT: &'static str = Self::WEB_USER_AGENT;
    pub const MOBILE_HTTP_USER_AGENT: &'static str =
        "OKMessages%2F26.38.2+%28Android+14%3B+Samsung+SM-S928B%3B+xxhdpi+450dpi+1440x3120%29";
    pub const OKHTTP_USER_AGENT: &'static str = "okhttp/4.12.0";
}
