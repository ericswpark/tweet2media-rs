use rand::{RngExt, distr::Alphanumeric};
use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, REFERER, SET_COOKIE,
    USER_AGENT,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use thiserror::Error;
use url::Url;

pub(super) const API_ROOT: &str = "https://x.com/i/api";
pub(super) const WEB_ROOT: &str = "https://x.com";
pub(super) const GUEST_API_ROOT: &str = "https://api.x.com";

// Same public web bearer token used by gallery-dl and the Twitter/X web client.
pub(super) const BEARER_TOKEN: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

pub(super) fn default_user_agent() -> String {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36".to_string()
}

/// Error type returned by all fallible `tweet2media` operations.
#[derive(Debug, Error)]
pub enum TwitterError {
    /// The input URL could not be parsed or did not contain a numeric `/status/<id>` segment.
    #[error("invalid tweet URL: {0}")]
    InvalidTweetUrl(String),

    /// Twitter/X indicated that a logged-in account is required to access the tweet.
    ///
    /// Add browser cookies such as `auth_token` and `ct0` through a client builder and retry.
    #[error("authentication is required: {0}")]
    AuthRequired(String),

    /// Twitter/X reported that the tweet cannot be viewed.
    #[error("tweet is unavailable: {0}")]
    TweetUnavailable(String),

    /// Twitter/X rate limited the current guest session, account, or network.
    #[error("Twitter/X rate limit exceeded")]
    RateLimited,

