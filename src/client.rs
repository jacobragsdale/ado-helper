use anyhow::{Context, Result, bail};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::CliError;

/// Shared HTTP client pre-configured with auth and base URL for the ADO REST API.
pub struct AdoClient {
    http: Client,
    /// Full org URL, e.g. "https://dev.azure.com/myorg" (no trailing slash)
    pub org: String,
    /// Default project — subcommands use this when --project is not supplied
    pub project: String,
    /// PAT token — stored only in memory, never written to disk
    pat: String,
    /// When true, mutating helpers print the would-be call and bail with
    /// `CliError::Explain` instead of touching ADO. Set by `main` from the
    /// global `--explain` flag.
    explain: AtomicBool,
}

impl AdoClient {
    pub fn new(org: String, project: String, pat: String) -> Result<Self> {
        if org.trim().is_empty() {
            bail!("org URL is empty");
        }
        if pat.trim().is_empty() {
            bail!("PAT is empty");
        }
        let org = org.trim_end_matches('/').to_string();
        let http = Client::builder().use_rustls_tls().build()?;
        Ok(Self {
            http,
            org,
            project,
            pat,
            explain: AtomicBool::new(false),
        })
    }

    pub fn set_explain(&self, explain: bool) {
        self.explain.store(explain, Ordering::Relaxed);
    }

    pub fn explain_enabled(&self) -> bool {
        self.explain.load(Ordering::Relaxed)
    }

    /// Print a would-be REST call and bail with `CliError::Explain` so the
    /// runtime treats the dry-run as success.
    fn dry_run<T>(&self, method: &str, path: &str, body: Option<&str>) -> Result<T> {
        eprintln!("DRY-RUN: {method} {url}", url = self.url(path));
        if let Some(b) = body {
            eprintln!("{b}");
        }
        Err(CliError::Explain.into())
    }

    /// Mutation guard for handlers that build a `RequestBuilder` by hand (e.g.
    /// raw-body POSTs). Returns `Some(err)` under `--explain` (after printing
    /// the would-be call) or `None` when the caller should proceed with the
    /// real request.
    pub fn explain_skip(
        &self,
        method: &str,
        path: &str,
        body_note: Option<&str>,
    ) -> Option<anyhow::Error> {
        if !self.explain_enabled() {
            return None;
        }
        eprintln!("DRY-RUN: {method} {url}", url = self.url(path));
        if let Some(b) = body_note {
            eprintln!("{b}");
        }
        Some(CliError::Explain.into())
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.org, path.trim_start_matches('/'))
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.http
            .get(self.url(path))
            .basic_auth("", Some(&self.pat))
            .header("Accept", "application/json")
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.http
            .post(self.url(path))
            .basic_auth("", Some(&self.pat))
            .header("Accept", "application/json")
    }

    pub fn patch(&self, path: &str) -> RequestBuilder {
        self.http
            .patch(self.url(path))
            .basic_auth("", Some(&self.pat))
            .header("Accept", "application/json")
    }

    pub fn put(&self, path: &str) -> RequestBuilder {
        self.http
            .put(self.url(path))
            .basic_auth("", Some(&self.pat))
            .header("Accept", "application/json")
    }

    pub fn delete(&self, path: &str) -> RequestBuilder {
        self.http
            .delete(self.url(path))
            .basic_auth("", Some(&self.pat))
            .header("Accept", "application/json")
    }

    pub async fn check_response(response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
            .unwrap_or(body);
        let formatted = format!("ADO error {status}: {message}");
        let err = match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => CliError::Auth(formatted),
            StatusCode::NOT_FOUND => CliError::NotFound(formatted),
            _ => CliError::Api(formatted),
        };
        Err(err.into())
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.get(path).send().await.context("GET request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>()
            .await
            .context("failed to parse JSON response")
    }

    /// GET an absolute URL and return its text body. Used for ADO signed log
    /// URLs, which are already authorized by their query string.
    pub async fn get_absolute_text(&self, url: &str) -> Result<String> {
        let resp = self
            .http
            .get(url)
            .header("Accept", "text/plain")
            .send()
            .await
            .context("GET request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.text().await.context("failed to read text response")
    }

    pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        if self.explain_enabled() {
            return self.dry_run("POST", path, serialize_for_explain(body).as_deref());
        }
        let resp = self
            .post(path)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("POST request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>()
            .await
            .context("failed to parse JSON response")
    }

    /// PATCH with a regular JSON body.
    pub async fn patch_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        if self.explain_enabled() {
            return self.dry_run("PATCH", path, serialize_for_explain(body).as_deref());
        }
        let resp = self
            .patch(path)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("PATCH request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>()
            .await
            .context("failed to parse JSON response")
    }

    /// PUT with a JSON body.
    pub async fn put_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        if self.explain_enabled() {
            return self.dry_run("PUT", path, serialize_for_explain(body).as_deref());
        }
        let resp = self
            .put(path)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("PUT request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>()
            .await
            .context("failed to parse JSON response")
    }

    /// PATCH with a JSON Patch body — required content-type for work item updates.
    pub async fn patch_json_patch<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        if self.explain_enabled() {
            return self.dry_run(
                "PATCH (json-patch)",
                path,
                serialize_for_explain(body).as_deref(),
            );
        }
        let resp = self
            .patch(path)
            .header("Content-Type", "application/json-patch+json")
            .json(body)
            .send()
            .await
            .context("PATCH request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>()
            .await
            .context("failed to parse JSON response")
    }

    /// DELETE that doesn't expect a JSON body in response.
    pub async fn delete_no_body(&self, path: &str) -> Result<()> {
        if self.explain_enabled() {
            return self.dry_run("DELETE", path, None);
        }
        let resp = self
            .delete(path)
            .send()
            .await
            .context("DELETE request failed")?;
        Self::check_response(resp).await?;
        Ok(())
    }

    /// PAT accessor — used by `repo clone` to embed credentials in the clone URL.
    /// Not for general use; everything else should go through the auth'd helpers.
    pub fn pat(&self) -> &str {
        &self.pat
    }

    /// Open a URL in the default browser. Cross-platform: macOS, Linux, Windows.
    pub fn open_in_browser(url: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        let (program, args): (&str, Vec<&str>) = ("open", vec![url]);
        #[cfg(target_os = "windows")]
        let (program, args): (&str, Vec<&str>) = ("cmd", vec!["/c", "start", "", url]);
        #[cfg(all(unix, not(target_os = "macos")))]
        let (program, args): (&str, Vec<&str>) = ("xdg-open", vec![url]);

        std::process::Command::new(program)
            .args(&args)
            .spawn()
            .with_context(|| format!("failed to spawn {program}"))?;
        Ok(())
    }
}

fn serialize_for_explain<B: serde::Serialize>(body: &B) -> Option<String> {
    serde_json::to_string_pretty(body).ok()
}

/// Percent-encode a single URL path or query component using UTF-8 bytes.
pub fn encode_path_segment(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(*b).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}
