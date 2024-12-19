use crate::handlers::EventSchema;
use crate::utils::insert_events_bulk;
use serde_json::from_str;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio::time::interval;

// 이벤트 컨슈머
pub async fn consume_events(
    rx: &Arc<Mutex<Receiver<EventSchema>>>,
    open_search_client: &opensearch::OpenSearch,
    conn: &Arc<Mutex<rusqlite::Connection>>,
) {
    tokio::spawn({
        // 기본 정보 클로닝
        let rx = Arc::clone(&rx);
        let open_search_client = open_search_client.clone();
        let conn = Arc::clone(&conn);

        async move {
            // 벌크 등록을 위한 설정
            let batch_size = 1000;
            let flush_interval = Duration::from_secs(10);
            let mut event_buffer = Vec::with_capacity(batch_size);
            let mut ticker = interval(flush_interval);

            loop {
                tokio::select! {
                     // 이벤트 컨슈밍
                    maybe_event = async {
                        let mut rx_guard = rx.lock().await;
                        rx_guard.recv().await
                    } => {
                        match maybe_event {
                            // 이벤트가 확인되면 버퍼에 저장
                            Some(event) => {
                                event_buffer.push(event);
                                if event_buffer.len() >= batch_size {
                                    // 버퍼가 배치 사이즈보다 큰 경우 이벤트 등록
                                    insert_events_bulk(&event_buffer, &open_search_client, conn.clone()).await;
                                    event_buffer.clear();
                                }
                            }
                            None => {
                                if !event_buffer.is_empty() {
                                    // 잔여 이벤트 대량 등록
                                    insert_events_bulk(&event_buffer, &open_search_client, conn.clone()).await;
                                    event_buffer.clear();
                                }
                                break;
                            }
                        }
                    },

                    _ = ticker.tick() => {
                        if !event_buffer.is_empty() {
                            // 배치 사이즈만큼 채워지지 않더라도 특정 시간 마다 등록
                            insert_events_bulk(&event_buffer, &open_search_client, conn.clone()).await;
                            event_buffer.clear();
                        }
                    }
                }
            }
        }
    });
}

// 펜딩된 이벤트를 전달하는 스케쥴러
pub async fn schedule_insert_pending_events(
    tx: &tokio::sync::mpsc::Sender<EventSchema>,
    conn: &Arc<Mutex<rusqlite::Connection>>,
) {
    tokio::spawn({
        // 기본 정보 클로닝
        let conn = Arc::clone(&conn);
        let tx = Arc::new(tx.clone());

        async move {
            // 반복 주기 설정
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                // 펜딩 이벤트 조회
                let sql = "SELECT id, log FROM events LIMIT 1000";
                let locked_conn = conn.lock().await;
                let mut stmt = match locked_conn.prepare(sql) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error preparing statement: {:?}", e);
                        continue;
                    }
                };

                let mut rows = match stmt.query([]) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Error querying events: {:?}", e);
                        continue;
                    }
                };

                while let Ok(Some(row)) = rows.next() {
                    // 조회된 이벤트 데이터를 프로듀싱
                    let id: i64 = row.get(0).unwrap();
                    let log: String = row.get(1).unwrap();
                    let data: EventSchema = from_str(&log).unwrap();
                    match tx.try_send(data) {
                        Ok(_) => {
                            // 이벤트 전달 완료 시 조회된 데이터 삭제
                            if let Err(e) =
                                locked_conn.execute("DELETE FROM events WHERE id = ?", &[&id])
                            {
                                eprintln!("Error deleting event with id {}: {:?}", id, e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error sending data to channel: {:?}", e);
                        }
                    }
                }
            }
        }
    });
}
