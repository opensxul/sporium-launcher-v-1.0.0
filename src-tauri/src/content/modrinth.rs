use super::{model::*, provider::ContentProvider};
use crate::{
    error::CoreError,
    game::{
        fs::{read_limited, write_atomic},
        network::hex,
    },
    instances::filesystem::Paths,
};
use reqwest::blocking::Client;
use serde::{Deserialize, de::DeserializeOwned};
use sha1::Digest;
use std::{
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct Modrinth {
    root: PathBuf,
    client: Client,
    next: Mutex<Instant>,
}
pub fn valid_id(id: &str) -> Result<(), CoreError> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(())
}
impl Modrinth {
    pub fn new(root: PathBuf) -> Result<Self, CoreError> {
        Ok(Self {
            root,
            client: Client::builder()
                .user_agent("Sporium/0.1.0 (Minecraft launcher)")
                .timeout(Duration::from_secs(20))
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| CoreError::Network)?,
            next: Mutex::new(Instant::now()),
        })
    }
    fn get<T: DeserializeOwned>(
        &self,
        route: &str,
        query: &[(&str, String)],
        ttl: u64,
        stale: bool,
    ) -> Result<(T, bool), CoreError> {
        let mut url = reqwest::Url::parse(&format!("https://api.modrinth.com/v2/{route}"))
            .map_err(|_| CoreError::InvalidInput)?;
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        let paths = Paths::new(&self.root)?;
        let cache = paths.root().join("shared/cache/modrinth").join(format!(
            "{}.json",
            hex(&sha1::Sha1::digest(url.as_str().as_bytes()))
        ));
        let mut saved = read_limited(&paths, &cache, 12_000_000)
            .ok()
            .and_then(|b| serde_json::from_slice::<T>(&b).ok());
        let age = cache
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok());
        if age.is_some_and(|age| age < Duration::from_secs(ttl))
            && let Some(saved) = saved.take()
        {
            return Ok((saved, true));
        }
        let result = (|| {
            // Serialize requests and honor provider cooldowns. Never sleep a worker for a long reset.
            let mut next = self.next.lock().unwrap();
            if next.saturating_duration_since(Instant::now()) > Duration::from_secs(2) {
                return Err(CoreError::RateLimited);
            }
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            *next = Instant::now() + Duration::from_millis(220);
            let response = self
                .client
                .get(url)
                .send()
                .map_err(|_| CoreError::Network)?;
            let reset = response
                .headers()
                .get("x-ratelimit-reset")
                .or_else(|| response.headers().get("retry-after"))
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60)
                .clamp(1, 3600);
            if response.status().as_u16() == 429
                || response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .is_some_and(|v| v == "0")
            {
                *next = Instant::now() + Duration::from_secs(reset);
            }
            if response.status().as_u16() == 429 {
                return Err(CoreError::RateLimited);
            }
            if response.status().as_u16() == 404 {
                return Err(CoreError::NotFound);
            }
            let response = response
                .error_for_status()
                .map_err(|_| CoreError::Network)?;
            if response.content_length().is_some_and(|n| n > 12_000_000) {
                return Err(CoreError::Integrity);
            }
            let mut bytes = Vec::new();
            response
                .take(12_000_001)
                .read_to_end(&mut bytes)
                .map_err(|_| CoreError::Network)?;
            if bytes.len() > 12_000_000 {
                return Err(CoreError::Integrity);
            }
            let value = serde_json::from_slice(&bytes).map_err(|_| CoreError::Integrity)?;
            write_atomic(&paths, &cache, &bytes)?;
            Ok((value, false))
        })();
        match result {
            Err(error @ (CoreError::Network | CoreError::RateLimited)) if stale => {
                saved.map(|v| (v, true)).ok_or(error)
            }
            other => other,
        }
    }
}
pub fn facets(query: &CatalogQuery) -> Result<String, CoreError> {
    let mut groups: Vec<Vec<String>> = vec![];
    if query.kind == "datapack" {
        groups.push(vec!["all_project_types:datapack".into()]);
        groups.push(vec!["categories:datapack".into()]);
    } else if query.kind == "plugin" {
        groups.push(vec!["all_project_types:plugin".into()]);
        if query.loader.is_empty() {
            groups.push(vec!["categories:paper".into(), "categories:purpur".into()]);
        }
    } else if !query.kind.is_empty() {
        groups.push(vec![format!("project_type:{}", query.kind)]);
    } else {
        groups.push(vec![
            "project_type:mod".into(),
            "project_type:resourcepack".into(),
            "project_type:shader".into(),
            "project_type:modpack".into(),
        ]);
    }
    for (key, value) in [
        ("versions", &query.minecraft),
        ("categories", &query.loader),
        ("categories", &query.category),
    ] {
        if !value.is_empty() {
            groups.push(vec![format!("{key}:{value}")]);
        }
    }
    let envs: &[&str] = match query.environment.as_str() {
        "client" => &[
            "client_only",
            "client_only_server_optional",
            "client_and_server",
            "singleplayer_only",
            "server_only",
            "server_only_client_optional",
            "client_or_server",
            "client_or_server_prefers_both",
        ],
        "both" => &[
            "client_and_server",
            "client_or_server",
            "client_or_server_prefers_both",
        ],
        "server" => &[
            "server_only",
            "dedicated_server_only",
            "server_only_client_optional",
            "client_and_server",
            "client_or_server",
            "client_or_server_prefers_both",
        ],
        _ => &[],
    };
    if !envs.is_empty() {
        groups.push(envs.iter().map(|e| format!("environment:{e}")).collect());
    }
    Ok(serde_json::to_string(&groups)?)
}
impl ContentProvider for Modrinth {
    fn search(&self, query: &CatalogQuery) -> Result<CatalogPage, CoreError> {
        #[derive(Deserialize)]
        struct Hit {
            #[serde(default)]
            icon_url: Option<String>,
            project_id: String,
            title: String,
            description: String,
            author: String,
            project_type: String,
            downloads: u64,
            categories: Vec<String>,
        }
        #[derive(Deserialize)]
        struct Response {
            hits: Vec<Hit>,
            total_hits: u32,
            offset: u32,
            limit: u32,
        }
        let (result, cached): (Response, _) = self.get(
            "search",
            &[
                ("query", query.query.clone()),
                ("facets", facets(query)?),
                ("index", query.sort.clone()),
                ("offset", query.offset.to_string()),
                ("limit", "12".into()),
            ],
            300,
            true,
        )?;
        Ok(CatalogPage {
            hits: result
                .hits
                .into_iter()
                .map(|h| ContentHit {
                    icon_url: h.icon_url,
                    id: h.project_id,
                    title: h.title,
                    description: h.description,
                    author: h.author,
                    kind: if query.kind == "datapack" {
                        "datapack".into()
                    } else {
                        h.project_type
                    },
                    downloads: h.downloads,
                    categories: h.categories,
                })
                .collect(),
            next_offset: (result.offset + result.limit < result.total_hits)
                .then_some(result.offset + result.limit),
            total: result.total_hits,
            cached,
        })
    }
    fn project(&self, id: &str) -> Result<ContentProject, CoreError> {
        valid_id(id)?;
        self.get(&format!("project/{id}"), &[], 900, false)
            .map(|r| r.0)
    }
    fn versions(&self, id: &str, minecraft: &str) -> Result<Vec<ContentVersion>, CoreError> {
        valid_id(id)?;
        let mut query = vec![("include_changelog", "false".into())];
        if !minecraft.is_empty() {
            query.push(("game_versions", serde_json::to_string(&[minecraft])?));
        }
        self.get(&format!("project/{id}/version"), &query, 300, false)
            .map(|r| r.0)
    }
    fn version(&self, id: &str) -> Result<ContentVersion, CoreError> {
        valid_id(id)?;
        self.get(&format!("version/{id}"), &[], 900, false)
            .map(|r| r.0)
    }
    fn version_from_hash(&self, sha512: &str) -> Result<Option<ContentVersion>, CoreError> {
        if sha512.len() != 128 || !sha512.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::InvalidInput);
        }
        match self.get(
            &format!("version_file/{sha512}"),
            &[("algorithm", "sha512".into())],
            900,
            false,
        ) {
            Ok((version, _)) => Ok(Some(version)),
            Err(CoreError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
    fn tags(&self) -> Result<ContentTags, CoreError> {
        #[derive(Deserialize)]
        struct Game {
            version: String,
        }
        let categories = self.get("tag/category", &[], 3600, true)?.0;
        let loaders = self.get("tag/loader", &[], 3600, true)?.0;
        let (versions, _): (Vec<Game>, _) = self.get("tag/game_version", &[], 3600, true)?;
        Ok(ContentTags {
            categories,
            loaders,
            game_versions: versions.into_iter().map(|g| g.version).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_cooldown_uses_cached_search_but_never_fabricates_install_metadata() {
        let root = tempfile::tempdir().unwrap();
        let provider = Modrinth::new(root.path().into()).unwrap();
        *provider.next.lock().unwrap() = Instant::now() + Duration::from_secs(60);
        let mut url = reqwest::Url::parse("https://api.modrinth.com/v2/search").unwrap();
        url.query_pairs_mut()
            .extend_pairs(std::iter::empty::<(&str, &str)>());
        let paths = Paths::new(root.path()).unwrap();
        let cache = paths.root().join("shared/cache/modrinth").join(format!(
            "{}.json",
            hex(&sha1::Sha1::digest(url.as_str().as_bytes()))
        ));
        write_atomic(&paths, &cache, br#"{"hits":[],"total_hits":0}"#).unwrap();
        let (value, cached): (serde_json::Value, _) = provider.get("search", &[], 0, true).unwrap();
        assert!(cached);
        assert_eq!(value["total_hits"], 0);
        assert!(matches!(
            provider.get::<ContentProject>("project/missing", &[], 0, false),
            Err(CoreError::RateLimited)
        ));
        write_atomic(&paths, &cache, b"broken json").unwrap();
        assert!(matches!(
            provider.get::<serde_json::Value>("search", &[], 0, true),
            Err(CoreError::RateLimited)
        ));
        let library = crate::instances::Library::new(
            root.path().into(),
            crate::storage::Database::new(root.path().join("launcher/sporium.sqlite3")),
        );
        assert!(library.snapshot().unwrap().instances.is_empty());
    }
}
