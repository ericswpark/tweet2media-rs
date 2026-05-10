//! Twitter/X media extraction clients and helpers.
//!
//! This module contains both blocking and async clients. The crate root re-exports
//! the main types and functions, so most users can import from `tweet2media`
//! directly instead of this module.

mod async_client;
mod blocking;
mod media;
mod shared;

pub use async_client::{
    async_media_urls_from_tweet_url, AsyncTwitterClient, AsyncTwitterClientBuilder,
};
pub use blocking::{media_urls_from_tweet_url, TwitterClient, TwitterClientBuilder};
pub use shared::{tweet_id_from_url, TwitterError};

#[cfg(test)]
mod tests {
    use super::tweet_id_from_url;

    #[test]
    fn parses_status_url() {
        assert_eq!(
            tweet_id_from_url("https://x.com/user/status/1234567890/photo/1").unwrap(),
            "1234567890"
        );
        assert_eq!(
            tweet_id_from_url("https://twitter.com/i/web/status/42").unwrap(),
            "42"
        );
    }
}
