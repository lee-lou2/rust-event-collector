use axum::{extract::State, Json};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

// 앱 상태
#[derive(Clone)]
pub struct AppState {
    pub tx: mpsc::Sender<EventSchema>,
    pub conn: Arc<Mutex<Connection>>,
}

// 이벤트 스키마
#[derive(Deserialize, Serialize, Debug)]
pub struct EventSchema {
    pub log_id: String,
    pub page: String,
    pub event: String,
    pub label: Option<String>,
    pub target: Option<String>,
    pub section: Option<String>,
    pub param: Option<serde_json::Value>,
}

// 서버 상태 점검
pub async fn ping_handler() -> &'static str {
    "pong"
}

// 이벤트 저장
pub async fn create_events_handler(
    State(state): State<AppState>,
    Json(payload): Json<EventSchema>,
) -> &'static str {
    // 이벤트를 채널로 전달 시도 후 실패 시 데이터베이스에 저장
    match state.tx.try_send(payload) {
        Ok(_) => "created",
        Err(_e) => {
            let payload = _e.into_inner();
            crate::utils::insert_pending_event(&payload, state.conn).await;
            "pending"
        }
    }
}
