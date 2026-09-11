use crate::fingerprint::ApkBuildFingerprint;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

type VersionsMap = HashMap<String, ApkBuildFingerprint>;

#[derive(Clone)]
pub struct VersionDataProvider {
    cache: Arc<RwLock<VersionsMap>>,
    http_client: reqwest::Client,
    local_path: String,
}

impl VersionDataProvider {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap(),
            local_path: "./assets/mobile_versions.json".to_string(),
        }
    }

    async fn fetch_remote(&self, url: &str) -> Option<VersionsMap> {
        let resp = self.http_client.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<VersionsMap>().await.ok()
    }

    async fn fetch_local(&self, path: &str) -> Option<VersionsMap> {
        let data = fs::read(path).await.ok()?;
        serde_json::from_slice::<VersionsMap>(&data).ok()
    }

    async fn merge_into_cache(&self, incoming: VersionsMap) {
        let mut cache = self.cache.write().await;
        for (ver, data) in incoming {
            cache.entry(ver).or_insert(data);
        }
    }

    pub async fn get_version(&self, version: &str) -> Option<ApkBuildFingerprint> {
        {
            let cache = self.cache.read().await;
            if let Some(data) = cache.get(version) {
                return Some(data.clone());
            }
        }

        if let Some(map) = self.fetch_local(&self.local_path).await {
            let found = map.get(version).cloned();
            self.merge_into_cache(map).await;
            if let Some(data) = found {
                return Some(data);
            }
        }

        const GITHUB_URL: &str = "https://raw.githubusercontent.com/junioraww/rumax/refs/heads/main/assets/mobile_versions.json";
        if let Some(map) = self.fetch_remote(GITHUB_URL).await {
            let found = map.get(version).cloned();
            self.merge_into_cache(map).await;
            if let Some(data) = found {
                return Some(data);
            }
        }

        const PYMAX_URL: &str = "https://hashes.pymax.org/versions.json";
        if let Some(map) = self.fetch_remote(PYMAX_URL).await {
            let found = map.get(version).cloned();
            self.merge_into_cache(map).await;
            if let Some(data) = found {
                return Some(data);
            }
        }

        None
    }
}
