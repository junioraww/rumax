use serde::{Deserialize, Serialize};
use rand::seq::SliceRandom;

const APP_VERSIONS: &[(&str, i32)] = &[
    ("26.31.0", 6822), ("26.32.0", 6828), ("26.32.1", 6831),
    ("26.33.0", 6835), ("26.33.1", 6840), ("26.34.0", 6850),
    ("26.34.1", 6856), ("26.35.0", 6861), ("26.35.1", 6867),
    ("26.36.2", 6879), ("26.37.0", 6886), ("26.37.1", 6900),
    ("26.38.0", 6902), ("26.38.1", 6907), ("26.38.2", 6911),
    ("26.39.0", 6915), ("26.40.0", 6925), ("26.40.1", 6928),
    ("26.41.0", 6931), ("26.42.0", 6936),
];

const ANDROID_DEVICES: &[(&str, &str, &str, &str)] = &[
    ("Samsung SM-A525F", "Android 13", "405dpi 405dpi 1080x2400", "arm64-v8a"),
    ("Samsung SM-A536B", "Android 14", "405dpi 405dpi 1080x2400", "arm64-v8a"),
    ("Samsung SM-A546E", "Android 14", "405dpi 405dpi 1080x2340", "arm64-v8a"),
    ("Samsung SM-G991B", "Android 14", "421dpi 421dpi 1080x2400", "arm64-v8a"),
    ("Samsung SM-G998B", "Android 13", "515dpi 515dpi 1440x3200", "arm64-v8a"),
    ("Samsung SM-S901B", "Android 14", "425dpi 425dpi 1080x2340", "arm64-v8a"),
    ("Samsung SM-S911B", "Android 14", "425dpi 425dpi 1080x2340", "arm64-v8a"),
    ("Samsung SM-S928B", "Android 14", "xxhdpi 450dpi 1440x3120", "arm64-v8a"),
    ("Xiaomi 2109119DG", "Android 13", "395dpi 395dpi 1080x2400", "arm64-v8a"),
    ("Xiaomi 2201117TG", "Android 13", "395dpi 395dpi 1080x2400", "arm64-v8a"),
    ("Xiaomi 2201123G", "Android 14", "526dpi 526dpi 1440x3200", "arm64-v8a"),
    ("Xiaomi 2210132G", "Android 14", "446dpi 446dpi 1220x2712", "arm64-v8a"),
    ("Xiaomi 23049PCD8G", "Android 14", "446dpi 446dpi 1220x2712", "arm64-v8a"),
    ("Redmi 2201116TG", "Android 13", "395dpi 395dpi 1080x2400", "arm64-v8a"),
    ("Redmi 22101316G", "Android 13", "395dpi 395dpi 1080x2400", "arm64-v8a"),
    ("Redmi 23021RAA2Y", "Android 14", "395dpi 395dpi 1080x2400", "arm64-v8a"),
    ("POCO 22011211G", "Android 13", "395dpi 395dpi 1080x2400", "arm64-v8a"),
    ("POCO 23049PCD8G", "Android 14", "446dpi 446dpi 1220x2712", "arm64-v8a"),
    ("Pixel 6", "Android 14", "411dpi 411dpi 1080x2400", "arm64-v8a"),
    ("Pixel 6a", "Android 14", "429dpi 429dpi 1080x2400", "arm64-v8a"),
    ("Pixel 7", "Android 14", "416dpi 416dpi 1080x2400", "arm64-v8a"),
    ("Pixel 7 Pro", "Android 14", "512dpi 512dpi 1440x3120", "arm64-v8a"),
    ("Pixel 8", "Android 14", "428dpi 428dpi 1080x2400", "arm64-v8a"),
    ("OnePlus NE2213", "Android 14", "525dpi 525dpi 1440x3216", "arm64-v8a"),
    ("OnePlus CPH2449", "Android 14", "451dpi 451dpi 1240x2772", "arm64-v8a"),
    ("realme RMX3085", "Android 13", "409dpi 409dpi 1080x2400", "arm64-v8a"),
    ("realme RMX3370", "Android 13", "409dpi 409dpi 1080x2400", "arm64-v8a"),
    ("realme RMX3630", "Android 13", "400dpi 400dpi 1080x2412", "arm64-v8a"),
    ("HUAWEI ELS-NX9", "Android 12", "441dpi 441dpi 1080x2340", "arm64-v8a"),
    ("HUAWEI VOG-L29", "Android 12", "398dpi 398dpi 1080x2340", "arm64-v8a"),
    ("HONOR RMO-NX1", "Android 13", "391dpi 391dpi 1080x2388", "arm64-v8a"),
    ("HONOR REA-NX9", "Android 13", "435dpi 435dpi 1200x2664", "arm64-v8a"),
];

const LOCALE_TIMEZONES: &[(&str, &str)] = &[
    ("ru", "Europe/Moscow"),
    ("ru", "Europe/Kaliningrad"),
    ("ru", "Europe/Samara"),
    ("ru", "Asia/Yekaterinburg"),
    ("ru", "Asia/Omsk"),
    ("ru", "Asia/Novosibirsk"),
    ("ru", "Asia/Krasnoyarsk"),
    ("ru", "Asia/Irkutsk"),
    ("ru", "Asia/Yakutsk"),
    ("ru", "Asia/Vladivostok"),
];

