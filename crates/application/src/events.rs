use futures_util::{Stream, StreamExt, stream};
use pixivarchive_db::{Db, DbError, EventNotificationListener, EventRepository};
use pixivarchive_domain::event::AppEvent;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

type WakeStream = Pin<Box<dyn Stream<Item = Result<(), DbError>> + Send>>;

#[derive(Clone)]
pub struct EventStream {
    db: Db,
    database_url: String,
    batch_limit: i64,
    poll_interval: Duration,
    wake_stream: Option<Arc<Mutex<Option<WakeStream>>>>,
}

impl EventStream {
    pub fn new(db: Db, database_url: impl Into<String>) -> Self {
        Self {
            db,
            database_url: database_url.into(),
            batch_limit: 1_000,
            poll_interval: Duration::from_secs(5),
            wake_stream: None,
        }
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn with_wake_stream<S>(mut self, wake_stream: S) -> Self
    where
        S: Stream<Item = Result<(), DbError>> + Send + 'static,
    {
        self.wake_stream = Some(Arc::new(Mutex::new(Some(Box::pin(wake_stream)))));
        self
    }

    pub async fn after(&self, last_event_id: Option<i64>) -> Result<OpenedEventStream, DbError> {
        let wakes = if let Some(wake_stream) = &self.wake_stream {
            wake_stream.lock().unwrap().take()
        } else {
            Some(open_pg_wake_stream(&self.database_url).await?)
        };
        let replay = EventRepository::new(self.db.clone())
            .replay_window(last_event_id, self.batch_limit)
            .await?;
        let cursor = replay
            .events
            .last()
            .map(|event| event.id)
            .or(replay.latest_event_id)
            .or(last_event_id)
            .unwrap_or(0);
        let mut pending = VecDeque::new();
        if replay.snapshot_refresh {
            pending.push_back(StreamEvent::SnapshotRefresh {
                latest_event_id: replay.latest_event_id.unwrap_or(0),
            });
        } else {
            pending.extend(replay.events.into_iter().map(StreamEvent::AppEvent));
        }
        Ok(OpenedEventStream {
            db: self.db.clone(),
            wakes,
            cursor,
            batch_limit: self.batch_limit,
            poll_interval: self.poll_interval,
            pending,
        })
    }
}

pub struct OpenedEventStream {
    db: Db,
    wakes: Option<WakeStream>,
    cursor: i64,
    batch_limit: i64,
    poll_interval: Duration,
    pending: VecDeque<StreamEvent>,
}

impl OpenedEventStream {
    pub fn into_stream(self) -> impl Stream<Item = Result<StreamEvent, DbError>> + Send + 'static {
        stream::unfold(self, |mut state| async move {
            loop {
                if let Some(event) = state.pending.pop_front() {
                    if let StreamEvent::AppEvent(event) = &event {
                        state.cursor = event.id;
                    }
                    return Some((Ok(event), state));
                }

                let wake = async {
                    match state.wakes.as_mut() {
                        Some(wakes) => wakes.next().await,
                        None => std::future::pending().await,
                    }
                };

                tokio::select! {
                    wake = wake => {
                        match wake {
                            Some(Ok(())) => {}
                            Some(Err(_)) => {
                                tokio::time::sleep(state.poll_interval).await;
                            }
                            None => {
                                state.wakes = None;
                            }
                        }
                    }
                    _ = tokio::time::sleep(state.poll_interval) => {}
                }

                match EventRepository::new(state.db.clone())
                    .list_after(state.cursor, state.batch_limit)
                    .await
                {
                    Ok(events) => {
                        state
                            .pending
                            .extend(events.into_iter().map(StreamEvent::AppEvent));
                    }
                    Err(error) => return Some((Err(error), state)),
                }
            }
        })
    }
}

async fn open_pg_wake_stream(database_url: &str) -> Result<WakeStream, DbError> {
    let listener = EventNotificationListener::connect(database_url).await?;
    Ok(Box::pin(stream::unfold(
        listener,
        |mut listener| async move {
            let result = listener.recv().await;
            Some((result, listener))
        },
    )))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    AppEvent(AppEvent),
    SnapshotRefresh { latest_event_id: i64 },
}
