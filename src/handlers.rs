use crate::utils::insert_pending_events;
use axum::{
    extract::{Extension, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

/// App state
#[derive(Clone)]
pub struct AppState {
    pub tx: mpsc::Sender<EventSchema>,
    pub conn: SqlitePool,
}

/// Event info schema
/// The data structure provided by the user in the body
#[derive(Deserialize, Serialize, Clone)]
pub struct EventInfoSchema {
    pub page: String,
    pub event: String,
    pub label: Option<String>,
    pub target: Option<String>,
    pub section: Option<String>,
    pub param: Option<serde_json::Value>,
}

/// Event schema
/// Event schema created with information included in the header and the input data
#[derive(Clone, Deserialize, Serialize)]
pub struct EventSchema {
    pub user_id: Option<String>,
    pub device_uuid: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub user_agent: Option<String>,
    pub event_info: EventInfoSchema,
}

/// Event creation handler
/// Pass the input event to the channel
pub async fn create_events_handler(
    State(state): State<AppState>,
    header: axum::http::HeaderMap,
    Extension(claims): Extension<crate::middlewares::Claims>,
    Json(payload): Json<EventInfoSchema>,
) -> &'static str {
    // Store user information
    let user_id = claims.sub;

    // Device and app information
    let get_header = |key| header.get(key).and_then(|v| v.to_str().ok());
    let device_uuid = get_header("device-uuid");
    let app_version = get_header("app-version");
    let os_version = get_header("os-version");
    let user_agent = get_header("user-agent");
    let data = EventSchema {
        user_id: Some(user_id),
        device_uuid: device_uuid.map(|v| v.to_string()),
        app_version: app_version.map(|v| v.to_string()),
        os_version: os_version.map(|v| v.to_string()),
        user_agent: user_agent.map(|v| v.to_string()),
        event_info: payload,
    };

    match state.tx.try_send(data) {
        Ok(_) => "created",
        Err(e) => {
            eprintln!("Error sending data to channel: {:?}", e);
            let payload = e.into_inner();
            match insert_pending_events(&[payload], &state.conn).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error inserting pending events: {:?}", e);
                }
            }
            "pending"
        }
    }
}