    /// An HTTP request failed before a usable Twitter/X response was received.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// A header value generated from caller-provided data was invalid.
    #[error("invalid HTTP header value: {0}")]
    InvalidHeader(#[from] reqwest::header::InvalidHeaderValue),

    /// A JSON response could not be parsed.
    #[error("JSON parse failed: {0}")]
    Json(#[from] serde_json::Error),

    /// Twitter/X returned a response shape or status code the library did not expect.
    #[error("unexpected Twitter/X API response: {0}")]
    UnexpectedResponse(String),
}

/// Extracts the numeric tweet ID from a Twitter/X status URL.
///
/// The parser accepts URLs from both `x.com` and `twitter.com`, as well as extra
/// path segments such as `/photo/1`, as long as the path contains a numeric
/// `/status/<id>` pair.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), tweet2media::TwitterError> {
/// let id = tweet2media::twitter::tweet_id_from_url(
///     "https://x.com/example/status/1234567890/photo/1",
/// )?;
/// assert_eq!(id, "1234567890");
/// # Ok(())
/// # }
/// ```
pub fn tweet_id_from_url(tweet_url: &str) -> Result<String, TwitterError> {
    let parsed =
        Url::parse(tweet_url).map_err(|_| TwitterError::InvalidTweetUrl(tweet_url.to_string()))?;

    let segments = parsed
        .path_segments()
        .ok_or_else(|| TwitterError::InvalidTweetUrl(tweet_url.to_string()))?
        .collect::<Vec<_>>();

    for window in segments.windows(2) {
        if window[0] == "status"
            && !window[1].is_empty()
            && window[1].bytes().all(|b| b.is_ascii_digit())
        {
            return Ok(window[1].to_string());
        }
    }

    Err(TwitterError::InvalidTweetUrl(tweet_url.to_string()))
}

pub(super) fn parse_cookie_header_into(
    cookies: &mut BTreeMap<String, String>,
    cookie_header: &str,
) {
    for part in cookie_header.split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() {
            cookies.insert(name.to_string(), value.trim().to_string());
        }
    }
}

pub(super) fn ensure_csrf_cookie(cookies: &mut BTreeMap<String, String>) {
    if !cookies.contains_key("ct0") {
        cookies.insert("ct0".to_string(), random_token(32));
    }
}

pub(super) fn cookie_header(cookies: &BTreeMap<String, String>) -> String {
    cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

pub(super) fn update_cookies_from_headers(
    cookies: &mut BTreeMap<String, String>,
    headers: &HeaderMap,
) {
    for value in headers.get_all(SET_COOKIE) {
        let Ok(value) = value.to_str() else {
            continue;
        };
        let Some(first) = value.split(';').next() else {
            continue;
        };
        let Some((name, cookie_value)) = first.split_once('=') else {
            continue;
        };
        if !name.trim().is_empty() {
            cookies.insert(name.trim().to_string(), cookie_value.trim().to_string());
        }
    }
}

pub(super) fn twitter_headers(
    user_agent: &str,
    csrf_token: &str,
    auth: bool,
    logged_in: bool,
    guest_token: Option<&str>,
    client_transaction_id: Option<&str>,
    cookies: &BTreeMap<String, String>,
) -> Result<HeaderMap, TwitterError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(REFERER, HeaderValue::from_static("https://x.com/"));
    headers.insert(USER_AGENT, HeaderValue::from_str(user_agent)?);
    headers.insert(AUTHORIZATION, HeaderValue::from_static(BEARER_TOKEN));
    headers.insert("x-csrf-token", HeaderValue::from_str(csrf_token)?);
    headers.insert("x-twitter-client-language", HeaderValue::from_static("en"));
    headers.insert("x-twitter-active-user", HeaderValue::from_static("yes"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));

    if auth {
        if logged_in {
            headers.insert(
                "x-twitter-auth-type",
                HeaderValue::from_static("OAuth2Session"),
            );
            if let Some(transaction_id) = client_transaction_id {
                headers.insert(
                    "x-client-transaction-id",
                    HeaderValue::from_str(transaction_id)?,
                );
            }
        } else if let Some(guest_token) = guest_token {
            headers.insert("x-guest-token", HeaderValue::from_str(guest_token)?);
        }
    }

    if !cookies.is_empty() {
        headers.insert(COOKIE, HeaderValue::from_str(&cookie_header(cookies))?);
    }

    Ok(headers)
}

pub(super) fn tweet_result_params(tweet_id: &str) -> Vec<(&'static str, String)> {
    let mut features = features_pagination();
    if let Value::Object(map) = &mut features {
        map.remove("rweb_video_screen_enabled");
    }

    vec![
        (
            "variables",
            json!({
                "tweetId": tweet_id,
                "withCommunity": false,
                "includePromotedContent": false,
                "withVoice": false
            })
            .to_string(),
        ),
        ("features", features.to_string()),
        (
            "fieldToggles",
            json!({
                "withArticleRichContentState": true,
                "withArticlePlainText": false,
                "withGrokAnalyze": false,
                "withDisallowedReplyControls": false
            })
            .to_string(),
        ),
    ]
}

pub(super) fn tweet_from_result_response(data: Value) -> Result<Value, TwitterError> {
    let mut tweet = data
        .pointer("/data/tweetResult/result")
        .cloned()
        .ok_or_else(|| {
            TwitterError::UnexpectedResponse("missing data.tweetResult.result".into())
        })?;

    if let Some(inner) = tweet.get("tweet") {
        tweet = inner.clone();
    }

    if tweet.get("__typename").and_then(Value::as_str) == Some("TweetUnavailable") {
        let reason = tweet
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("Unavailable");
        return match reason {
            "NsfwViewerHasNoStatedAge" | "NsfwLoggedOut" => {
                Err(TwitterError::AuthRequired("NSFW tweet".into()))
            }
            "Protected" => Err(TwitterError::AuthRequired("protected tweet".into())),
            _ => Err(TwitterError::TweetUnavailable(reason.to_string())),
        };
    }

    Ok(tweet)
}

pub(super) fn parse_response_json(
    status: reqwest::StatusCode,
    data: Value,
    logged_in: bool,
) -> Result<Value, TwitterError> {
    if status.as_u16() == 429 {
        return Err(TwitterError::RateLimited);
    }

    if let Some(errors) = data.get("errors").and_then(Value::as_array)
        && !errors.is_empty()
    {
        let messages = errors
            .iter()
            .filter_map(|error| error.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let message = if messages.is_empty() {
            "unspecified API error".to_string()
        } else {
            messages.join(", ")
        };

        if message.contains("Could not authenticate you") {
            return Err(TwitterError::TweetUnavailable(message));
        }
        if (status.as_u16() == 403 || status.as_u16() == 404) && !logged_in {
            return Err(TwitterError::AuthRequired(
                "authenticated cookies are required for this tweet".into(),
            ));
        }
        if !status.is_success() {
            return Err(TwitterError::UnexpectedResponse(format!(
                "{} {} ({})",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                message
            )));
        }
    }

    if !status.is_success() {
        return Err(TwitterError::UnexpectedResponse(format!(
            "{} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("")
        )));
    }

    Ok(data)
}

fn features_pagination() -> Value {
    json!({
        "payments_enabled": false,
        "rweb_video_screen_enabled": false,
        "rweb_xchat_enabled": false,
        "profile_label_improvements_pcf_label_in_post_enabled": true,
        "rweb_tipjar_consumption_enabled": true,
        "verified_phone_label_enabled": false,
        "creator_subscriptions_tweet_preview_api_enabled": true,
        "responsive_web_graphql_timeline_navigation_enabled": true,
        "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
        "premium_content_api_read_enabled": false,
        "communities_web_enable_tweet_community_results_fetch": true,
        "c9s_tweet_anatomy_moderator_badge_enabled": true,
        "responsive_web_grok_analyze_button_fetch_trends_enabled": false,
        "responsive_web_grok_analyze_post_followups_enabled": true,
        "responsive_web_jetfuel_frame": true,
        "responsive_web_grok_share_attachment_enabled": true,
        "articles_preview_enabled": true,
        "responsive_web_edit_tweet_api_enabled": true,
        "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
        "view_counts_everywhere_api_enabled": true,
        "longform_notetweets_consumption_enabled": true,
        "responsive_web_twitter_article_tweet_consumption_enabled": true,
        "tweet_awards_web_tipping_enabled": false,
        "responsive_web_grok_show_grok_translated_post": false,
        "responsive_web_grok_analysis_button_from_backend": true,
        "creator_subscriptions_quote_tweet_preview_enabled": false,
        "freedom_of_speech_not_reach_fetch_enabled": true,
        "standardized_nudges_misinfo": true,
        "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
        "longform_notetweets_rich_text_read_enabled": true,
        "longform_notetweets_inline_media_enabled": true,
        "responsive_web_grok_image_annotation_enabled": true,
        "responsive_web_grok_imagine_annotation_enabled": true,
        "responsive_web_grok_community_note_auto_translation_is_enabled": false,
        "responsive_web_enhance_cards_enabled": false
    })
}

fn random_token(len: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}