fn random_uuid_v4() -> String {
    let mut b = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn random_device_id() -> String {
    let mut b = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut b);
    hex::encode(b)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Request {
    pub ver: u8,
    #[serde(default)]
    pub cmd: u8,
    pub seq: u64,
    pub opcode: u16,
    pub payload: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Response {
    pub ver: u8,
    pub cmd: u8,
    pub seq: u64,
    pub opcode: u16,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub device_id: String,
    pub mt_instance: String,
    pub user_agent: UserAgent,
}

impl Identity {
    pub fn generate_mobile() -> Self {
        Self {
            device_id: random_device_id(),
            mt_instance: random_uuid_v4(),
            user_agent: UserAgent::generate_mobile(),
        }
    }

    pub fn generate_web() -> Self {
        Self {
            device_id: random_uuid_v4(),
            mt_instance: random_uuid_v4(),
            user_agent: UserAgent::generate_web(),
        }
    }

    pub fn default_mobile() -> Self {
        Self {
            device_id: random_device_id(),
            mt_instance: random_uuid_v4(),
            user_agent: UserAgent::default_mobile(),
        }
    }

    pub fn default_web() -> Self {
        Self {
            device_id: random_uuid_v4(),
            mt_instance: random_uuid_v4(),
            user_agent: UserAgent::default_web(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAgent {
    pub device_type: String,
    pub app_version: String,
    pub os_version: String,
    pub timezone: String,
    pub screen: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_device_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,

    pub locale: String,
    pub build_number: i32,
    pub device_name: String,
    pub device_locale: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_user_agent: Option<String>,
}


impl UserAgent {
    pub fn generate_mobile() -> Self {
        let mut rng = rand::thread_rng();
        let app = APP_VERSIONS.choose(&mut rng).unwrap_or(&("26.42.0", 6936));
        let dev = ANDROID_DEVICES.choose(&mut rng).unwrap_or(&("Samsung SM-S928B", "Android 14", "xxhdpi 450dpi 1440x3120", "arm64-v8a"));
        let loc = LOCALE_TIMEZONES.choose(&mut rng).unwrap_or(&("ru", "Europe/Moscow"));

        Self {
            device_type: "ANDROID".to_string(),
            app_version: app.0.to_string(),
            os_version: dev.1.to_string(),
            timezone: loc.1.to_string(),
            screen: dev.2.to_string(),
            push_device_type: Some("GCM".to_string()),
            arch: Some(dev.3.to_string()),
            locale: loc.0.to_string(),
            build_number: app.1,
            device_name: dev.0.to_string(),
            device_locale: loc.0.to_string(),
            release: None,
            header_user_agent: None,
        }
    }

    pub fn generate_web() -> Self {
        let mut rng = rand::thread_rng();
        let loc = LOCALE_TIMEZONES.choose(&mut rng).unwrap_or(&("ru", "Europe/Moscow"));

        let mut ua = Self::default_web();
        ua.timezone = loc.1.to_string();
        ua.locale = loc.0.to_string();
        ua.device_locale = loc.0.to_string();
        ua
    }

    pub fn to_http_user_agent(&self) -> String {
        let raw = format!(
            "OKMessages/{} ({}; {}; {})",
            self.app_version, self.os_version, self.device_name, self.screen
        );
        url::form_urlencoded::byte_serialize(raw.as_bytes()).collect()
    }

    pub fn default_web() -> Self {
        Self {
            device_type: "WEB".to_string(),
            app_version: "26.8.4".to_string(),
            os_version: "Windows".to_string(),
            timezone: "Europe/Moscow".to_string(),
            screen: "1920x1080 1.0x".to_string(),
            push_device_type: None,
            arch: None,
            locale: "ru".to_string(),
            build_number: 0,
            device_name: "Chrome".to_string(),
            device_locale: "ru".to_string(),
            release: None,
            header_user_agent: Some(crate::constants::Constants::WEB_USER_AGENT.to_string()),
        }
    }

    pub fn default_mobile() -> Self {
        Self {
            device_type: "ANDROID".to_string(),
            app_version: "26.42.0".to_string(),
            os_version: "Android 14".to_string(),
            timezone: "Europe/Moscow".to_string(),
            screen: "xxhdpi 450dpi 1440x3120".to_string(),
            push_device_type: Some("GCM".to_string()),
            arch: Some("arm64-v8a".to_string()),
            locale: "ru".to_string(),
            build_number: 6936,
            device_name: "Samsung SM-S928B".to_string(),
            device_locale: "ru".to_string(),
            release: None,
            header_user_agent: None,
        }
    }

    pub fn to_web_payload(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        if let Some(m) = v.as_object_mut() {
            m.retain(|k, _| matches!(k.as_str(),
                "deviceType" | "locale" | "deviceLocale" | "osVersion" | "deviceName" |
                "headerUserAgent" | "appVersion" | "screen" | "timezone" | "pushDeviceType"));
            m.entry("headerUserAgent").or_insert(serde_json::json!(
                crate::constants::Constants::WEB_USER_AGENT));
        }
        v
    }
}
