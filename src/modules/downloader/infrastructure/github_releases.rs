use serde::Deserialize;
use std::time::Duration;

use crate::debug_log;
use crate::modules::downloader::domain::entities::ReleaseInfo;
use crate::modules::downloader::domain::errors::DownloaderError;
use crate::modules::downloader::domain::ports::ReleasePort;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/pausegarra/pullyt/releases/latest";
const USER_AGENT: &str = "pullyt update-check";
const RELEASE_TIMEOUT_SECS: u64 = 5;

pub struct GitHubReleaseAdapter;

impl ReleasePort for GitHubReleaseAdapter {
    fn fetch_latest_release(&self) -> Result<ReleaseInfo, DownloaderError> {
        debug_log!("[updates] github: fetch_latest_release start");
        let agent = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(RELEASE_TIMEOUT_SECS))
            .timeout_read(Duration::from_secs(RELEASE_TIMEOUT_SECS))
            .timeout_write(Duration::from_secs(RELEASE_TIMEOUT_SECS))
            .build();

        debug_log!("[updates] github: requesting {LATEST_RELEASE_URL}");
        let response: LatestReleaseResponse = agent
            .get(LATEST_RELEASE_URL)
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| DownloaderError::ReleaseCheckFailed(e.to_string()))?
            .into_json()
            .map_err(|e| DownloaderError::ReleaseCheckFailed(e.to_string()))?;

        let version = response
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&response.tag_name)
            .to_string();

        debug_log!(
            "[updates] github: received release tag={} url={}",
            response.tag_name, response.html_url
        );

        Ok(ReleaseInfo {
            version,
            url: response.html_url,
        })
    }
}

#[derive(Deserialize)]
struct LatestReleaseResponse {
    tag_name: String,
    html_url: String,
}
