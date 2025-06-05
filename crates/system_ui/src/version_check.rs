use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct GitData {
    tag_name: String,
    html_url: String,
    body: String,
}

pub fn build_date() -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(build_time::build_time_utc!("%Y-%m-%d"), "%Y-%m-%d").unwrap()
}

pub fn check_update_sync() -> Option<(String, String)> {
    #[cfg(not(target_arch = "wasm32"))]
    return futures_lite::future::block_on(check_update());

    // can't block in wasm
    #[cfg(target_arch = "wasm32")]
    None
}

pub async fn check_update() -> Option<(String, String)> {
    let latest: GitData = platform::compat(async {
        reqwest::get("https://api.github.com/repos/decentraland/bevy-explorer/releases/latest")
            .await
            .ok()?
            .json()
            .await
            .ok()
    })
    .await?;
    let latest_date = latest
        .tag_name
        .split('-')
        .skip(1)
        .take(3)
        .collect::<Vec<_>>()
        .join("-");
    let latest_date = chrono::NaiveDate::parse_from_str(&latest_date, "%Y-%m-%d").ok()?;

    if latest_date > build_date() {
        Some((latest.body, latest.html_url))
    } else {
        None
    }
}
