//! Unit tests for HikerAPI error mapping and payload mappers (mock server).

use insta_dl_gui_lib::hiker::{map_post, map_profile, HikerClient, BASE_URL};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_at(uri: &str) -> HikerClient {
    HikerClient::with_base_url("tok".into(), uri.to_string())
}

#[tokio::test]
async fn clips_chunk_maps_posts_and_cursor() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .and(query_param("user_id", "42"))
        .and(query_param("end_cursor", "next"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            [{
                "pk": "r1",
                "code": "REEL1",
                "media_type": 2,
                "taken_at": 1_776_000_000,
                "resources": [],
                "video_versions": [{
                    "url": "https://cdninstagram.com/r1.mp4",
                    "width": 720
                }],
                "image_versions2": {
                    "candidates": [{
                        "url": "https://cdninstagram.com/r1.jpg",
                        "width": 720
                    }]
                }
            }],
            "after"
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let page = client_at(&server.uri())
        .user_clips_chunk("42", Some("next"))
        .await
        .unwrap();

    assert_eq!(
        page.posts
            .iter()
            .map(|post| post.pk.as_str())
            .collect::<Vec<_>>(),
        ["r1"]
    );
    assert_eq!(page.posts[0].resources.len(), 1);
    assert_eq!(
        page.posts[0].resources[0].kind,
        insta_dl_gui_lib::models::MediaKind::Video
    );
    assert_eq!(page.end_cursor.as_deref(), Some("after"));
}

#[tokio::test]
async fn clips_chunk_rejects_non_array_payload() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/user/clips/chunk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "items": [] })))
        .mount(&server)
        .await;

    let error = client_at(&server.uri())
        .user_clips_chunk("42", None)
        .await
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("clips/chunk: expected [items, cursor]"));
}

#[tokio::test]
async fn status_codes_map_to_typed_errors() {
    let server = MockServer::start().await;
    for (status, expected) in [
        (401u16, "AuthInvalid"),
        (402, "QuotaExhausted"),
        (403, "Banned"),
        (404, "NotFound"),
        (500, "Transient"),
    ] {
        let p = format!("/e{status}");
        Mock::given(method("GET"))
            .and(path(&p))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let err = client_at(&server.uri()).get(&p, &[]).await.unwrap_err();
        assert_eq!(err.code(), expected, "status {status} → {err}");
    }
}

#[tokio::test]
async fn rate_limit_carries_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sys/balance"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "7"))
        .mount(&server)
        .await;
    let err = client_at(&server.uri()).balance().await.unwrap_err();
    assert!(err.to_string().contains("7"), "{err}");
}

#[tokio::test]
async fn balance_payload_parses() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sys/balance"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "requests": 12345,
            "rate": 10,
            "amount": 4.2,
            "currency": "usd"
        })))
        .mount(&server)
        .await;
    let balance = client_at(&server.uri()).balance().await.unwrap();
    assert_eq!(balance.requests, 12345);
    assert_eq!(balance.rate, Some(10));
    assert_eq!(balance.amount, Some(4.2));
    assert_eq!(balance.currency.as_deref(), Some("usd"));
}

#[tokio::test]
async fn auth_header_is_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sys/balance"))
        .and(wiremock::matchers::header("x-access-key", "tok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "requests": 1 })),
        )
        .expect(1)
        .mount(&server)
        .await;
    client_at(&server.uri()).balance().await.unwrap();
}

#[test]
fn default_base_url_is_production() {
    assert_eq!(BASE_URL, "https://api.hikerapi.com");
}

#[test]
fn customer_facing_hiker_links_use_the_referral_url() {
    for error in [
        insta_dl_gui_lib::hiker::HikerError::AuthInvalid,
        insta_dl_gui_lib::hiker::HikerError::QuotaExhausted,
    ] {
        let message = error.to_string();
        assert!(message.contains("https://hikerapi.com/p/uk064a1b"));
        assert!(!message.contains("hikerapi.com/tokens"));
    }
}

#[test]
fn map_post_handles_modern_video_shape() {
    let media = serde_json::json!({
        "pk": "3880296624023575664",
        "code": "DXZlTiKEpxw",
        "media_type": 2,
        "taken_at": 1776787455,
        "caption_text": "hello",
        "like_count": 42,
        "comment_count": 3,
        "user": { "pk": "25025320", "username": "instagram" },
        "video_versions": [
            { "url": "https://cdn.fbsbx.com/low.mp4", "width": 360 },
            { "url": "https://cdn.fbsbx.com/high.mp4", "width": 720 }
        ],
        "image_versions2": { "candidates": [ { "url": "https://cdninstagram.com/thumb.jpg", "width": 750 } ] }
    });
    let post = map_post(&media).unwrap();
    assert_eq!(post.code, "DXZlTiKEpxw");
    assert_eq!(post.taken_at, Some(1776787455));
    assert_eq!(post.resources.len(), 1);
    // highest-width video wins
    assert_eq!(post.resources[0].url, "https://cdn.fbsbx.com/high.mp4");
    assert_eq!(
        post.thumbnail_url.as_deref(),
        Some("https://cdninstagram.com/thumb.jpg")
    );
    assert_eq!(post.owner_username.as_deref(), Some("instagram"));
}

#[test]
fn map_post_handles_legacy_flat_shape_and_carousel() {
    let media = serde_json::json!({
        "pk": 12345678901234567_u64,
        "code": "AbCdEfGh",
        "media_type": 8,
        "taken_at": "2026-04-21T16:04:15Z",
        "carousel_media": [
            { "media_type": 1, "thumbnail_url": "https://cdninstagram.com/a.jpg" },
            { "media_type": 2, "video_url": "https://cdninstagram.com/b.mp4" }
        ]
    });
    let post = map_post(&media).unwrap();
    assert_eq!(post.pk, "12345678901234567");
    assert_eq!(post.taken_at, Some(1776787455));
    assert_eq!(post.resources.len(), 2);
    assert_eq!(post.resources[0].url, "https://cdninstagram.com/a.jpg");
    assert_eq!(post.resources[1].url, "https://cdninstagram.com/b.mp4");
}

#[test]
fn map_profile_extracts_hd_avatar() {
    let user = serde_json::json!({
        "pk": 25025320,
        "username": "Instagram",
        "full_name": "Instagram",
        "media_count": 7000,
        "follower_count": 700000000_u64,
        "is_private": false,
        "is_verified": true,
        "profile_pic_url": "https://cdninstagram.com/lo.jpg",
        "profile_pic_url_hd": "https://cdninstagram.com/hi.jpg"
    });
    let p = map_profile(&user).unwrap();
    assert_eq!(p.username, "Instagram");
    assert_eq!(
        p.avatar_url.as_deref(),
        Some("https://cdninstagram.com/hi.jpg")
    );
    assert!(p.is_verified);
}
