//! Google Drive source: collects raw files from folders shared with a service account.
//!
//! Native Google Docs/Sheets/Slides and shortcuts are skipped. Duplicate names get a short id suffix.
//! Each run lists the configured folders, downloads what is new or changed, and marks files that
//! disappeared from Drive as deleted (their data follows the normal retention rules).

use crate::integrations::syncsource::{self, Download, Item};
use crate::integrations::{Outcome, SourceRow};
use crate::platform::db::Db;
use crate::platform::store::Store;
use anyhow::{anyhow, bail, Context, Result};
use fafnirx_common::{clean_remote_path, unix_now, ChunkHasher};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use unicode_normalization::UnicodeNormalization;

const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const NATIVE_PREFIX: &str = "application/vnd.google-apps.";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

#[derive(Deserialize)]
pub struct Config {
    pub service_account: ServiceAccount,
    /// Drive folder ids (or Shared Drive ids) shared with the service account.
    pub folders: Vec<String>,
}

#[derive(Deserialize)]
pub struct ServiceAccount {
    pub client_email: String,
    pub private_key: String,
    #[serde(default)]
    pub token_uri: Option<String>,
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    files: Vec<Entry>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize, Clone)]
struct Entry {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default, rename = "md5Checksum")]
    md5: Option<String>,
    #[serde(default, rename = "modifiedTime")]
    modified: Option<String>,
}

/// Replaces characters that are unsafe in file names on common filesystems and normalizes to NFC.
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String =
        name.nfc().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let cleaned = cleaned.trim_matches(|c| c == ' ' || c == '.').to_string();
    if cleaned.is_empty() {
        "_".into()
    } else {
        cleaned
    }
}

/// Names for the children of one folder: unique case-insensitively, stable across runs.
/// The entry with the smallest id keeps its name; the others get ` (<id prefix>)` before the extension.
fn unique_names(entries: &[Entry]) -> Vec<String> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| entries[a].id.cmp(&entries[b].id));
    let mut taken = HashSet::new();
    let mut names = vec![String::new(); entries.len()];
    for i in order {
        let base = sanitize_name(&entries[i].name);
        let mut candidate = base.clone();
        let mut take = 6;
        while !taken.insert(candidate.to_lowercase()) {
            let id = &entries[i].id;
            let tag: String = id.chars().take(take).collect();
            candidate = match base.rfind('.').filter(|&dot| dot > 0) {
                Some(dot) => format!("{} ({}){}", &base[..dot], tag, &base[dot..]),
                None => format!("{base} ({tag})"),
            };
            take += 4;
        }
        names[i] = candidate;
    }
    names
}

pub(super) fn parse_time_ns(s: &str) -> i64 {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|t| t.unix_timestamp_nanos() as i64)
        .unwrap_or(0)
}

struct Drive {
    http: reqwest::Client,
    api: String,
    account: ServiceAccount,
    token: tokio::sync::Mutex<Option<(String, std::time::Instant)>>,
}

impl Drive {
    fn new(account: ServiceAccount) -> Result<Self> {
        let api = std::env::var("FAFNIRX_GDRIVE_API").unwrap_or_else(|_| "https://www.googleapis.com".into());
        Ok(Self {
            http: reqwest::Client::builder().timeout(Duration::from_secs(300)).user_agent("fafnirx").build()?,
            api: api.trim_end_matches('/').to_string(),
            account,
            token: tokio::sync::Mutex::new(None),
        })
    }

