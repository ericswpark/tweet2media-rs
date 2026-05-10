use serde_json::Value;
use std::collections::HashSet;

pub(super) fn extract_media_urls(tweet: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let media_arrays = [
        tweet.pointer("/legacy/extended_entities/media"),
        tweet.pointer("/legacy/entities/media"),
        tweet.pointer("/extended_entities/media"),
        tweet.pointer("/entities/media"),
    ];

    for media_array in media_arrays.into_iter().flatten() {
        let Some(items) = media_array.as_array() else {
            continue;
        };

        for media in items {
            if media_unavailable(media) {
                continue;
            }

            if let Some(url) = best_video_url(media).or_else(|| image_url(media))
                && seen.insert(url.clone())
            {
                out.push(url);
            }
        }
    }

    out
}

fn media_unavailable(media: &Value) -> bool {
    media
        .pointer("/ext_media_availability/status")
        .and_then(Value::as_str)
        == Some("Unavailable")
}

fn best_video_url(media: &Value) -> Option<String> {
    let variants = media.pointer("/video_info/variants")?.as_array()?;

    variants
        .iter()
        .filter(|variant| {
            let url = variant
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let content_type = variant
                .get("content_type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            content_type == "video/mp4" || url.contains(".mp4")
        })
        .max_by_key(|variant| variant.get("bitrate").and_then(Value::as_u64).unwrap_or(0))
        .and_then(|variant| variant.get("url").and_then(Value::as_str))
        .or_else(|| {
            variants
                .iter()
                .max_by_key(|variant| variant.get("bitrate").and_then(Value::as_u64).unwrap_or(0))
                .and_then(|variant| variant.get("url").and_then(Value::as_str))
        })
        .map(str::to_string)
}

fn image_url(media: &Value) -> Option<String> {
    let url = media
        .get("media_url_https")
        .or_else(|| media.get("media_url"))
        .and_then(Value::as_str)?;
    Some(original_image_url(url))
}

fn original_image_url(url: &str) -> String {
    if url.contains("?format=") {
        if let Some((prefix, _)) = url.split_once("name=") {
            return format!("{prefix}name=orig");
        }
        return format!("{url}&name=orig");
    }

    let clean = url.split('?').next().unwrap_or(url);
    if let Some((base, ext)) = clean.rsplit_once('.') {
        return format!("{base}?format={ext}&name=orig");
    }

    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rewrites_image_to_original() {
        assert_eq!(
            original_image_url("https://pbs.twimg.com/media/abc.jpg"),
            "https://pbs.twimg.com/media/abc?format=jpg&name=orig"
        );
        assert_eq!(
            original_image_url("https://pbs.twimg.com/media/abc?format=png&name=small"),
            "https://pbs.twimg.com/media/abc?format=png&name=orig"
        );
    }

    #[test]
    fn extracts_images_and_best_video() {
        let tweet = json!({
            "legacy": {
                "extended_entities": {
                    "media": [
                        {"media_url_https": "https://pbs.twimg.com/media/a.jpg"},
                        {
                            "video_info": {
                                "variants": [
                                    {"content_type": "application/x-mpegURL", "url": "https://video.twimg.com/a.m3u8"},
                                    {"content_type": "video/mp4", "bitrate": 256000, "url": "https://video.twimg.com/a-low.mp4"},
                                    {"content_type": "video/mp4", "bitrate": 832000, "url": "https://video.twimg.com/a-high.mp4"}
                                ]
                            },
                            "media_url_https": "https://pbs.twimg.com/media/video-thumb.jpg"
                        }
                    ]
                }
            }
        });

        assert_eq!(
            extract_media_urls(&tweet),
            vec![
                "https://pbs.twimg.com/media/a?format=jpg&name=orig".to_string(),
                "https://video.twimg.com/a-high.mp4".to_string()
            ]
        );
    }
}
