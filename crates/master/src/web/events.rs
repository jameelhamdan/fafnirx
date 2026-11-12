//! Live updates: changes are announced with PostgreSQL `NOTIFY`; the master listens for them on a connection of its
//! own and passes them on to browsers as server-sent events.

use super::access::{Access, Ctx};
use super::App;
use crate::platform::db::{Reach, Topic, CHANNEL};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use postgres::{Config, NoTls};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};
use tokio_stream::StreamExt;

/// Listens for notifications on a connection of its own and feeds them to the event streams.
pub fn listen(state: App, url: String) {
    std::thread::spawn(move || {
        let config: Config = match url.parse() {
            Ok(c) => c,
            Err(e) => return tracing::error!("live updates: bad database URL: {e}"),
        };
        loop {
            if let Err(e) = listen_once(&state, &config) {
                tracing::warn!("live updates: {e}; reconnecting");
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn listen_once(state: &App, config: &Config) -> Result<(), postgres::Error> {
    use postgres::fallible_iterator::FallibleIterator;
    let mut client = config.connect(NoTls)?;
    client.batch_execute(&format!("LISTEN {CHANNEL}"))?;
    // Browsers that missed something while this was down refresh everything.
    let _ = state.events.send(r#"{"topic":"resync","key":""}"#.into());
    loop {
        {
            let mut notifications = client.notifications();
            let mut waiting = notifications.timeout_iter(Duration::from_secs(30));
            while let Some(n) = waiting.next()? {
                // Nobody listening is fine.
                let _ = state.events.send(n.payload().to_string());
            }
        }
        // Nothing arrived for a while: this also notices a connection that died quietly.
        client.simple_query("SELECT 1")?;
    }
}

/// Whether a change is for this user (see `Reach`). A source is announced with `<user>/<name>`, so it is for whoever can see the
/// folder of that name; a mirror (`mirrors`) is for the administrators.
fn is_for(who: &Access, payload: &str) -> bool {
    if who.is_admin() {
        return true;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else { return false };
    let key = value["key"].as_str().unwrap_or("");
    let Some(topic) = value["topic"].as_str().and_then(Topic::parse) else { return false };
    match topic.reach() {
        Reach::Everyone => true,
        Reach::Folder => key.is_empty() || who.can_see(key),
        Reach::Source => key.contains('/') && who.can_see(key),
        Reach::Admins => false,
    }
}

/// How often an open stream looks again at its login: a member taken off a folder stops hearing about it, and a stream
/// whose session ended (logged out, disabled) closes; the browser's reconnect then meets the login check.
const RECHECK: Duration = Duration::from_secs(60);

pub async fn stream(Ctx { state, who }: Ctx, headers: HeaderMap) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
    let mut changes = BroadcastStream::new(state.events.subscribe());
    tokio::spawn(async move {
        let mut who = who;
        let mut recheck = tokio::time::interval_at(tokio::time::Instant::now() + RECHECK, RECHECK);
        loop {
            let payload = tokio::select! {
                // The browser went away.
                _ = tx.closed() => return,
                _ = recheck.tick() => {
                    let headers = headers.clone();
                    match crate::platform::db::with(&state.db, move |c| super::auth::user_of_session(c, &headers)).await {
                        Ok(Some(now)) => who = now,
                        Ok(None) => return,
                        // The database is away for a moment: the stream goes on with what it knew.
                        Err(e) => tracing::debug!("live updates: recheck the login: {e}"),
                    }
                    continue;
                }
                received = changes.next() => match received {
                    Some(Ok(payload)) => payload,
                    // This browser fell behind: tell it to refresh instead of replaying.
                    Some(Err(BroadcastStreamRecvError::Lagged(_))) => r#"{"topic":"resync","key":""}"#.to_string(),
                    None => return,
                },
            };
            if is_for(&who, &payload) && tx.send(payload).await.is_err() {
                return;
            }
        }
    });
    let events = ReceiverStream::new(rx).map(|payload| Ok(Event::default().data(payload)));
    Sse::new(events).keep_alive(KeepAlive::default())
}
