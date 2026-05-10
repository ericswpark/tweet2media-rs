//! Extract original-size image URLs and highest-quality video URLs from Twitter/X tweets.
//!
//! `tweet2media` provides small blocking and async clients that call Twitter/X's
//! public web API, parse the tweet response, and return direct media URLs. It can
//! be used anonymously through a guest session for public tweets, or with browser
//! cookies from a logged-in account for protected, age-restricted, or otherwise
//! authenticated-only tweets.
//!
//! # Quick start
//!
//! For a public tweet, the blocking convenience function is usually enough:
//!
//! ```no_run
//! # fn main() -> Result<(), tweet2media::TwitterError> {
//! let urls = tweet2media::media_urls_from_tweet_url(
//!     "https://x.com/example/status/1234567890123456789",
//! )?;
//!
//! for url in urls {
//!     println!("{url}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Reuse a client when fetching more than one tweet so guest tokens and cookies
//! can be kept in memory between requests:
//!
//! ```no_run
//! # fn main() -> Result<(), tweet2media::TwitterError> {
//! let mut client = tweet2media::TwitterClient::builder().build()?;
//!
//! let first = client.media_urls_from_tweet_url_mut(
//!     "https://x.com/example/status/1234567890123456789",
//! )?;
//! let second = client.media_urls_from_tweet_id("9876543210987654321")?;
//! # let _ = (first, second);
//! # Ok(())
//! # }
//! ```
//!
//! # Authenticated tweets
//!
//! Some tweets require a logged-in Twitter/X session. Provide cookies copied from
//! your browser's `Cookie` request header, or add individual cookies with the
//! builder. The most important cookies are normally `auth_token` and `ct0`.
//!
//! ```no_run
//! # fn main() -> Result<(), tweet2media::TwitterError> {
//! let mut client = tweet2media::TwitterClient::builder()
//!     .cookie_header("auth_token=...; ct0=...")
//!     .build()?;
//!
//! let urls = client.media_urls_from_tweet_url_mut(
//!     "https://x.com/example/status/1234567890123456789",
//! )?;
//! # let _ = urls;
//! # Ok(())
//! # }
//! ```
//!
//! # Async usage
//!
//! The async API mirrors the blocking API and uses `reqwest::Client` internally:
//!
//! ```no_run
//! # async fn run() -> Result<(), tweet2media::TwitterError> {
//! let mut client = tweet2media::AsyncTwitterClient::builder().build()?;
//! let urls = client
//!     .media_urls_from_tweet_url_mut("https://x.com/example/status/1234567890123456789")
//!     .await?;
//! # let _ = urls;
//! # Ok(())
//! # }
//! ```
//!
//! # Returned media URLs
//!
//! Image URLs are rewritten to Twitter/X's original-size variant when possible.
//! Video and GIF media return the highest-bitrate MP4 variant found in the tweet
//! response. Media order follows the order in Twitter/X's media arrays, with
//! duplicates removed.
//!
//! # Errors
//!
//! Most fallible APIs return [`TwitterError`]. Notable cases include invalid tweet
//! URLs, rate limiting, unavailable tweets, and tweets that require authentication.

pub mod twitter;

pub use twitter::{
    async_media_urls_from_tweet_url, media_urls_from_tweet_url, AsyncTwitterClient,
    AsyncTwitterClientBuilder, TwitterClient, TwitterClientBuilder, TwitterError,
};
