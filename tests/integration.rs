use tweet2media::media_urls_from_tweet_url;

#[test]
fn fetch_test_tweet_media() {
    let urls = media_urls_from_tweet_url("https://x.com/FrameworkPuter/status/2110759613046083913")
        .expect("failed to fetch tweet media");
    println!("URLs: {:?}", urls);
    assert_eq!(urls.len(), 1);
}
