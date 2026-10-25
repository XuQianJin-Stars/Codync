//! Which cloud base URL this computer uses, and the address a person saves.
//!
//! `CODYNC_CLOUD=off` wins, then the switch in settings, then `CODYNC_CLOUD_URL`, then the
//! saved `cloud_url`, then the build's cloud. Unit tests point at a fake cloud through kv
//! `test_cloud_url`, so they never reach a real one.

use super::cloud::{self, CloudStatus};
use crate::hub::Hub;
use crate::store::Store;
use anyhow::{Result, bail};

/// The cloud this build belongs to: main only for builds made with `CODYNC_ENV=main` (the
/// release workflows), dev for every local build, debug or release, like the apps built next
/// to it.
fn build_cloud_url() -> &'static str {
    crate::environment::Environment::current().cloud_url()
}

/// The cloud base URL in use, or `None` when the cloud is off.
pub fn url(store: &Store) -> Option<String> {
    current(store).map(|address| address.url)
}

/// The registration token for the cloud actually in use. Absent for the official relay.
pub(crate) fn relay_token(store: &Store) -> Option<String> {
    current(store).and_then(|address| address.token)
}

/// `off` wins. Otherwise the first valid candidate, then `builtin`. An illegal candidate is skipped.
#[cfg(test)]
fn choose_url(off: bool, candidates: [Option<String>; 2], builtin: &str) -> Option<String> {
    let [env, saved] = candidates;
    resolve(off, env.as_deref(), saved.as_deref(), None, builtin).map(|address| address.url)
}

struct Address {
    url: String,
    /// Registration secret from the URL's user name (`https://token@relay.example.com`). Never part of `url`.
    token: Option<String>,
}

/// The cloud in use: its public address, and the registration token when this computer has one.
fn current(store: &Store) -> Option<Address> {
    let off = std::env::var("CODYNC_CLOUD").is_ok_and(|v| v == "off")
        || store.kv_get("cloud_enabled").as_deref() == Some("false");
    if cfg!(test) {
        return if off { None } else { store.kv_get("test_cloud_url").map(|url| Address { url, token: None }) };
    }
    let env = std::env::var("CODYNC_CLOUD_URL").ok();
    let saved = store.kv_get("cloud_url");
    let saved_token = store.kv_get("cloud_relay_token");
    resolve(off, env.as_deref(), saved.as_deref(), saved_token.as_deref(), build_cloud_url())
}

/// `off` wins. Otherwise the first valid candidate, then `builtin`. An illegal candidate is skipped.
/// A token belongs to the candidate that won: the saved kv token is not sent to an env URL.
fn resolve(
    off: bool,
    env: Option<&str>,
    saved: Option<&str>,
    saved_token: Option<&str>,
    builtin: &str,
) -> Option<Address> {
    if off {
        return None;
    }
    for (raw, extra) in [(env, None), (saved, saved_token)] {
        let Some(raw) = raw else { continue };
        if let Some(mut address) = parse_address(raw) {
            if address.token.is_none() {
                address.token = extra.filter(|token| !token.is_empty()).map(str::to_owned);
            }
            return Some(address);
        }
    }
    parse_address(builtin).or_else(|| Some(Address { url: builtin.to_owned(), token: None }))
}

/// A stored or typed cloud URL: https, or plain http to this computer, with no path, query or fragment.
/// A user name is the registration token and is not part of the returned address.
#[cfg(test)]
fn normalize_url(raw: &str) -> Option<String> {
    parse_address(raw).map(|address| address.url)
}

/// https, or plain http to this computer (local dev). No path, query, fragment or password.
/// The user name, when present, is a header-safe registration token.
fn parse_address(raw: &str) -> Option<Address> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let parsed = reqwest::Url::parse(&format!("{trimmed}/")).ok()?;
    let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.password().is_some()
        || !(parsed.scheme() == "https" || (parsed.scheme() == "http" && local))
    {
        return None;
    }
    let token = match parsed.username() {
        "" => None,
        name => {
            let decoded = decode_percent(name)?;
            if !decoded.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
                return None;
            }
            Some(decoded)
        }
    };
    let host = parsed.host_str()?;
    let port = parsed.port().map_or(String::new(), |port| format!(":{port}"));
    Some(Address { url: format!("{}://{host}{port}", parsed.scheme()), token })
}

