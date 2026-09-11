use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApkBuildFingerprint {
    pub signature_scheme: String,
    pub certificate_count: i64,
    pub certificate_meta_sha256: String,
    pub certificate_sha256: Vec<String>,
    pub dex_meta_sha256: String,
    #[serde(default)]
    pub so_meta_sha256_arm64_v8a: Option<String>,
    pub so_meta_sha256: HashMap<String, String>,
    pub build_number: i64,
}

pub struct FingerprintGenerator {
    pub data: ApkBuildFingerprint,
}

impl FingerprintGenerator {
    pub fn new(version_data: ApkBuildFingerprint) -> Self {
        Self { data: version_data }
    }

    pub fn generate_fingerprint(
        &self,
        device_id: &str,
        calls_seed: i64,
        arch: Option<&str>,
    ) -> Option<Vec<u8>> {
        let arch = arch.unwrap_or("arm64-v8a");
        let so_hash_hex = self.data.so_meta_sha256.get(arch)?;

        let seed_bytes = calls_seed.to_be_bytes();
        let device_bytes = device_id.as_bytes();

        let cert_meta_bytes = hex::decode(&self.data.certificate_meta_sha256).ok()?;
        let dex_meta_bytes = hex::decode(&self.data.dex_meta_sha256).ok()?;
        let so_meta_bytes = hex::decode(so_hash_hex).ok()?;

        let mut hasher1 = Sha256::new();
        hasher1.update(&cert_meta_bytes);
        hasher1.update(&seed_bytes);
        hasher1.update(device_bytes);
        let h1 = hasher1.finalize();

        let mut hasher2 = Sha256::new();
        hasher2.update(&dex_meta_bytes);
        hasher2.update(&seed_bytes);
        hasher2.update(device_bytes);
        let h2 = hasher2.finalize();

        let mut hasher3 = Sha256::new();
        hasher3.update(&so_meta_bytes);
        hasher3.update(&seed_bytes);
        hasher3.update(device_bytes);
        let h3 = hasher3.finalize();

        let mut result = Vec::with_capacity(96);
        result.extend_from_slice(&h1);
        result.extend_from_slice(&h2);
        result.extend_from_slice(&h3);

        Some(result)
    }
}
