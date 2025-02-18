mod config;
mod handlers;
mod middlewares;
mod tasks;
mod utils;

use axum::{
    middleware::from_fn,
    routing::{get, post},
    Router,
};
use handlers::{create_events_handler, AppState, EventSchema};
use opensearch::{http::transport::Transport, OpenSearch};
use std::sync::Arc;
use tasks::{consume_events, schedule_insert_pending_events};
use tokio::sync::{mpsc, Mutex};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true),
        )
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .init();

    // OpenSearch client configuration
    let envs = config::get_environments();
    let transport = Transport::single_node(&envs.open_search_dns).unwrap();
    let open_search_client = OpenSearch::new(transport);

    // Database configuration
    let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(10)
        .connect(envs.database_url.as_str())
        .await
        .expect("Failed to create pool");

    // Channel configuration
    let (tx, rx) = mpsc::channel::<EventSchema>(100000);
    let rx = Arc::new(Mutex::new(rx));

    // Run consumer & scheduler asynchronously
    consume_events(&rx, &open_search_client, &db_pool).await;
    schedule_insert_pending_events(&tx, &db_pool).await;

    // Web server
    let state = AppState {
        tx: tx.clone(),
        conn: db_pool,
    };

    // Router configuration and middleware addition
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .route(
            "/events",
            post(create_events_handler).layer(from_fn(middlewares::jwt_auth_middleware)),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // Start the server
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", &envs.server_port))
        .await
        .unwrap();
    println!("Server is running at http://0.0.0.0:{}", &envs.server_port);
    axum::serve(listener, app).await.unwrap();
}