    async fn access_token(&self, force: bool) -> Result<String> {
        let mut cached = self.token.lock().await;
        if let Some((t, expires)) = cached.as_ref() {
            if !force && *expires > std::time::Instant::now() + Duration::from_secs(60) {
                return Ok(t.clone());
            }
        }
        #[derive(serde::Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            scope: &'a str,
            aud: &'a str,
            iat: u64,
            exp: u64,
        }
        let uri = self.account.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
        let iat = unix_now() as u64;
        let claims = Claims { iss: &self.account.client_email, scope: SCOPE, aud: uri, iat, exp: iat + 3600 };
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(self.account.private_key.as_bytes())
            .context("the service account private_key is not a valid RSA key")?;
        let jwt = jsonwebtoken::encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256), &claims, &key)?;
        let resp = self
            .http
            .post(uri)
            .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", jwt.as_str())])
            .send()
            .await
            .context("request access token")?;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        if !status.is_success() {
            bail!(
                "token request failed ({status}): {}",
                body["error_description"].as_str().or(body["error"].as_str()).unwrap_or("no detail")
            );
        }
        let token =
            body["access_token"].as_str().ok_or_else(|| anyhow!("token response has no access_token"))?.to_string();
        let ttl = body["expires_in"].as_u64().unwrap_or(3600);
        *cached = Some((token.clone(), std::time::Instant::now() + Duration::from_secs(ttl)));
        Ok(token)
    }

    /// GET with a bearer token, retrying rate limits and server errors.
    async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.api, path);
        let mut force_token = false;
        for attempt in 0..4u32 {
            let token = self.access_token(force_token).await?;
            force_token = false;
            let resp = self.http.get(&url).bearer_auth(token).query(query).send().await;
            match resp {
                Ok(r) if r.status().is_success() => return Ok(r),
                Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 => force_token = true,
                // Rate limits and server errors are worth retrying; Drive reports some rate limits as 403.
                Ok(r)
                    if r.status().as_u16() == 429
                        || r.status().is_server_error()
                        || (r.status().as_u16() == 403 && attempt < 2) =>
                {
                    tracing::warn!("drive {path}: {} (attempt {})", r.status(), attempt + 1);
                }
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    bail!("drive {path} failed ({status}): {}", body.chars().take(300).collect::<String>());
                }
                Err(e) if attempt == 3 => return Err(e).context(format!("drive {path}")),
                Err(e) => tracing::warn!("drive {path}: {e} (attempt {})", attempt + 1),
            }
            tokio::time::sleep(Duration::from_millis(500 << attempt)).await;
        }
        bail!("drive {path}: giving up after retries")
    }

    async fn folder_name(&self, id: &str) -> Result<String> {
        let r =
            self.get(&format!("/drive/v3/files/{id}"), &[("fields", "name"), ("supportsAllDrives", "true")]).await?;
        let v: serde_json::Value = r.json().await?;
        Ok(v["name"].as_str().unwrap_or(id).to_string())
    }

    async fn children(&self, folder: &str) -> Result<Vec<Entry>> {
        let q = format!("'{folder}' in parents and trashed = false");
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut query = vec![
                ("q", q.as_str()),
                ("fields", "nextPageToken,files(id,name,mimeType,size,md5Checksum,modifiedTime)"),
                ("pageSize", "1000"),
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ];
            if let Some(p) = page.as_deref() {
                query.push(("pageToken", p));
            }
            let listing: Listing =
                self.get("/drive/v3/files", &query).await?.json().await.context("parse file listing")?;
            out.extend(listing.files);
            match listing.next_page_token {
                Some(t) if !t.is_empty() => page = Some(t),
                _ => return Ok(out),
            }
        }
    }

    /// Every collectable file under the configured folders.
    async fn list(&self, folders: &[String]) -> Result<(Vec<Item>, usize)> {
        let mut items = Vec::new();
        let mut skipped = 0;
        let mut seen_ids = HashSet::new();
        let mut visited = HashSet::new();
        let mut tops = Vec::new();
        for id in folders {
            tops.push(Entry {
                id: id.clone(),
                name: self
                    .folder_name(id)
                    .await
                    .with_context(|| format!("folder {id} (is it shared with the service account?)"))?,
                mime_type: FOLDER_MIME.into(),
                size: None,
                md5: None,
                modified: None,
            });
        }
        let top_names = unique_names(&tops);
        let mut stack: Vec<(String, String)> = tops.iter().zip(top_names).map(|(e, n)| (e.id.clone(), n)).collect();
        while let Some((folder, prefix)) = stack.pop() {
            if !visited.insert(folder.clone()) {
                continue;
            }
            let entries = self.children(&folder).await.with_context(|| format!("list {prefix}"))?;
            let names = unique_names(&entries);
            for (e, name) in entries.iter().zip(names) {
                let path = format!("{prefix}/{name}");
                if e.mime_type == FOLDER_MIME {
                    stack.push((e.id.clone(), path));
                } else if e.mime_type.starts_with(NATIVE_PREFIX) {
                    skipped += 1; // Google Docs/Sheets/Slides, shortcuts, forms...
                } else if seen_ids.insert(e.id.clone()) {
                    if clean_remote_path(&path).is_err() {
                        skipped += 1;
                        continue;
                    }
                    let modified = e.modified.clone().unwrap_or_default();
                    items.push(Item {
                        id: e.id.clone(),
                        path,
                        marker: format!("{}:{modified}", e.md5.as_deref().unwrap_or("")),
                        size: e.size.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0),
                        mtime_ns: parse_time_ns(&modified),
                    });
                }
            }
        }
        Ok((items, skipped))
    }

    /// Streams a file into a temporary file. Returns (path, root hash, size).
    async fn download(&self, item: &Item, store: &Store) -> Result<Download> {
        let mut resp = self
            .get(&format!("/drive/v3/files/{}", item.id), &[("alt", "media"), ("supportsAllDrives", "true")])
            .await?;
        let path = store.temp_path();
        let received = async {
            let mut file = tokio::fs::File::create(&path).await.context("create temporary file")?;
            let mut hasher = ChunkHasher::default();
            while let Some(part) = resp.chunk().await.context("download interrupted")? {
                hasher.update(&part);
                file.write_all(&part).await?;
            }
            file.flush().await?;
            Ok::<_, anyhow::Error>(hasher.finish())
        }
        .await;
        let (root, total) = match received {
            Ok(done) => done,
            Err(e) => {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(e);
            }
        };
        if item.size != 0 && total != item.size {
            let _ = tokio::fs::remove_file(&path).await;
            bail!("received {total} bytes, Drive lists {}", item.size);
        }
        Ok(Download { path, root, size: total })
    }
}

