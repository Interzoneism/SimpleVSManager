//! HTTP access to the Mod DB. Async (reqwest on the caller's tokio runtime).

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::model::NamedTag;
use super::parse::{self, SITE_URL};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("Offline mode is on. Turn it off to talk to the Mod DB.")]
    Offline,
    #[error("Couldn't reach the Mod DB: {0}")]
    Http(String),
    #[error("The Mod DB sent something unexpected: {0}")]
    BadResponse(String),
    #[error("Couldn't save the download to {path}: {message}")]
    Io { path: String, message: String },
    #[error("Cancelled.")]
    Cancelled,
}

fn is_loopback(host: Option<&str>) -> bool {
    matches!(host, Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
}

/// Download addresses must be https. A plain http one is upgraded (the Mod DB and its CDN
/// serve https); only loopback (tests) may stay http. Anything else is refused.
pub fn secure_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.starts_with("https://") {
        return Some(url.to_string());
    }
    let rest = url.strip_prefix("http://")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_only = host.rsplit_once(':').map_or(host, |(h, p)| if p.chars().all(|c| c.is_ascii_digit()) { h } else { host });
    if is_loopback(Some(host_only)) {
        Some(url.to_string())
    } else {
        Some(format!("https://{rest}"))
    }
}

impl From<reqwest::Error> for NetError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest's Display hides the cause ("error sending request"); include the chain.
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        NetError::Http(msg)
    }
}

pub type NetResult<T> = Result<T, NetError>;

/// Byte progress of a download.
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub received: u64,
    pub total: Option<u64>,
}

#[derive(Clone)]
pub struct ModDbClient {
    http: reqwest::Client,
    base: String,
}

impl ModDbClient {
    pub fn new() -> NetResult<Self> {
        Self::with_base(SITE_URL)
    }

    /// For tests or mirrors.
    pub fn with_base(base: &str) -> NetResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("SimpleVSManager/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            // Never follow a redirect from https to plain http: a swapped mod zip is code
            // that runs in the game.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    attempt.error("too many redirects")
                } else if attempt.url().scheme() != "https" && !is_loopback(attempt.url().host_str()) {
                    attempt.error("refused a redirect to an insecure (http) address")
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self { http, base: base.trim_end_matches('/').to_string() })
    }

    async fn get_text(&self, url: &str) -> NetResult<String> {
        let resp = self.http.get(url).send().await?.error_for_status()?;
        Ok(resp.text().await?)
    }

    /// `/api/mod/<id>`: the raw `mod` object, or `None` if the Mod DB doesn't know the id.
    pub async fn fetch_mod(&self, mod_id: &str) -> NetResult<Option<Value>> {
        let url = format!("{}/api/mod/{}", self.base, urlencode(mod_id));
        let body = self.get_text(&url).await?;
        parse::mod_from_response(&body).map_err(NetError::BadResponse)
    }

    /// `/api/mods`, optionally limited to one game version tag. The API has no paging: this
    /// returns every matching mod (about 10,000, ~4 MB).
    pub async fn fetch_mod_list(&self, game_version_tag_id: Option<&str>) -> NetResult<Vec<Value>> {
        let mut url = format!("{}/api/mods?orderby=downloads&orderdirection=desc", self.base);
        if let Some(id) = game_version_tag_id {
            url.push_str("&gameversions[]=");
            url.push_str(&urlencode(id));
        }
        let body = self.get_text(&url).await?;
        parse::list_from_response(&body).map_err(NetError::BadResponse)
    }

    pub async fn fetch_tags(&self) -> NetResult<Vec<NamedTag>> {
        let body = self.get_text(&format!("{}/api/tags", self.base)).await?;
        parse::tags_from_response(&body, "tags").map_err(NetError::BadResponse)
    }

    pub async fn fetch_game_versions(&self) -> NetResult<Vec<NamedTag>> {
        let body = self.get_text(&format!("{}/api/gameversions", self.base)).await?;
        parse::tags_from_response(&body, "gameversions").map_err(NetError::BadResponse)
    }

    /// Streams `url` into `dest`, reporting progress. `cancelled` is polled between chunks.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        mut progress: impl FnMut(DownloadProgress),
        cancelled: impl Fn() -> bool,
    ) -> NetResult<u64> {
        use std::io::Write;
        let io_err = |e: std::io::Error| NetError::Io { path: dest.display().to_string(), message: e.to_string() };
        let url = secure_url(url).ok_or_else(|| NetError::BadResponse(format!("refused to download from an insecure address: {url}")))?;
        let mut resp = self.http.get(&url).send().await?.error_for_status()?;
        let total = resp.content_length();
        let mut file = std::fs::File::create(dest).map_err(io_err)?;
        let mut received = 0u64;
        progress(DownloadProgress { received, total });
        while let Some(chunk) = resp.chunk().await? {
            if cancelled() {
                drop(file);
                let _ = std::fs::remove_file(dest);
                return Err(NetError::Cancelled);
            }
            file.write_all(&chunk).map_err(io_err)?;
            received += chunk.len() as u64;
            progress(DownloadProgress { received, total });
        }
        file.sync_all().map_err(io_err)?;
        Ok(received)
    }
}

/// Percent-encodes everything except unreserved characters.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes() {
        assert_eq!(super::urlencode("carry on/ü"), "carry%20on%2F%C3%BC");
        assert_eq!(super::urlencode("-28149"), "-28149");
    }
}

#[cfg(test)]
mod secure_url_tests {
    use super::secure_url;

    #[test]
    fn http_is_upgraded_except_loopback() {
        assert_eq!(secure_url("https://moddbcdn.vintagestory.at/a.zip").as_deref(), Some("https://moddbcdn.vintagestory.at/a.zip"));
        assert_eq!(secure_url("http://moddbcdn.vintagestory.at/a.zip").as_deref(), Some("https://moddbcdn.vintagestory.at/a.zip"));
        assert_eq!(secure_url("http://127.0.0.1:1234/a.zip").as_deref(), Some("http://127.0.0.1:1234/a.zip"));
        assert_eq!(secure_url("ftp://x/a.zip"), None);
        assert_eq!(secure_url("file:///etc/passwd"), None);
    }
}
