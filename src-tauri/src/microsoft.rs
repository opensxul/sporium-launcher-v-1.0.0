use crate::{
    commands::AppState,
    error::{CommandError, CoreError},
    storage::Database,
};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

const TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";
fn client_id() -> Result<String, CoreError> {
    let value = std::env::var("SPORIUM_MICROSOFT_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("SPORIUM_MICROSOFT_CLIENT_ID").map(str::to_owned))
        .ok_or(CoreError::SignIn)?;
    uuid::Uuid::parse_str(&value).map_err(|_| CoreError::SignIn)?;
    Ok(value)
}
#[tauri::command]
pub fn microsoft_available() -> bool {
    client_id().is_ok()
}
fn http() -> Result<Client, CoreError> {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| CoreError::Network)
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, CoreError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(CoreError::SignIn)
}
fn response(request: reqwest::blocking::RequestBuilder) -> Result<Value, CoreError> {
    request
        .send()
        .map_err(|_| CoreError::Network)?
        .error_for_status()
        .map_err(|_| CoreError::SignIn)?
        .json()
        .map_err(|_| CoreError::SignIn)
}
fn protect(value: &[u8], encrypt: bool) -> Result<Vec<u8>, CoreError> {
    use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
    let input = CRYPT_INTEGER_BLOB {
        cbData: value.len().try_into().map_err(|_| CoreError::SignIn)?,
        pbData: value.as_ptr().cast_mut(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    // DPAPI allocates the result. Copy it before releasing it with LocalFree.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 || out.pbData.is_null() {
            return Err(CoreError::SignIn);
        }
        let result = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData.cast());
        Ok(result)
    }
}
pub struct Identity {
    pub name: String,
    pub uuid: String,
    pub access: String,
    pub xuid: String,
    pub client: String,
}
fn minecraft(http: &Client, msa: &str, client: String) -> Result<Identity, CoreError> {
    let xbox=response(http.post("https://user.auth.xboxlive.com/user/authenticate").json(&json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={msa}")},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"})))?;
    let xsts=response(http.post("https://xsts.auth.xboxlive.com/xsts/authorize").json(&json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[field(&xbox,"Token")?]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"})))?;
    let hash = xsts
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(Value::as_str)
        .ok_or(CoreError::SignIn)?;
    let game = response(
        http.post("https://api.minecraftservices.com/authentication/login_with_xbox")
            .json(&json!({"identityToken":format!("XBL3.0 x={hash};{}",field(&xsts,"Token")?)})),
    )?;
    let access = field(&game, "access_token")?.to_owned();
    let rights = response(
        http.get("https://api.minecraftservices.com/entitlements/mcstore")
            .bearer_auth(&access),
    )?;
    if !rights["items"].as_array().is_some_and(|items| {
        items.iter().any(|i| {
            matches!(
                i["name"].as_str(),
                Some("game_minecraft" | "product_minecraft")
            )
        })
    }) {
        return Err(CoreError::Entitlement);
    }
    let profile = response(
        http.get("https://api.minecraftservices.com/minecraft/profile")
            .bearer_auth(&access),
    )?;
    let uuid = uuid::Uuid::parse_str(field(&profile, "id")?)
        .map_err(|_| CoreError::SignIn)?
        .simple()
        .to_string();
    let name = field(&profile, "name")?.to_owned();
    if !crate::game::local_profile::valid_nickname(&name) {
        return Err(CoreError::SignIn);
    }
    Ok(Identity {
        name,
        uuid,
        access,
        client,
        xuid: xsts
            .pointer("/DisplayClaims/xui/0/xid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
    })
}
pub fn refresh(db: &Database, id: &str) -> Result<Identity, CoreError> {
    let (uuid, encrypted): (String, Vec<u8>) = db.connect()?.query_row(
        "SELECT minecraft_uuid,refresh_secret FROM microsoft_accounts WHERE profile_id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let secret = String::from_utf8(protect(&encrypted, false)?).map_err(|_| CoreError::SignIn)?;
    let client = client_id()?;
    let http = http()?;
    let tokens = response(http.post(TOKEN).form(&[
        ("grant_type", "refresh_token"),
        ("client_id", &client),
        ("refresh_token", &secret),
        ("scope", SCOPE),
    ]))?;
    if let Some(next) = tokens["refresh_token"].as_str() {
        let changed = db.connect()?.execute(
            "UPDATE microsoft_accounts SET refresh_secret=?1 WHERE profile_id=?2 AND refresh_secret=?3",
            rusqlite::params![protect(next.as_bytes(), true)?, id, encrypted],
        )?;
        if changed != 1 {
            return Err(CoreError::RecordConflict);
        }
    }
    let identity = minecraft(&http, field(&tokens, "access_token")?, client)?;
    if identity.uuid != uuid {
        return Err(CoreError::SignIn);
    }
    Ok(identity)
}
struct Pending {
    ticket: String,
    device: String,
    profile: String,
    client: String,
    next: Instant,
    end: Instant,
    interval: Duration,
}
#[derive(Default)]
pub struct Login(Mutex<Option<Pending>>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    ticket: String,
    user_code: String,
    interval: u64,
    verification: String,
}
#[tauri::command]
pub async fn microsoft_begin(
    profile: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Challenge, CommandError> {
    use tauri::Manager;
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if !crate::profiles::snapshot(&db)?
            .profiles
            .iter()
            .any(|p| p.id == profile)
        {
            return Err(CoreError::NotFound);
        }
        let client = client_id()?;
        let v = response(
            http()?
                .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
                .form(&[("client_id", client.as_str()), ("scope", SCOPE)]),
        )?;
        let ticket = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        let interval = v["interval"].as_u64().unwrap_or(5).clamp(1, 60);
        let verification =
            reqwest::Url::parse(field(&v, "verification_uri")?).map_err(|_| CoreError::SignIn)?;
        if verification.scheme() != "https"
            || !matches!(
                verification.host_str(),
                Some("www.microsoft.com" | "microsoft.com" | "login.microsoftonline.com")
            )
            || !verification.username().is_empty()
            || verification.password().is_some()
        {
            return Err(CoreError::SignIn);
        }
        let challenge = Challenge {
            verification: verification.to_string(),
            ticket: ticket.clone(),
            user_code: field(&v, "user_code")?.to_owned(),
            interval,
        };
        let pending = Pending {
            ticket,
            device: field(&v, "device_code")?.into(),
            profile,
            client,
            next: now + Duration::from_secs(interval),
            end: now + Duration::from_secs(v["expires_in"].as_u64().unwrap_or(900).min(1800)),
            interval: Duration::from_secs(interval),
        };
        opener::open(verification.as_str()).map_err(|_| CoreError::OpenFolderFailed)?;
        *app.state::<Login>()
            .0
            .lock()
            .map_err(|_| CoreError::Worker)? = Some(pending);
        Ok(challenge)
    })
    .await
    .map_err(|_| CoreError::Worker)?
    .map_err(Into::into)
}
#[tauri::command]
pub fn microsoft_cancel(ticket: String, login: tauri::State<'_, Login>) {
    if let Ok(mut guard) = login.0.lock()
        && guard.as_ref().is_some_and(|p| p.ticket == ticket)
    {
        *guard = None
    }
}
#[tauri::command]
pub async fn microsoft_poll(
    ticket: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, CommandError> {
    use tauri::Manager;
    let db = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || poll(&db, &app.state::<Login>(), ticket))
        .await
        .map_err(|_| CoreError::Worker)?
        .map_err(Into::into)
}

fn poll(db: &Database, login: &Login, ticket: String) -> Result<bool, CoreError> {
    let (device, client, profile) = {
        let mut guard = login.0.lock().map_err(|_| CoreError::Worker)?;
        let p = guard
            .as_mut()
            .filter(|p| p.ticket == ticket)
            .ok_or(CoreError::Cancelled)?;
        if Instant::now() >= p.end {
            *guard = None;
            return Err(CoreError::SignIn);
        }
        if Instant::now() < p.next {
            return Ok(false);
        }
        p.next = Instant::now() + p.interval;
        (p.device.clone(), p.client.clone(), p.profile.clone())
    };
    let http = http()?;
    let v: Value = http
        .post(TOKEN)
        .form(&[
            ("client_id", client.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", &device),
        ])
        .send()
        .map_err(|_| CoreError::Network)?
        .json()
        .map_err(|_| CoreError::SignIn)?;
    match v["error"].as_str() {
        Some("authorization_pending") => return Ok(false),
        Some("slow_down") => {
            if let Some(p) = login
                .0
                .lock()
                .map_err(|_| CoreError::Worker)?
                .as_mut()
                .filter(|p| p.ticket == ticket)
            {
                p.interval += Duration::from_secs(5);
                p.next = Instant::now() + p.interval
            }
            return Ok(false);
        }
        Some(_) => return Err(CoreError::SignIn),
        None => {}
    }
    let identity = minecraft(&http, field(&v, "access_token")?, client)?;
    let secret = protect(field(&v, "refresh_token")?.as_bytes(), true)?;
    let mut guard = login.0.lock().map_err(|_| CoreError::Worker)?;
    if !guard.as_ref().is_some_and(|p| p.ticket == ticket) {
        return Err(CoreError::Cancelled);
    }
    let mut conn = db.connect()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let name: String = tx.query_row(
        "SELECT nickname FROM local_profiles WHERE id=?1",
        [&profile],
        |r| r.get(0),
    )?;
    if !name.eq_ignore_ascii_case(&identity.name) {
        return Err(CoreError::ProfileMismatch);
    }
    let updated=tx.execute("INSERT INTO microsoft_accounts(profile_id,minecraft_uuid,refresh_secret) VALUES(?1,?2,?3) ON CONFLICT(profile_id) DO UPDATE SET refresh_secret=excluded.refresh_secret WHERE microsoft_accounts.minecraft_uuid=excluded.minecraft_uuid",rusqlite::params![profile,identity.uuid,secret])?;
    if updated != 1 {
        return Err(CoreError::SignIn);
    }
    tx.execute(
        "UPDATE profile_state SET revision=revision+1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    *guard = None;
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_secrets_roundtrip_for_current_windows_user() {
        let encrypted = protect(b"test-only-value", true).unwrap();
        assert_ne!(encrypted, b"test-only-value");
        assert_eq!(protect(&encrypted, false).unwrap(), b"test-only-value");
    }
}
