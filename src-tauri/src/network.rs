use std::sync::Arc;

use crate::config::Config;
use crate::hiker::HikerClient;

pub struct NetworkClients {
    pub hiker: Option<Arc<HikerClient>>,
    pub cdn: reqwest::Client,
}

impl NetworkClients {
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let proxy = config.proxy_url.as_deref();
        let hiker = config
            .token
            .as_ref()
            .map(|token| HikerClient::with_proxy(token.clone(), proxy).map(Arc::new))
            .transpose()?;

        Ok(Self {
            hiker,
            cdn: build_cdn_client(proxy)?,
        })
    }
}

pub fn build_cdn_client(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    crate::proxy::apply_proxy(
        reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()),
        proxy_url,
    )?
    .build()
    .map_err(|_| "Could not configure the CDN client".to_owned())
}
