use anyhow::{Context, Result, bail};
use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;

/// Shared HTTP client pre-configured with auth and base URL for the ADO REST API.
pub struct AdoClient {
    http: Client,
    /// Full org URL, e.g. "https://dev.azure.com/myorg" (no trailing slash)
    pub org: String,
    /// Default project — subcommands use this when --project is not supplied
    pub project: String,
    /// PAT token — stored only in memory, never written to disk
    pat: String,
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
        Ok(Self { http, org, project, pat })
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
        bail!("ADO error {status}: {message}")
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.get(path).send().await.context("GET request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>().await.context("failed to parse JSON response")
    }

    pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .post(path)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("POST request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>().await.context("failed to parse JSON response")
    }

    /// PATCH with a JSON Patch body — required content-type for work item updates.
    pub async fn patch_json_patch<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .patch(path)
            .header("Content-Type", "application/json-patch+json")
            .json(body)
            .send()
            .await
            .context("PATCH request failed")?;
        let resp = Self::check_response(resp).await?;
        resp.json::<T>().await.context("failed to parse JSON response")
    }

    /// DELETE that doesn't expect a JSON body in response.
    pub async fn delete_no_body(&self, path: &str) -> Result<()> {
        let resp = self.delete(path).send().await.context("DELETE request failed")?;
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
