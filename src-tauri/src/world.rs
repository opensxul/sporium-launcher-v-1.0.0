use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use ts_rs::TS;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AfterLaunch {
    #[default]
    Open,
    Minimize,
    Tray,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct WorldSettings {
    pub address: String,
    pub version: String,
    pub feed: String,
    pub auto_join: bool,
    pub after_launch: AfterLaunch,
    pub auto_updates: bool,
    pub particles: bool,
    pub parallax: bool,
    pub intensity: u8,
    pub memory_mb: u32,
}
#[derive(Deserialize)]
struct Distribution {
    address: String,
}
impl Default for WorldSettings {
    fn default() -> Self {
        Self {
            address: serde_json::from_str::<Distribution>(include_str!(
                "../../config/sporium.json"
            ))
            .expect("valid packaged server configuration")
            .address,
            version: String::new(),
            feed: String::new(),
            auto_join: true,
            after_launch: AfterLaunch::Open,
            auto_updates: true,
            particles: true,
            parallax: true,
            intensity: 55,
            memory_mb: 0,
        }
    }
}
impl WorldSettings {
    pub fn validate(&self) -> Result<(), CoreError> {
        if (!self.address.is_empty() && endpoint(&self.address).is_none())
            || (!self.feed.is_empty() && feed_url(&self.feed).is_none())
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            || self.intensity > 100
            || (self.memory_mb != 0 && !(512..=32768).contains(&self.memory_mb))
        {
            return Err(CoreError::InvalidSettings);
        }
        Ok(())
    }
}

pub fn endpoint(input: &str) -> Option<(String, u16)> {
    if input.is_empty()
        || input.len() > 253
        || !input
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
    {
        return None;
    }
    if input.starts_with('[') {
        let end = input.find(']')?;
        let host = input.get(1..end)?.parse::<std::net::Ipv6Addr>().ok()?;
        let rest = &input[end + 1..];
        let port = if rest.is_empty() {
            25565
        } else {
            rest.strip_prefix(':')?.parse::<u16>().ok()?
        };
        return (port > 0).then(|| (host.to_string(), port));
    }
    let (host, port) = match input.split_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (input, 25565),
    };
    if port == 0
        || host.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return None;
    }
    Some((host.to_owned(), port))
}
pub fn feed_url(input: &str) -> Option<reqwest::Url> {
    if input.len() > 2048 {
        return None;
    }
    let url = reqwest::Url::parse(input).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some()
        && url.fragment().is_none())
    .then_some(url)
}

#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    online: bool,
    players: Option<u32>,
    maximum: Option<u32>,
    version: Option<String>,
    latency_ms: Option<u64>,
}

