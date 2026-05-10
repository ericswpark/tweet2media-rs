use super::media::extract_media_urls;
use super::shared::{
    default_user_agent, ensure_csrf_cookie, parse_cookie_header_into, parse_response_json,
    tweet_from_result_response, tweet_id_from_url, tweet_result_params, twitter_headers,
    update_cookies_from_headers, TwitterError, API_ROOT, BEARER_TOKEN, GUEST_API_ROOT, WEB_ROOT,
};
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

/// Fetches a tweet URL through an anonymous guest session and returns all image/video media URLs.
///
/// This convenience function creates a new [`TwitterClient`] for one request. If you are fetching
/// several tweets, build and reuse a client instead so the guest token can be reused.
///
/// For authenticated-only tweets, use [`TwitterClient::builder`] and provide cookies from a logged-in
/// browser session (`auth_token` and `ct0` are the important ones).
///
/// # Examples
///
/// ```no_run
/// # fn main() -> Result<(), tweet2media::TwitterError> {
/// let urls = tweet2media::media_urls_from_tweet_url(
///     "https://x.com/example/status/1234567890123456789",
/// )?;
/// # let _ = urls;
/// # Ok(())
/// # }
/// ```
pub fn media_urls_from_tweet_url(tweet_url: &str) -> Result<Vec<String>, TwitterError> {
    TwitterClient::builder()
        .build()?
        .media_urls_from_tweet_url(tweet_url)
}

/// Builder for a blocking [`TwitterClient`].
///
/// Use this builder when you need to provide logged-in cookies, set a custom user agent or timeout,
/// or inject a preconfigured [`reqwest::blocking::Client`].
#[derive(Debug, Clone)]
pub struct TwitterClientBuilder {
    cookies: BTreeMap<String, String>,
    user_agent: String,
    timeout: Duration,
    client_transaction_id: Option<String>,
    client: Option<Client>,
}

impl Default for TwitterClientBuilder {
    fn default() -> Self {
        Self {
            cookies: BTreeMap::new(),
            user_agent: default_user_agent(),
            timeout: Duration::from_secs(30),
            client_transaction_id: None,
            client: None,
        }
    }
}

impl TwitterClientBuilder {
    /// Adds one cookie, e.g. `auth_token` or `ct0`, from a logged-in browser session.
    pub fn cookie(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.cookies.insert(name.into(), value.into());
        self
    }

    /// Adds cookies from a normal `Cookie` header string, e.g. `"auth_token=...; ct0=..."`.
    pub fn cookie_header(mut self, cookie_header: &str) -> Self {
        parse_cookie_header_into(&mut self.cookies, cookie_header);
        self
    }

    /// Sets the `User-Agent` header used for Twitter/X requests.
    ///
    /// The default is a desktop browser user agent. Supplying the same user agent as the browser
    /// session that provided your cookies can help avoid authentication mismatches.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Sets the timeout used when this builder constructs its own reqwest client.
    ///
    /// This is ignored if [`Self::client`] is used with a preconfigured client.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Uses a caller-provided `reqwest::blocking::Client`.
    ///
    /// This is useful for applications that need proxy configuration, proxy authentication,
    /// custom TLS settings, custom DNS, or shared connection pools. The library still adds
    /// request-specific Twitter/X headers and its own in-memory `Cookie` header on each request.
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Optional support for logged-in sessions that require X's web transaction header.
    ///
    /// gallery-dl computes this dynamically from X's web scripts. For the single-tweet endpoint,
    /// anonymous guest sessions usually do not need it; if your logged-in cookies are rejected,
    /// pass a fresh `x-client-transaction-id` captured from your browser's network requests.
    pub fn client_transaction_id(mut self, value: impl Into<String>) -> Self {
        self.client_transaction_id = Some(value.into());
        self
    }

    /// Builds a blocking [`TwitterClient`].
    ///
    /// If no `ct0` CSRF cookie was provided, a random one is generated for guest-session requests.
    /// Construction can fail if the underlying reqwest client cannot be built.
    pub fn build(mut self) -> Result<TwitterClient, TwitterError> {
        ensure_csrf_cookie(&mut self.cookies);

        let client = match self.client {
            Some(client) => client,
            None => Client::builder()
                .timeout(self.timeout)
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()?,
        };

        Ok(TwitterClient {
            client,
            cookies: self.cookies,
            user_agent: self.user_agent,
            guest_token: None,
            client_transaction_id: self.client_transaction_id,
        })
    }
}

/// Blocking Twitter/X media extraction client.
///
/// A client stores cookies and a guest token in memory. Reuse the same client for multiple requests
/// when possible, especially with [`TwitterClient::media_urls_from_tweet_url_mut`] or
/// [`TwitterClient::media_urls_from_tweet_id`].
#[derive(Debug, Clone)]
pub struct TwitterClient {
    client: Client,
    cookies: BTreeMap<String, String>,
    user_agent: String,
    guest_token: Option<String>,
    client_transaction_id: Option<String>,
}

