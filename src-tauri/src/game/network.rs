use crate::error::CoreError;
use reqwest::blocking::Client;
use sha1::Digest;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone)]
pub enum Hash {
    Sha1(String),
    Sha256(String),
    Sha512(String),
}
#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub hash: Hash,
    pub size: u64,
}

pub fn approved(url: &reqwest::Url) -> bool {
    if url.scheme() != "https"
        || url.port().is_some_and(|port| port != 443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    match url.host_str() {
        Some(
            "piston-meta.mojang.com"
            | "piston-data.mojang.com"
            | "launchermeta.mojang.com"
            | "launcher.mojang.com"
            | "libraries.minecraft.net"
            | "resources.download.minecraft.net"
            | "api.adoptium.net"
            | "api.mojang.com"
            | "sessionserver.mojang.com"
            | "textures.minecraft.net"
            | "meta.fabricmc.net"
            | "maven.fabricmc.net"
            | "files.minecraftforge.net"
            | "maven.minecraftforge.net"
            | "maven.neoforged.net"
            | "api.modrinth.com"
            | "cdn.modrinth.com"
            | "release-assets.githubusercontent.com"
            | "objects.githubusercontent.com",
        ) => true,
        Some("github.com") => {
            url.path().starts_with("/adoptium/temurin")
                && url.path().contains("-binaries/releases/download/")
        }
        _ => false,
    }
}

#[derive(Clone)]
pub struct Network {
    client: Client,
    #[cfg(test)]
    pub(crate) test_origin: Option<String>,
}
impl Network {
    pub(crate) fn artifact_size(&self, url: &str) -> Option<u64> {
        let url = reqwest::Url::parse(url).ok()?;
        if !approved(&url) && !self.test_url(&url) {
            return None;
        }
        self.client
            .head(url)
            .header("Accept-Encoding", "identity")
            .timeout(Duration::from_secs(15))
            .send()
            .ok()?
            .error_for_status()
            .ok()?
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)?
            .to_str()
            .ok()?
            .parse()
            .ok()
    }
    pub(crate) fn transfer_response(
        &self,
        url: &str,
        offset: u64,
    ) -> Result<reqwest::blocking::Response, CoreError> {
        let url = reqwest::Url::parse(url).map_err(|_| CoreError::Network)?;
        if !approved(&url) && !self.test_url(&url) {
            return Err(CoreError::Network);
        }
        let mut request = self
            .client
            .get(url)
            .header("Accept-Encoding", "identity")
            .timeout(Duration::from_secs(30));
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        request.send().map_err(|_| CoreError::Network)
    }
    fn test_url(&self, _url: &reqwest::Url) -> bool {
        #[cfg(test)]
        {
            self.test_origin
                .as_deref()
                .is_some_and(|origin| _url.origin().ascii_serialization() == origin)
        }
        #[cfg(not(test))]
        false
    }
    pub fn new() -> Result<Self, CoreError> {
        Self::with_timeout(Duration::from_secs(120))
    }
    pub fn with_timeout(timeout: Duration) -> Result<Self, CoreError> {
        let client = Client::builder()
            .user_agent("Sporium/0.1.0 (Minecraft launcher)")
            .connect_timeout(Duration::from_secs(15))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 8 || !approved(attempt.url()) {
                    attempt.error("untrusted redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| CoreError::Network)?;
        Ok(Self {
            client,
            #[cfg(test)]
            test_origin: None,
        })
    }
    fn response(&self, url: &str) -> Result<reqwest::blocking::Response, CoreError> {
        let url = reqwest::Url::parse(url).map_err(|_| CoreError::Network)?;
        if !approved(&url) {
            return Err(CoreError::Network);
        }
        for attempt in 0..3 {
            match self
                .client
                .get(url.clone())
                .send()
                .and_then(|r| r.error_for_status())
            {
                Ok(response) => return Ok(response),
                Err(error) => {
                    // Public host and error category only; URLs can contain private query strings.
                    eprintln!(
                        "Network {}: status={:?}, timeout={}, connect={}",
                        url.host_str().unwrap_or("unknown"),
                        error.status(),
                        error.is_timeout(),
                        error.is_connect()
                    );
                    if error
                        .status()
                        .is_some_and(|s| s.is_client_error() && s.as_u16() != 429)
                        || attempt == 2
                    {
                        return Err(CoreError::Network);
                    }
                }
            }
        }
        Err(CoreError::Network)
    }
    pub fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, CoreError> {
        let response = self.response(url)?;
        if response.content_length().is_some_and(|size| size > limit) {
            return Err(CoreError::Integrity);
        }
        let mut result = Vec::new();
        response
            .take(limit + 1)
            .read_to_end(&mut result)
            .map_err(|_| CoreError::Network)?;
        if result.len() as u64 > limit {
            return Err(CoreError::Integrity);
        }
        Ok(result)
    }
    pub fn optional_bytes(&self, url: &str, limit: u64) -> Result<Option<Vec<u8>>, CoreError> {
        let url = reqwest::Url::parse(url).map_err(|_| CoreError::Network)?;
        if !approved(&url) {
            return Err(CoreError::Network);
        }
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|_| CoreError::Network)?;
        if matches!(response.status().as_u16(), 204 | 404) {
            return Ok(None);
        }
        let response = response
            .error_for_status()
            .map_err(|_| CoreError::Network)?;
        let mut bytes = Vec::new();
        response
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CoreError::Network)?;
        if bytes.len() as u64 > limit {
            return Err(CoreError::Integrity);
        }
        Ok(Some(bytes))
    }
}

pub fn verify(path: &Path, expected: &Hash, size: u64) -> Result<bool, CoreError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if file.metadata()?.len() != size {
        return Ok(false);
    }
    let digest = match expected {
        Hash::Sha1(_) => hash_reader::<sha1::Sha1>(&mut file)?,
        Hash::Sha256(_) => hash_reader::<sha2::Sha256>(&mut file)?,
        Hash::Sha512(_) => hash_reader::<sha2::Sha512>(&mut file)?,
    };
    let (expected, len) = match expected {
        Hash::Sha1(value) => (value, 40),
        Hash::Sha256(value) => (value, 64),
        Hash::Sha512(value) => (value, 128),
    };
    if expected.len() != len || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Integrity);
    }
    Ok(digest.eq_ignore_ascii_case(expected))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn hash_reader<D: Digest>(file: &mut File) -> Result<String, CoreError> {
    let mut hash = D::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex(&hash.finalize()))
}