fn varint(mut value: u32, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn parse_varint(data: &[u8], pos: &mut usize) -> Option<usize> {
    let mut n = 0usize;
    for shift in (0..32).step_by(7) {
        let b = *data.get(*pos)?;
        *pos += 1;
        if shift == 28 && b > 7 {
            return None;
        }
        n |= usize::from(b & 127) << shift;
        if b < 128 {
            return Some(n);
        }
    }
    None
}
async fn packet(socket: &mut TcpStream) -> Result<Vec<u8>, CoreError> {
    let mut prefix = Vec::with_capacity(5);
    loop {
        let b = socket.read_u8().await.map_err(|_| CoreError::Network)?;
        prefix.push(b);
        if b < 128 {
            break;
        }
        if prefix.len() == 5 {
            return Err(CoreError::Network);
        }
    }
    let size = parse_varint(&prefix, &mut 0)
        .filter(|n| *n > 0 && *n <= 512 * 1024)
        .ok_or(CoreError::Network)?;
    let mut data = vec![0; size];
    socket
        .read_exact(&mut data)
        .await
        .map_err(|_| CoreError::Network)?;
    Ok(data)
}
async fn query(host: &str, port: u16) -> Result<ServerStatus, CoreError> {
    let mut socket = match TcpStream::connect((host, port)).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            return Ok(ServerStatus::default());
        }
        Err(_) => return Err(CoreError::Network),
    };
    let mut handshake = vec![0];
    varint(u32::MAX, &mut handshake);
    varint(host.len() as u32, &mut handshake);
    handshake.extend(host.as_bytes());
    handshake.extend(port.to_be_bytes());
    handshake.push(1);
    let mut request = vec![];
    varint(handshake.len() as u32, &mut request);
    request.extend(handshake);
    request.extend([1, 0]);
    socket
        .write_all(&request)
        .await
        .map_err(|_| CoreError::Network)?;
    let data = packet(&mut socket).await?;
    let mut i = 0;
    if parse_varint(&data, &mut i) != Some(0) {
        return Err(CoreError::Network);
    }
    let len = parse_varint(&data, &mut i).ok_or(CoreError::Network)?;
    if len != data.len() - i {
        return Err(CoreError::Network);
    }
    #[derive(Deserialize)]
    struct Reply {
        version: Version,
        players: Option<Players>,
    }
    #[derive(Deserialize)]
    struct Version {
        name: String,
    }
    #[derive(Deserialize)]
    struct Players {
        online: Option<u32>,
        max: Option<u32>,
    }
    let reply: Reply = serde_json::from_slice(&data[i..]).map_err(|_| CoreError::Network)?;
    let started = Instant::now();
    let ping = [9, 1, 0, 0, 0, 0, 0, 0, 0, 1];
    let latency_ms = tokio::time::timeout(Duration::from_millis(800), async {
        socket.write_all(&ping).await.ok()?;
        (packet(&mut socket).await.ok()?.as_slice() == &ping[1..])
            .then(|| started.elapsed().as_millis() as u64)
    })
    .await
    .ok()
    .flatten();
    Ok(ServerStatus {
        online: true,
        players: reply.players.as_ref().and_then(|v| v.online),
        maximum: reply.players.and_then(|v| v.max),
        version: Some(reply.version.name.chars().take(100).collect()),
        latency_ms,
    })
}
async fn preferences(state: &AppState) -> Result<WorldSettings, CommandError> {
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || db.load_settings().map(|s| s.values.world))
        .await
        .map_err(|_| CoreError::Worker)?
        .map_err(Into::into)
}
#[tauri::command]
pub async fn server_status(
    state: tauri::State<'_, AppState>,
) -> Result<ServerStatus, CommandError> {
    let (host, port) =
        endpoint(&preferences(&state).await?.address).ok_or(CoreError::InvalidSettings)?;
    tokio::time::timeout(Duration::from_secs(5), query(&host, port))
        .await
        .map_err(|_| CoreError::Network)?
        .map_err(Into::into)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    title: String,
    date: String,
    summary: String,
    body: String,
    image: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Photo {
    title: String,
    image: String,
    thumbnail: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Feed {
    news: Vec<Article>,
    gallery: Vec<Photo>,
}

fn asset_url(base: &reqwest::Url, path: &str) -> Option<reqwest::Url> {
    let url = base.join(path).ok()?;
    (url.origin() == base.origin() && feed_url(url.as_str()).is_some()).then_some(url)
}
fn read_url(url: reqwest::Url, limit: u64) -> Result<Vec<u8>, CoreError> {
    use std::io::Read;
    let response = reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| CoreError::Network)?
        .get(url)
        .send()
        .map_err(|_| CoreError::Network)?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > limit) {
        return Err(CoreError::Network);
    }
    let mut buf = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|_| CoreError::Network)?;
    if buf.len() as u64 > limit {
        return Err(CoreError::Network);
    }
    Ok(buf)
}
fn parse_feed(base: &reqwest::Url, bytes: &[u8]) -> Result<Feed, CoreError> {
    let mut feed: Feed = serde_json::from_slice(bytes).map_err(|_| CoreError::InvalidInput)?;
    if feed.news.len() > 100 || feed.gallery.len() > 100 {
        return Err(CoreError::InvalidInput);
    }
    for p in &mut feed.gallery {
        p.image = asset_url(base, &p.image)
            .ok_or(CoreError::InvalidInput)?
            .into();
        p.thumbnail = p
            .thumbnail
            .as_ref()
            .and_then(|v| asset_url(base, v))
            .map(Into::into);
    }
    for n in &mut feed.news {
        n.image = n
            .image
            .as_ref()
            .and_then(|v| asset_url(base, v))
            .map(Into::into);
    }
    feed.news.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(feed)
}
#[tauri::command]
pub async fn world_feed(state: tauri::State<'_, AppState>) -> Result<Feed, CommandError> {
    let base = feed_url(&preferences(&state).await?.feed).ok_or(CoreError::InvalidSettings)?;
    tauri::async_runtime::spawn_blocking(move || {
        parse_feed(&base, &read_url(base.clone(), 512 * 1024)?)
    })
    .await
    .map_err(|_| CoreError::Worker)?
    .map_err(Into::into)
}
#[tauri::command]
pub async fn world_image(
    path: String,
    small: bool,
    state: tauri::State<'_, AppState>,
) -> Result<String, CommandError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let base = feed_url(&preferences(&state).await?.feed).ok_or(CoreError::InvalidSettings)?;
    let url = asset_url(&base, &path).ok_or(CoreError::InvalidInput)?;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = read_url(url, 6 * 1024 * 1024)?;
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| CoreError::InvalidInput)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(96 * 1024 * 1024);
        reader.limits(limits);
        let img = reader.decode().map_err(|_| CoreError::InvalidInput)?;
        let width = if small { 480 } else { 1920 };
        let img = img.thumbnail(width, width);
        let mut output = std::io::Cursor::new(Vec::new());
        img.to_rgb8()
            .write_to(&mut output, image::ImageFormat::Jpeg)
            .map_err(|_| CoreError::InvalidInput)?;
        Ok::<_, CoreError>(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(output.into_inner())
        ))
    })
    .await
    .map_err(|_| CoreError::Worker)?
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_real_articles_and_requires_same_origin_photos() {
        let url = feed_url("https://example.org/news/feed.json").unwrap();
        let bytes=br#"{"news":[{"title":"A","summary":"one","date":"2026-01-01","body":"a"},{"title":"B","summary":"two","date":"2026-01-02","body":"b","image":"cover.jpg"}],"gallery":[{"title":"Spawn","image":"world.jpg","thumbnail":"world-small.jpg"}]}"#;
        let feed = parse_feed(&url, bytes).unwrap();
        assert_eq!(feed.news[0].title, "B");
        assert_eq!(
            feed.news[0].image.as_deref(),
            Some("https://example.org/news/cover.jpg")
        );
        assert_eq!(
            feed.gallery[0].thumbnail.as_deref(),
            Some("https://example.org/news/world-small.jpg")
        );
        assert!(
            parse_feed(
                &url,
                br#"{"gallery":[{"title":"Spawn","image":"https://elsewhere.example/image.jpg"}]}"#
            )
            .is_err()
        );
    }
    #[test]
    fn destinations_and_bounds() {
        assert_eq!(endpoint("[::1]:25566"), Some(("::1".into(), 25566)));
        for value in [
            "",
            "bad host",
            "-flag",
            "host:0",
            "host:99999",
            "user@host",
            "foo..bar",
        ] {
            assert!(endpoint(value).is_none())
        }
        let url = feed_url("https://example.org/feed.json").unwrap();
        assert!(asset_url(&url, "photos/spawn.jpg").is_some());
        for value in [
            "https://other.example/image",
            "file:///C:/x",
            "//other.example",
            "https://name@example.org/x",
        ] {
            assert!(asset_url(&url, value).is_none())
        }
        assert!(parse_varint(&[255, 255, 255, 255, 255, 0], &mut 0).is_none());
    }
}