pub async fn run(db: Db, store: Arc<Store>, src: SourceRow) -> Result<Outcome> {
    let cfg: Config = serde_json::from_str(&src.config).context("source configuration is invalid")?;
    if cfg.folders.is_empty() {
        bail!("no folders configured");
    }
    let drive = Drive::new(cfg.service_account)?;
    let (items, skipped) = drive.list(&cfg.folders).await?;
    let fetch_store = store.clone();
    let drive = &drive;
    syncsource::apply(db, store, src.id, src.device, items, skipped, |item| {
        let store = fetch_store.clone();
        async move { drive.download(&item, &store).await }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str) -> Entry {
        Entry {
            id: id.into(),
            name: name.into(),
            mime_type: "text/plain".into(),
            size: None,
            md5: None,
            modified: None,
        }
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(sanitize_name("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_name(".."), "_");
        assert_eq!(sanitize_name("  report.  "), "report");
        assert_eq!(sanitize_name("e\u{301}.txt"), "\u{e9}.txt");
        assert_eq!(sanitize_name("tab\there"), "tab_here");
    }

    #[test]
    fn duplicate_names_get_a_stable_suffix() {
        let a = unique_names(&[entry("zzz111", "x.txt"), entry("aaa222", "x.txt"), entry("mmm333", "X.TXT")]);
        assert_eq!(a[1], "x.txt", "the smallest id keeps the name");
        assert_eq!(a[0], "x (zzz111).txt");
        assert_eq!(a[2], "X (mmm333).TXT");
        // The same siblings in another order produce the same names.
        let b = unique_names(&[entry("aaa222", "x.txt"), entry("mmm333", "X.TXT"), entry("zzz111", "x.txt")]);
        assert_eq!((&b[0], &b[1], &b[2]), (&a[1], &a[2], &a[0]));
        assert_eq!(unique_names(&[entry("id1", "noext"), entry("id2", "noext")])[1], "noext (id2)");
    }

    #[test]
    fn rfc3339_to_unix_nanoseconds() {
        assert_eq!(parse_time_ns("1970-01-01T00:00:01.5Z"), 1_500_000_000);
        assert_eq!(parse_time_ns("garbage"), 0);
    }
}