impl TwitterClient {
    /// Creates a builder for configuring a blocking client.
    pub fn builder() -> TwitterClientBuilder {
        TwitterClientBuilder::default()
    }

    /// Fetches media URLs from a tweet URL, consuming the client.
    ///
    /// This is convenient when you only need one request from an already-built client. Use
    /// [`Self::media_urls_from_tweet_url_mut`] if you want to keep using the same client after the
    /// request.
    pub fn media_urls_from_tweet_url(
        mut self,
        tweet_url: &str,
    ) -> Result<Vec<String>, TwitterError> {
        self.media_urls_from_tweet_url_mut(tweet_url)
    }

    /// Fetches media URLs from a tweet URL while preserving the client for reuse.
    ///
    /// The URL must contain a numeric `/status/<id>` path segment. Both `x.com` and `twitter.com`
    /// URLs are accepted.
    pub fn media_urls_from_tweet_url_mut(
        &mut self,
        tweet_url: &str,
    ) -> Result<Vec<String>, TwitterError> {
        let tweet_id = tweet_id_from_url(tweet_url)?;
        self.media_urls_from_tweet_id(&tweet_id)
    }

    /// Fetches media URLs from a numeric tweet ID.
    ///
    /// Returns an empty vector when the tweet is accessible but contains no supported image, GIF, or
    /// video media.
    pub fn media_urls_from_tweet_id(
        &mut self,
        tweet_id: &str,
    ) -> Result<Vec<String>, TwitterError> {
        let tweet = self.tweet_result_by_rest_id(tweet_id)?;
        Ok(extract_media_urls(&tweet))
    }

    fn logged_in(&self) -> bool {
        self.cookies.contains_key("auth_token")
    }

    fn csrf_token(&self) -> &str {
        self.cookies.get("ct0").map(String::as_str).unwrap_or("")
    }

    fn tweet_result_by_rest_id(&mut self, tweet_id: &str) -> Result<Value, TwitterError> {
        let endpoint = "/graphql/qxWQxcMLiTPcavz9Qy5hwQ/TweetResultByRestId";
        let url = format!("{API_ROOT}{endpoint}");
        let params = tweet_result_params(tweet_id);
        let data = self.call_json(Method::GET, &url, Some(&params), true)?;
        tweet_from_result_response(data)
    }

    fn call_json(
        &mut self,
        method: Method,
        url: &str,
        params: Option<&[(&str, String)]>,
        auth: bool,
    ) -> Result<Value, TwitterError> {
        if auth && !self.logged_in() && self.guest_token.is_none() {
            self.authenticate_guest()?;
        }

        let mut request = self.client.request(method, url);
        request = self.apply_headers(request, auth)?;
        if let Some(params) = params {
            request = request.query(params);
        }

        let response = request.send()?;
        self.handle_response(response)
    }

    fn authenticate_guest(&mut self) -> Result<(), TwitterError> {
        let url = format!("{GUEST_API_ROOT}/1.1/guest/activate.json");
        let response = self
            .client
            .post(&url)
            .header(AUTHORIZATION, HeaderValue::from_static(BEARER_TOKEN))
            .header(USER_AGENT, HeaderValue::from_str(&self.user_agent)?)
            .header(ACCEPT, HeaderValue::from_static("*/*"))
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .send()?;
        let data = self.handle_response(response)?;
        let token = data
            .get("guest_token")
            .and_then(Value::as_str)
            .ok_or_else(|| TwitterError::UnexpectedResponse("missing guest_token".into()))?
            .to_string();
        self.cookies.insert("gt".to_string(), token.clone());
        self.guest_token = Some(token);
        Ok(())
    }

    fn apply_headers(
        &self,
        request: RequestBuilder,
        auth: bool,
    ) -> Result<RequestBuilder, TwitterError> {
        let headers = twitter_headers(
            &self.user_agent,
            self.csrf_token(),
            auth,
            self.logged_in(),
            self.guest_token.as_deref(),
            self.client_transaction_id.as_deref(),
            &self.cookies,
        )?;

        Ok(request
            .headers(headers)
            .header("Origin", WEB_ROOT)
            .header("X-Requested-With", "XMLHttpRequest"))
    }

    fn handle_response(&mut self, response: Response) -> Result<Value, TwitterError> {
        self.update_cookies(response.headers());

        let status = response.status();
        let text = response.text()?;
        let data: Value = serde_json::from_str(&text).unwrap_or_else(|_| {
            json!({
                "errors": [{ "message": text }]
            })
        });

        parse_response_json(status, data, self.logged_in())
    }

    fn update_cookies(&mut self, headers: &HeaderMap) {
        update_cookies_from_headers(&mut self.cookies, headers);
    }
}
