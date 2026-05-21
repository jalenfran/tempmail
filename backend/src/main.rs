use anyhow::Result;
use tempmail::{MIGRATOR, api, config, smtp, state};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tempmail=info,tower_http=info,axum=info".into()),
        )
        .init();

    let config_path =
        std::env::var("TEMPMAIL_CONFIG").unwrap_or_else(|_| "./config.toml".to_string());
    let cfg = config::Config::load(&config_path)?;

    let state = state::AppState::new(cfg.clone()).await?;
    MIGRATOR.run(&state.db).await?;

    // Spawn SMTP receiver if configured.
    if let Some(smtp_cfg) = cfg.smtp.clone() {
        let smtp_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = smtp::run(smtp_cfg, smtp_state).await {
                tracing::error!(error = ?e, "smtp server exited");
            }
        });
    }

    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind(&cfg.server.listen).await?;
    tracing::info!(addr = %cfg.server.listen, domain = %cfg.mail.domain, "tempmail listening");
    axum::serve(listener, app).await?;
    Ok(())
}
