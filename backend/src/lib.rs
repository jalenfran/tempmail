pub mod api;
pub mod config;
pub mod error;
pub mod events;
pub mod models;
pub mod outbound;
pub mod pipeline;
pub mod smtp;
pub mod state;
pub mod storage;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
