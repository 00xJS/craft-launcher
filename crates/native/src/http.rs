//! HTTPS requests: small JSON/text answers, redirect probes, and verified downloads.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

const USER_AGENT: &str = concat!("ArtCraftLauncher/", env!("CARGO_PKG_VERSION"));
/// Largest JSON or text answer we read.
const MAX_SMALL: u64 = 4 << 20;

/// What a GET of a small resource produced.
#[derive(Debug, PartialEq, Eq)]
pub enum Fetched {
    Body {
        text: String,
        etag: Option<String>,
    },
    /// `If-None-Match` matched: the saved copy is current.
    NotModified,
    NotFound,
    /// GitHub's API refused: out of quota until `reset` (Unix seconds), if it said.
    RateLimited {
        reset: Option<i64>,
    },
}

#[derive(Clone)]
pub struct Http {
    small: ureq::Agent,
    no_redirect: ureq::Agent,
    download: ureq::Agent,
    token: Option<String>,
}

impl Http {
    pub fn new() -> Result<Self, String> {
        let roots = os_roots()?;
        let tls = || ureq::tls::TlsConfig::builder().root_certs(roots.clone()).build();
        let small = ureq::Agent::config_builder()
            .tls_config(tls())
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .build()
            .new_agent();
        let no_redirect = ureq::Agent::config_builder()
            .tls_config(tls())
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(USER_AGENT)
            .build()
            .new_agent();
        let download = ureq::Agent::config_builder()
            .tls_config(tls())
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .build()
            .new_agent();
        // An optional token raises GitHub's limit from 60 to 5000 requests an hour. Only ever sent
        // to api.github.com.
        let token = std::env::var("ARTCRAFT_LAUNCHER_GITHUB_TOKEN").ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        Ok(Self { small, no_redirect, download, token })
    }

    /// GET a GitHub API resource, revalidating with `etag` when given.
    pub fn get_api(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String> {
        let mut req = self.small.get(url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(tag) = etag {
            req = req.header("If-None-Match", tag);
        }
        if let Some(token) = &self.token
            && url.starts_with("https://api.github.com/")
        {
            req = req.header("Authorization", &format!("Bearer {token}"));
        }
        let mut resp = req.call().map_err(|e| format!("couldn't reach GitHub ({e})"))?;
        let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
        let status = resp.status().as_u16();
        match status {
            200 => {
                let etag = header("etag");
                let text =
                    resp.body_mut().with_config().limit(MAX_SMALL).read_to_string().map_err(|e| format!("unreadable answer from GitHub ({e})"))?;
                Ok(Fetched::Body { text, etag })
            }
            304 => Ok(Fetched::NotModified),
            404 => Ok(Fetched::NotFound),
            403 | 429 => {
                let remaining = header("x-ratelimit-remaining");
                let reset = header("x-ratelimit-reset").and_then(|r| r.parse::<i64>().ok());
                let retry_after = header("retry-after").and_then(|r| r.parse::<i64>().ok()).map(|s| crate::now() + s);
                if status == 429 || remaining.as_deref() == Some("0") || retry_after.is_some() {
                    Ok(Fetched::RateLimited { reset: reset.or(retry_after) })
                } else {
                    Err(format!("GitHub refused the request (HTTP {status})"))
                }
            }
            s => Err(format!("GitHub answered HTTP {s}")),
        }
    }

    /// GET a small text file (following redirects).
    pub fn get_text(&self, url: &str) -> Result<Option<String>, String> {
        let mut resp = self.small.get(url).call().map_err(|e| format!("couldn't reach {} ({e})", host(url)))?;
        match resp.status().as_u16() {
            200 => resp.body_mut().with_config().limit(MAX_SMALL).read_to_string().map(Some).map_err(|e| format!("unreadable answer ({e})")),
            404 => Ok(None),
            s => Err(format!("{} answered HTTP {s}", host(url))),
        }
    }

    /// Where `url` redirects to, without following it.
    pub fn redirect_target(&self, url: &str) -> Result<Option<String>, String> {
        let resp = self.no_redirect.head(url).call().map_err(|e| format!("couldn't reach {} ({e})", host(url)))?;
        match resp.status().as_u16() {
            300..=399 => Ok(resp.headers().get("location").and_then(|v| v.to_str().ok()).map(str::to_string)),
            404 => Ok(None),
            s => Err(format!("{} answered HTTP {s}", host(url))),
        }
    }

    /// Downloads `url` to `dest`, checking the size (when known) and SHA-256 as it goes.
    /// `progress(done, total)` is called often; `cancel` stops the download.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        size: u64,
        sha256: &str,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), String> {
        let mut resp = self.download.get(url).call().map_err(|e| format!("couldn't start the download ({e})"))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(format!("the download failed (HTTP {status})"));
        }
        let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(size);
        if size > 0 && total != size {
            return Err(format!("the server offered {total} bytes, but the release lists {size}"));
        }
        let limit = if total > 0 { total } else { 8 << 30 };
        let mut reader = resp.body_mut().with_config().limit(limit).reader();
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {} ({e})", dir.display()))?;
        }
        let part = dest.with_extension("part");
        let mut file = std::fs::File::create(&part).map_err(|e| format!("couldn't save the download ({e})"))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done: u64 = 0;
        progress(0, total);
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err("cancelled".into());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("the download was interrupted ({e})"))?;
            if n == 0 {
                break;
            }
            let chunk = buf.get(..n).ok_or("read past the buffer")?;
            hasher.update(chunk);
            file.write_all(chunk).map_err(|e| format!("couldn't save the download ({e})"))?;
            done = done.saturating_add(n as u64);
            progress(done, total);
        }
        file.sync_all().map_err(|e| format!("couldn't save the download ({e})"))?;
        drop(file);
        if total > 0 && done != total {
            let _ = std::fs::remove_file(&part);
            return Err(format!("the download stopped early ({done} of {total} bytes)"));
        }
        let got = hex(&hasher.finalize());
        if !got.eq_ignore_ascii_case(sha256) {
            let _ = std::fs::remove_file(&part);
            return Err("the download doesn't match its published checksum, so it wasn't installed".into());
        }
        std::fs::rename(&part, dest).map_err(|e| format!("couldn't save the download ({e})"))
    }
}

/// The certificate authorities the operating system trusts.
fn os_roots() -> Result<ureq::tls::RootCerts, String> {
    let found = rustls_native_certs::load_native_certs();
    let certs: Vec<ureq::tls::Certificate<'static>> = found.certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
    if certs.is_empty() {
        return Err("no trusted certificates found on this system".into());
    }
    Ok(ureq::tls::RootCerts::new_with_certs(&certs))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// SHA-256 of a file on disk.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("couldn't read {} ({e})", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("couldn't read {} ({e})", path.display()))?;
    Ok(hex(&hasher.finalize()))
}

fn host(url: &str) -> &str {
    url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url)
}