/// `Url::username` leaves percent-encoding in place. The relay compares the decoded secret.
fn decode_percent(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = hex_digit(*bytes.get(index + 1)?)?;
            let lo = hex_digit(*bytes.get(index + 2)?)?;
            out.push((hi << 4) | lo);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// `setCloudUrl`: points this computer at a cloud, or clears the saved URL so the build's cloud
/// is used again. Empty and null clear it. An illegal URL is refused and changes nothing.
/// The relay restarts either way. Local only.
pub fn set_cloud_url(hub: &Hub, raw: Option<&str>) -> Result<CloudStatus> {
    match raw.map(str::trim).filter(|u| !u.is_empty()) {
        None => {
            hub.store.kv_delete("cloud_url")?;
            hub.store.kv_delete("cloud_relay_token")?;
        }
        Some(raw) => {
            let Some(address) = parse_address(raw) else {
                bail!("That relay address isn't a usable cloud URL.");
            };
            hub.store.kv_set("cloud_url", &address.url)?;
            match address.token {
                Some(token) => hub.store.kv_set("cloud_relay_token", &token)?,
                None => hub.store.kv_delete("cloud_relay_token")?,
            }
        }
    }
    let now = url(&hub.store);
    cloud::update_status(hub, |s| {
        s.enabled = now.is_some();
        s.url.clone_from(&now);
    });
    hub.cloud.config.send_modify(|g| *g += 1);
    Ok(hub.cloud.status())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::remote::identity::Identity;
    use std::sync::Arc;

    fn temp_hub() -> Arc<Hub> {
        let dir = std::env::temp_dir().join(format!("codync-cloud-url-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("t.db")).unwrap();
        Hub::new(store, "host".into(), Identity::load_or_create(&dir).unwrap(), "token".into(), 0)
    }

    #[test]
    fn only_safe_cloud_urls() {
        assert_eq!(
            normalize_url("https://codync-cloud.example.workers.dev/").as_deref(),
            Some("https://codync-cloud.example.workers.dev")
        );
        assert_eq!(normalize_url("http://127.0.0.1:8787/").as_deref(), Some("http://127.0.0.1:8787"));
        assert_eq!(normalize_url("https://tok@relay.example.com/").as_deref(), Some("https://relay.example.com"));
        let encoded = parse_address("https://abcdefghijklmnopqrst%40@relay.example.com/").unwrap();
        assert_eq!(encoded.url, "https://relay.example.com");
        assert_eq!(encoded.token.as_deref(), Some("abcdefghijklmnopqrst@"));
        assert!(parse_address("https://bad%2@relay.example.com").is_none());
        assert!(parse_address("https://has%20space@relay.example.com").is_none());
        assert!(normalize_url("http://example.com/").is_none());
        assert!(normalize_url("https://example.com/path/").is_none());
        assert!(normalize_url("ftp://example.com/").is_none());
        assert!(normalize_url("https://relay.example.com/#setup").is_none());
        assert!(normalize_url("https://user:secret@relay.example.com").is_none());
    }

    #[test]
    fn a_saved_cloud_url_is_used_only_when_nothing_in_front_of_it_is() {
        assert_eq!(
            choose_url(false, [None, Some("https://relay.example.com/".into())], "https://api.codync.dev"),
            Some("https://relay.example.com".into())
        );
        assert_eq!(
            choose_url(
                false,
                [Some("https://from.env".into()), Some("https://saved.example".into())],
                "https://builtin"
            ),
            Some("https://from.env".into())
        );
        assert_eq!(
            choose_url(false, [Some("http://example.com".into()), None], "https://builtin"),
            Some("https://builtin".into()),
            "an illegal candidate is skipped"
        );
        assert_eq!(choose_url(true, [Some("https://relay.example.com".into()), None], "https://builtin"), None);
        assert_eq!(
            choose_url(false, [None, Some("https://tok@relay.example.com/#nope".into())], "https://builtin"),
            Some("https://builtin".into()),
            "a fragment is not a usable address"
        );
        let with_token =
            resolve(false, None, Some("https://relay.example.com"), Some("s3cret-token-value"), "https://builtin");
        let with_token = with_token.unwrap();
        assert_eq!(with_token.url, "https://relay.example.com");
        assert_eq!(with_token.token.as_deref(), Some("s3cret-token-value"));
        let from_env = resolve(
            false,
            Some("https://from.env"),
            Some("https://saved.example"),
            Some("saved-token"),
            "https://builtin",
        )
        .unwrap();
        assert_eq!(from_env.url, "https://from.env");
        assert_eq!(from_env.token, None, "the env address does not inherit the saved token");
    }

    #[test]
    fn set_cloud_url_stores_a_normalized_url_or_clears_it() {
        let hub = temp_hub();
        set_cloud_url(&hub, Some("https://relay.example.com/")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("https://relay.example.com"));
        let bad = set_cloud_url(&hub, Some("http://example.com")).unwrap_err();
        assert!(bad.to_string().contains("relay address"), "{bad}");
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("https://relay.example.com"));
        set_cloud_url(&hub, Some("  ")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url"), None);
        set_cloud_url(&hub, Some("http://127.0.0.1:8788")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("http://127.0.0.1:8788"));
        assert_eq!(hub.store.kv_get("cloud_relay_token"), None);
        set_cloud_url(&hub, Some("https://s3cret-token-value@relay.example.com/")).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url").as_deref(), Some("https://relay.example.com"));
        assert_eq!(hub.store.kv_get("cloud_relay_token").as_deref(), Some("s3cret-token-value"));
        let fragment = set_cloud_url(&hub, Some("https://relay.example.com/#setup")).unwrap_err();
        assert!(fragment.to_string().contains("relay address"), "{fragment}");
        assert_eq!(hub.store.kv_get("cloud_relay_token").as_deref(), Some("s3cret-token-value"));
        set_cloud_url(&hub, None).unwrap();
        assert_eq!(hub.store.kv_get("cloud_url"), None);
        assert_eq!(hub.store.kv_get("cloud_relay_token"), None);
    }
}
