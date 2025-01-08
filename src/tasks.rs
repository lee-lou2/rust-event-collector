use crate::handlers::EventSchema;
use crate::utils::insert_events_bulk;
use serde_json::from_str;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio::time::interval;

/// Event consumer
/// When data is registered in the channel, consume it and group it in batches of 1000 or group events within 10 seconds and pass them to the bulk insert function
pub async fn consume_events(
    rx: &Arc<Mutex<Receiver<EventSchema>>>,
    open_search_client: &opensearch::OpenSearch,
    db_pool: &SqlitePool,
) {
    tokio::spawn({
        // Clone basic information
        let rx = Arc::clone(rx);
        let open_search_client = open_search_client.clone();
        let db_pool = db_pool.clone();

        async move {
            // Configuration for bulk insertion
            let batch_size = 1000;
            let mut event_buffer = Vec::with_capacity(batch_size);
            let mut ticker = interval(Duration::from_secs(10));

            loop {
                tokio::select! {
                     // Event consumption
                    maybe_event = async {
                        let mut rx_guard = rx.lock().await;
                        rx_guard.recv().await
                    } => {
                        match maybe_event {
                            // If an event is found, store it in the buffer
                            Some(event) => {
                                event_buffer.push(event);
                                if event_buffer.len() >= batch_size {
                                    // If the buffer exceeds the batch size, insert events
                                    insert_events_bulk(&event_buffer, &open_search_client, &db_pool).await;
                                    event_buffer.clear();
                                }
                            }
                            None => {
                                if !event_buffer.is_empty() {
                                    // Bulk insert remaining events
                                    insert_events_bulk(&event_buffer, &open_search_client, &db_pool).await;
                                    event_buffer.clear();
                                }
                                break;
                            }
                        }
                    },

                    _ = ticker.tick() => {
                        if !event_buffer.is_empty() {
                            // Insert periodically even without reaching batch size
                            insert_events_bulk(&event_buffer, &open_search_client, &db_pool).await;
                            event_buffer.clear();
                        }
                    }
                }
            }
        }
    });
}

/// Scheduler to forward pending events
/// Retrieves pending events from the database at specific intervals and inputs them into the channel
pub async fn schedule_insert_pending_events(
    tx: &tokio::sync::mpsc::Sender<EventSchema>,
    db_pool: &SqlitePool,
) {
    tokio::spawn({
        // Clone basic information
        let db_pool = db_pool.clone();
        let tx = Arc::new(tx.clone());

        async move {
            // Set repeat period
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                // Fetch pending events
                match sqlx::query!("SELECT id, log FROM events LIMIT 1000")
                    .fetch_all(&db_pool)
                    .await
                {
                    Ok(rows) => {
                        for row in rows {
                            // Produce fetched event data
                            let id = row.id;
                            let log = row.log.unwrap_or(String::from(""));
                            let data: EventSchema = from_str(&log).unwrap();
                            match tx.try_send(data) {
                                Ok(_) => {
                                    match sqlx::query!("DELETE FROM events WHERE id = ?", id)
                                        .execute(&db_pool)
                                        .await
                                    {
                                        Ok(_) => {}
                                        Err(e) => {
                                            eprintln!(
                                                "Error deleting event with id {}: {:?}",
                                                id, e
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error sending data to channel: {:?}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error fetching events: {:?}", e);
                    }
                }
            }
        }
    });
}
