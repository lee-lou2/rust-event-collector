mod handlers;
mod middlewares;
mod tasks;
mod utils;

use axum::{
    middleware::from_fn,
    routing::{get, post},
    Router,
};
use dotenv::dotenv;
use handlers::{create_events_handler, ping_handler, AppState, EventSchema};
use opensearch::{http::transport::Transport, OpenSearch};
use rusqlite::Connection;
use std::sync::Arc;
use tasks::{consume_events, schedule_insert_pending_events};
use tokio::sync::{mpsc, Mutex};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // 환경 변수 조회
    dotenv().ok();

    // 로그 초기화
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    // OpenSearch 클라이언트 설정
    let open_search_dns =
        std::env::var("OPEN_SEARCH_DNS").unwrap_or_else(|_| "http://localhost:9200".to_string());
    let transport = Transport::single_node(&open_search_dns).unwrap();
    let open_search_client = OpenSearch::new(transport);

    // 데이터베이스 설정
    let conn = Connection::open("db.sqlite3").unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY AUTOINCREMENT, log TEXT)",
        [],
    )
    .unwrap();
    let conn = Arc::new(Mutex::new(conn));

    // 채널 설정
    let (tx, rx) = mpsc::channel::<EventSchema>(100000);
    let rx = Arc::new(Mutex::new(rx));

    // 컨슈머 & 스케쥴러 비동기 실행
    consume_events(&rx, &open_search_client, &conn).await;
    schedule_insert_pending_events(&tx, &conn).await;

    // 웹서버
    let state = AppState {
        tx: tx.clone(),
        conn: conn.clone(),
    };

    // 라우터 설정 및 미들웨어 추가
    let app = Router::new()
        .route("/ping", get(ping_handler))
        .route("/events", post(create_events_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(from_fn(middlewares::auth_middleware));

    // 서버 실행
    let server_host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", server_host, server_port))
        .await
        .unwrap();
    println!(
        "Server is running at http://{}:{}",
        server_host, server_port
    );
    axum::serve(listener, app).await.unwrap();
}
