use anyhow::Result;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub mail: MailConfig,
    pub resend: ResendConfig,
    #[serde(default)]
    pub smtp: Option<SmtpConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    /// e.g. "0.0.0.0:25"
    pub listen: String,
    /// Greeting hostname, appears in the 220 banner and EHLO response.
    #[serde(default = "default_smtp_hostname")]
    pub hostname: String,
    /// Optional PEM-encoded TLS certificate chain. If both this and `tls_key`
    /// are set, the server advertises STARTTLS and accepts TLS upgrades.
    #[serde(default)]
    pub tls_cert: Option<String>,
    /// Optional PEM-encoded private key matching `tls_cert`.
    #[serde(default)]
    pub tls_key: Option<String>,
}

fn default_smtp_hostname() -> String {
    "tempmail".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen: String,
    pub data_dir: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub public_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub api_token: String,
    pub ingest_hmac_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MailConfig {
    pub domain: String,
    #[serde(default = "default_max_msg")]
    pub max_message_bytes: usize,
    #[serde(default = "default_true")]
    pub catch_all: bool,
}

fn default_max_msg() -> usize {
    26_214_400
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResendConfig {
    pub api_key: String,
    #[serde(default)]
    pub default_from_name: Option<String>,
    #[serde(default = "default_resend_base_url")]
    pub base_url: String,
}

fn default_resend_base_url() -> String {
    "https://api.resend.com".to_string()
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let cfg: Config = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("TEMPMAIL_").split("__"))
            .extract()?;
        Ok(cfg)
    }
}
