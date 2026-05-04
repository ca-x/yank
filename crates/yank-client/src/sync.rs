use anyhow::{Context, Result};
use reqwest::blocking::Client;
use std::time::Duration;
use yank_core::{Clip, PullClipsResponse, PushClipRequest, PushClipResponse};

#[derive(Clone, Debug)]
pub struct SyncConfig {
    pub server_url: String,
    pub token: String,
}

impl SyncConfig {
    pub fn new(server_url: String, token: String) -> Option<Self> {
        let server_url = server_url.trim().trim_end_matches('/').to_owned();
        let token = token.trim().to_owned();
        if server_url.is_empty() || token.is_empty() {
            None
        } else {
            Some(Self { server_url, token })
        }
    }
}

pub struct SyncClient {
    http: Client,
    config: SyncConfig,
}

impl SyncClient {
    pub fn new(config: SyncConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .expect("reqwest client should build"),
            config,
        }
    }

    pub fn push_clip(&self, clip: Clip) -> Result<Clip> {
        let response = self
            .http
            .post(format!("{}/api/sync/push", self.config.server_url))
            .bearer_auth(&self.config.token)
            .json(&PushClipRequest { clip })
            .send()
            .context("pushing clip to yank server")?
            .error_for_status()
            .context("server rejected clip push")?
            .json::<PushClipResponse>()
            .context("decoding clip push response")?;
        Ok(response.clip)
    }

    pub fn pull_since(&self, since: i64) -> Result<PullClipsResponse> {
        self.http
            .get(format!("{}/api/sync/pull", self.config.server_url))
            .bearer_auth(&self.config.token)
            .query(&[("since", since)])
            .send()
            .context("pulling clips from yank server")?
            .error_for_status()
            .context("server rejected clip pull")?
            .json::<PullClipsResponse>()
            .context("decoding clip pull response")
    }

    pub fn delete_clip(&self, id: &str) -> Result<()> {
        self.http
            .delete(format!("{}/api/clips/{}", self.config.server_url, id))
            .bearer_auth(&self.config.token)
            .send()
            .context("deleting clip on yank server")?
            .error_for_status()
            .context("server rejected clip delete")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incomplete_sync_config() {
        assert!(SyncConfig::new(String::new(), "token".to_owned()).is_none());
        assert!(SyncConfig::new("http://localhost".to_owned(), String::new()).is_none());
    }

    #[test]
    fn trims_sync_config() {
        let config =
            SyncConfig::new("http://localhost:7219/".to_owned(), " token ".to_owned()).unwrap();
        assert_eq!(config.server_url, "http://localhost:7219");
        assert_eq!(config.token, "token");
    }
}
