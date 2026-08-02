use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use vault_server::api;
use vault_server::config;
use vault_server::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cfg = config::load();
    let state = AppState::new(cfg.clone());

    // Periodic session GC: drops expired sessions + their rclone config dirs.
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                state.sessions.gc(&state.cfg);
            }
        });
    }

    let frontend_dir = cfg.frontend_dir.clone();
    let api_router = api::router();
    let app = api_router
        .fallback_service(ServeDir::new(&frontend_dir))
        .layer(RequestBodyLimitLayer::new(cfg.max_upload_bytes as usize))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.listen_addr)
        .await
        .expect("failed to bind listener");
    tracing::info!("Vault server listening on {}", cfg.listen_addr);
    axum::serve(listener, app).await.expect("server error");
}
