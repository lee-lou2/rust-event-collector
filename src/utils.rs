use crate::handlers::EventSchema;
use chrono::Datelike;
use opensearch::{BulkParts, OpenSearch};
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// Register pending events
/// Save pending data to the database when the channel is full or an error occurs while saving to OpenSearch.
/// The stored data will be retrieved periodically and reinserted into OpenSearch.
pub async fn insert_pending_events(
    events: &[EventSchema],
    db_pool: &SqlitePool,
) -> Result<(), sqlx::Error> {
    // Start transaction
    let mut tx = db_pool.begin().await?;
    for event in events {
        // Serialize event to JSON string
        let serialized_event = serde_json::to_string(event).unwrap();
        // Use placeholders in the query and bind values
        sqlx::query!("INSERT INTO events (log) VALUES (?)", serialized_event)
            .execute(&mut *tx) // Explicitly dereference transaction
            .await?;
    }
    // Commit transaction
    tx.commit().await?;
    Ok(())
}

/// Bulk insert events
/// Code for storing a large number of events in bulk in OpenSearch.
/// If an error occurs during saving, the data is saved in the pending database and the error is output.
pub async fn insert_events_bulk(
    events: &[EventSchema],
    open_search_client: &OpenSearch,
    db_pool: &SqlitePool,
) {
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap());
    let envs = crate::config::get_environments();
    let index_name = format!(
        "events_{}_{}-{}",
        &envs.server_environment,
        now.year(),
        now.month()
    );

    let mut bulk_lines = Vec::with_capacity(events.len() * 2);
    for event in events {
        let action_meta = serde_json::to_string(&json!({"index": {"_index": index_name}})).unwrap();
        bulk_lines.push(action_meta);
        let doc = serde_json::to_string(event).unwrap();
        bulk_lines.push(doc);
    }

    let response = open_search_client
        .bulk(BulkParts::None)
        .body(bulk_lines) // Pass Vec<String>
        .send()
        .await;

    match response {
        Ok(res) => {
            let status = res.status_code();
            let body: Value = match res.json().await {
                Ok(v) => v,
                Err(e) => {
                    // Save to pending database if event registration fails
                    eprintln!("Failed to parse bulk response: {:?}", e);
                    match insert_pending_events(events, db_pool).await {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Failed to insert pending events: {:?}", e);
                        }
                    }
                    return;
                }
            };

            if !status.is_success() {
                eprintln!(
                    "Bulk insert request failed with status {}: {:?}",
                    status, body
                );
                match insert_pending_events(events, db_pool).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed to insert pending events: {:?}", e);
                    }
                }
                return;
            }

            let errors = body["errors"].as_bool().unwrap_or(false);
            if errors {
                let blank_vec = vec![];
                let items = body["items"].as_array().unwrap_or(&blank_vec);
                let mut failed_events = vec![];
                for (i, item) in items.iter().enumerate() {
                    if let Some(index_res) = item.get("index") {
                        let st = index_res["status"].as_u64().unwrap_or(0);
                        if !(200..300).contains(&st) {
                            if let Some(event) = events.get(i) {
                                eprintln!(
                                    "Failed to insert event into OpenSearch (status: {}): {:?}",
                                    st, index_res
                                );
                                failed_events.push((*event).clone());
                            }
                        }
                    } else if let Some(event) = events.get(i) {
                        failed_events.push((*event).clone());
                    }
                }
                // Save failed data as pending
                let events = &failed_events[..];
                match insert_pending_events(events, db_pool).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed to insert pending events: {:?}", e);
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
            match insert_pending_events(events, db_pool).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Failed to insert pending events: {:?}", e);
                }
            }
        }
    }
}
