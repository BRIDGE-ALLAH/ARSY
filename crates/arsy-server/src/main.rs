use std::sync::Arc;

use anyhow::Context;
use arsy_core::services::UserService;
use arsy_infra::db::InMemoryUserRepository;
use arsy_server::{config::AppConfig, routes, state::AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("arsy_server=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cfg = AppConfig::from_env();
    // Swap InMemoryUserRepository for a real adapter once a database is wired in.
    let state = AppState::new(Arc::new(UserService::new(Arc::new(
        InMemoryUserRepository::default(),
    ))));

    let listener = tokio::net::TcpListener::bind((cfg.addr.as_str(), cfg.port))
        .await
        .with_context(|| format!("failed to bind {}:{}", cfg.addr, cfg.port))?;
    tracing::info!(addr = %cfg.addr, port = cfg.port, "arsy-server listening");

    axum::serve(listener, routes::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server exited with error")?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(windows)]
    let terminate = async { std::future::pending::<()>().await };
    #[cfg(not(windows))]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => sig.recv().await,
            Err(_) => None,
        };
    };

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received, stopping gracefully");
}
