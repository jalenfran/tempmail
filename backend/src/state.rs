use crate::config::Config;
use crate::events::EventBus;
use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<Config>,
    pub events: EventBus,
    pub data_dir: PathBuf,
}

impl AppState {
    pub async fn new(cfg: Config) -> Result<Self> {
        let data_dir = PathBuf::from(&cfg.server.data_dir);
        std::fs::create_dir_all(&data_dir)?;
        std::fs::create_dir_all(data_dir.join("eml"))?;
        std::fs::create_dir_all(data_dir.join("attachments"))?;

        let db_path = data_dir.join("tempmail.db");
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let db = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(opts)
            .await?;

        Ok(Self {
            db,
            config: Arc::new(cfg),
            events: EventBus::new(256),
            data_dir,
        })
    }
}
