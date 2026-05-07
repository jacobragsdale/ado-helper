use anyhow::{Context, Result};
use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;

/// Shared HTTP client pre-configured with auth and base URL for the ADO REST API.
pub struct AdoClient {
    http: Client,
    /// Full org URL, e.g. "https://dev.azure.com/myorg"
    pub org: String,
    /// Default project — subcommands use this when --project is not supplied
    pub project: String,
    /// PAT token — stored only in memory, never written to disk
    pat: String,
}

impl AdoClient {
    /*
     * IMPLEMENTATION NOTES — AdoClient::new()
     *
     * Build a reqwest::Client with rustls-tls (already selected in Cargo.toml via
     * default-features = false, features = ["rustls-tls"]).
     *
     * Validate that org and pat are non-empty; return an error if either is missing.
     * Trim trailing slashes from org so URL construction is always clean.
     *
     *   let http = Client::builder()
     *       .use_rustls_tls()
     *       .build()?;
     */
    pub fn new(org: String, project: String, pat: String) -> Result<Self> {
        todo!("construct AdoClient with a reqwest Client")
    }

    /*
     * IMPLEMENTATION NOTES — AdoClient::get() / post() / patch() / put()
     *
     * Each method constructs a RequestBuilder pre-populated with:
     *
     *   1. The full URL: format!("{}/{}", self.org, path)
     *      The `path` argument should start without a slash, e.g.:
     *        "_apis/git/repositories?api-version=7.1"
     *        "MyProject/_apis/git/repositories?api-version=7.1"
     *
     *   2. Basic auth: ADO uses an empty username and the PAT as the password.
     *      reqwest's basic_auth("", Some(&self.pat)) handles the base64 encoding.
     *
     *   3. Accept: application/json header.
     *
     * Return the RequestBuilder so callers can chain .json(&body).send().await.
     */
    pub fn get(&self, path: &str) -> RequestBuilder {
        todo!("return self.http.get(url).basic_auth(...).header(Accept, application/json)")
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        todo!("return self.http.post(url).basic_auth(...).header(Accept, application/json)")
    }

    pub fn patch(&self, path: &str) -> RequestBuilder {
        todo!("return self.http.patch(url).basic_auth(...).header(Accept, application/json)")
    }

    pub fn put(&self, path: &str) -> RequestBuilder {
        todo!("return self.http.put(url).basic_auth(...).header(Accept, application/json)")
    }

    /*
     * IMPLEMENTATION NOTES — AdoClient::check_response()
     *
     * ADO returns 4xx/5xx with a JSON body like:
     *   {"$id":"1","innerException":null,"message":"TF401019: ...","typeName":"..."}
     *
     * If response.status().is_success() is false:
     *   1. Read the body text: let body = response.text().await?
     *   2. Try to parse the "message" field from the JSON body.
     *   3. Return anyhow::bail!("ADO error {status}: {message}")
     *
     * If successful, return Ok(response) so callers can call .json::<T>().await?.
     */
    pub async fn check_response(response: Response) -> Result<Response> {
        todo!("check HTTP status, extract ADO error message on failure")
    }

    /*
     * IMPLEMENTATION NOTES — AdoClient::get_json()
     *
     * Convenience wrapper:
     *   1. Call self.get(path).send().await?
     *   2. Pass through check_response().
     *   3. Deserialize with .json::<T>().await?
     *
     * Usage:  let repos: RepoListResponse = client.get_json("proj/_apis/...").await?;
     */
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        todo!("send GET request and deserialize JSON response")
    }

    /*
     * IMPLEMENTATION NOTES — AdoClient::post_json()
     *
     * Same as get_json but sends a POST with a JSON body:
     *   1. Call self.post(path).json(&body).send().await?
     *   2. Pass through check_response().
     *   3. Deserialize with .json::<T>().await?
     */
    pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        todo!("send POST with JSON body and deserialize JSON response")
    }

    /*
     * IMPLEMENTATION NOTES — AdoClient::patch_json()
     *
     * Same pattern as post_json but uses PATCH. Used for:
     *   - Completing a PR (PATCH pullrequests/{id})
     *   - Updating a work item (PATCH workitems/{id} with JSON Patch content-type)
     *
     * Note: Work item updates require Content-Type: application/json-patch+json
     * (not application/json). Add an extra .header() call for those endpoints.
     */
    pub async fn patch_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        todo!("send PATCH with JSON body and deserialize JSON response")
    }

    /// Build a path scoped to the default project: "{project}/{path}"
    pub fn project_path(&self, path: &str) -> String {
        format!("{}/{}", self.project, path)
    }

    /// Open a URL in the default browser on Windows using `cmd /c start <url>`
    pub fn open_in_browser(url: &str) -> Result<()> {
        /*
         * IMPLEMENTATION NOTES — open_in_browser()
         *
         * On Windows, run:
         *   std::process::Command::new("cmd")
         *       .args(["/c", "start", url])
         *       .spawn()?;
         *
         * The `start` command is a cmd.exe built-in that opens the URL in the
         * default browser. We spawn (don't wait) because the browser opens async.
         *
         * If this tool is ever run on macOS for testing, fall back to:
         *   std::process::Command::new("open").arg(url).spawn()?;
         */
        todo!("shell out to cmd /c start <url> on Windows")
    }
}
