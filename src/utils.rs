use crate::handlers::EventSchema;
use opensearch::{BulkParts, OpenSearch};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

// 펜딩된 이벤트 등록
pub async fn insert_pending_event(event: &EventSchema, conn: Arc<Mutex<Connection>>) {
    // 데이터베이스에 저장
    println!("Event inserted into SQLite successfully.");
    let locked_conn = conn.lock().await;
    let log = serde_json::to_string(&event).unwrap();
    locked_conn
        .execute("INSERT INTO events (log) VALUES (?1)", [&log])
        .unwrap();
}

// 이벤트 대량 등록
pub async fn insert_events_bulk(
    events: &[EventSchema],
    open_search_client: &OpenSearch,
    conn: Arc<Mutex<Connection>>,
) {
    // 오픈서치에 저장
    let index_name = "events";

    let mut bulk_lines = Vec::with_capacity(events.len() * 2);
    for event in events {
        let action_meta = serde_json::to_string(&json!({"index": {"_index": index_name}})).unwrap();
        bulk_lines.push(action_meta);
        let doc = serde_json::to_string(event).unwrap();
        bulk_lines.push(doc);
    }

    let response = open_search_client
        .bulk(BulkParts::None)
        .body(bulk_lines) // Vec<String>을 전달
        .send()
        .await;

    match response {
        Ok(res) => {
            let status = res.status_code();
            let body: Value = match res.json().await {
                Ok(v) => v,
                Err(e) => {
                    // 이벤트 등록 실패 시 펜딩 데이터베이스에 저장
                    eprintln!("Failed to parse bulk response: {:?}", e);
                    for event in events {
                        insert_pending_event(event, conn.clone()).await;
                    }
                    return;
                }
            };

            if !status.is_success() {
                eprintln!(
                    "Bulk insert request failed with status {}: {:?}",
                    status, body
                );
                for event in events {
                    // 이벤트 등록 실패 시 펜딩 데이터베이스에 저장
                    insert_pending_event(event, conn.clone()).await;
                }
                return;
            }

            let errors = body["errors"].as_bool().unwrap_or(false);
            if errors {
                let blank_vec = vec![];
                let items = body["items"].as_array().unwrap_or(&blank_vec);
                for (i, item) in items.iter().enumerate() {
                    if let Some(index_res) = item.get("index") {
                        let st = index_res["status"].as_u64().unwrap_or(0);
                        if st < 200 || st >= 300 {
                            if let Some(event) = events.get(i) {
                                eprintln!(
                                    "Failed to insert event into OpenSearch (status: {}): {:?}",
                                    st, index_res
                                );
                                // 이벤트 등록 실패 시 펜딩 데이터베이스에 저장
                                insert_pending_event(event, conn.clone()).await;
                            }
                        }
                    } else {
                        if let Some(event) = events.get(i) {
                            insert_pending_event(event, conn.clone()).await;
                        }
                    }
                }
            } else {
                println!(
                    "Bulk inserted {} events into OpenSearch successfully.",
                    events.len()
                );
            }
        }
        Err(e) => {
            eprintln!("Failed to send bulk request to OpenSearch: {:?}", e);
            for event in events {
                // 이벤트 등록 실패 시 펜딩 데이터베이스에 저장
                insert_pending_event(event, conn.clone()).await;
            }
        }
    }
}
