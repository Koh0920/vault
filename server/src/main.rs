use vault_server::api;
use vault_server::config;
use vault_server::AppState;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

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

    let frontend_dir = if cfg.frontend_dir.exists() {
        cfg.frontend_dir.clone()
    } else {
        // fall back to bundled dist if configured path missing
        cfg.frontend_dir.clone()
    };

    let api_router = api::router();
    let app = api_router
        .fallback_service(ServeDir::new(&frontend_dir))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.listen_addr)
        .await
        .expect("failed to bind listener");
    tracing::info!("Vault server listening on {}", cfg.listen_addr);
    axum::serve(listener, app).await.expect("server error");
}