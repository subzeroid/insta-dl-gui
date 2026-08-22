//! Live smoke test — makes real HikerAPI calls, costs quota. Skipped unless
//! SMOKE_TOKEN is set (token is NOT stored in the repo).
//!
//! ```sh
//! SMOKE_TOKEN=... cargo test --test live_download -- --nocapture
//! ```

use insta_dl_gui_lib::cdn;
use insta_dl_gui_lib::hiker::{map_post, map_profile, HikerClient};

const TEST_POST_CODE: &str = "DXZlTiKEpxw"; // instagram's own public post
const TEST_PROFILE: &str = "instagram";

#[tokio::test]
async fn balance_and_single_post_download() {
    let Some(token) = std::env::var("SMOKE_TOKEN").ok().filter(|t| !t.is_empty()) else {
        eprintln!("SMOKE_TOKEN not set — skipping live smoke");
        return;
    };
    let client = HikerClient::new(token);

    let balance = client.balance().await.expect("balance");
    println!("balance: {} requests", balance.requests);
    assert!(balance.requests > 0, "no quota left for smoke test");

    let media = client
        .media_by_code(TEST_POST_CODE)
        .await
        .expect("media_by_code");
    let post = map_post(&media).expect("map_post");
    println!(
        "post {}: {} resource(s), owner @{:?}",
        post.code,
        post.resources.len(),
        post.owner_username
    );
    assert!(!post.code.is_empty());
    assert!(
        !post.resources.is_empty(),
        "post has no downloadable resources"
    );

    let dest_dir = std::env::temp_dir().join(format!("insta-dl-gui-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dest_dir).unwrap();

    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let mut last_error = None;
    for resource in &post.resources {
        let base = dest_dir.join(format!("{}_smoke", post.code));
        match cdn::stream_to_file(&http, &resource.url, &base, post.taken_at, |_| {}, None).await {
            Ok(outcome) => {
                println!("saved {} ({} bytes)", outcome.path.display(), outcome.bytes);
                assert!(outcome.path.exists());
                assert!(outcome.bytes > 0);
                if post.taken_at.is_some() {
                    let meta = std::fs::metadata(&outcome.path).unwrap();
                    let mtime = meta.modified().unwrap().elapsed().unwrap();
                    // taken_at of an old post must not be "just now"
                    assert!(mtime.as_secs() > 60, "mtime not preserved");
                }
                last_error = None;
                break;
            }
            Err(e) => {
                println!("resource failed ({e}), trying next…");
                last_error = Some(e);
            }
        }
    }
    assert!(last_error.is_none(), "all resources failed");

    std::fs::remove_dir_all(&dest_dir).ok();
}

#[tokio::test]
async fn profile_fetch_and_first_page() {
    let Some(token) = std::env::var("SMOKE_TOKEN").ok().filter(|t| !t.is_empty()) else {
        eprintln!("SMOKE_TOKEN not set — skipping live smoke");
        return;
    };
    let client = HikerClient::new(token);

    let user = client
        .user_by_username(TEST_PROFILE)
        .await
        .expect("user_by_username");
    let profile = map_profile(&user).expect("map_profile");
    println!(
        "@{} pk={} media_count={} private={}",
        profile.username, profile.pk, profile.media_count, profile.is_private
    );
    assert!(!profile.pk.is_empty());
    assert!(!profile.is_private);
    assert!(profile.avatar_url.is_some());

    let page = client
        .user_medias_chunk(&profile.pk, None)
        .await
        .expect("medias_chunk");
    println!(
        "first page: {} posts, cursor={:?}",
        page.posts.len(),
        page.end_cursor.is_some()
    );
    assert!(!page.posts.is_empty(), "expected posts on first page");

    // Stories shape (2 requests) — @instagram usually has none active.
    let stories = client.user_stories(&profile.pk).await.expect("stories");
    println!("stories: {} items", stories.len());

    // Avatar download through the CDN streamer.
    if let Some(url) = &profile.avatar_url {
        let dest =
            std::env::temp_dir().join(format!("insta-dl-gui-smoke-avatar-{}", std::process::id()));
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let out = cdn::stream_to_file(&http, url, &dest, None, |_| {}, None)
            .await
            .expect("avatar download");
        println!("avatar saved: {} ({} bytes)", out.path.display(), out.bytes);
        assert!(out.bytes > 0);
        std::fs::remove_file(&out.path).ok();
    }
}

#[tokio::test]
async fn search_autocomplete() {
    let Some(token) = std::env::var("SMOKE_TOKEN").ok().filter(|t| !t.is_empty()) else {
        eprintln!("SMOKE_TOKEN not set — skipping");
        return;
    };
    let client = HikerClient::new(token);
    let users = client.search_accounts("nike").await.expect("search_accounts");
    assert!(!users.is_empty());
    let mapped: Vec<_> = users.iter().filter_map(insta_dl_gui_lib::hiker::map_search_user).collect();
    assert!(mapped.iter().any(|u| u.username == "nike"), "exact match must be present");
    println!("search 'nike': {} raw → {} mapped, first: {:?}", users.len(), mapped.len(), mapped.first().map(|u| &u.username));
}
