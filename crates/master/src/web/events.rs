//! Live updates: changes are announced with PostgreSQL `NOTIFY`; the master listens for them on a connection of its
//! own and passes them on to browsers as server-sent events.

use super::access::Access;
use super::Shared;
use crate::platform::db::CHANNEL;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Extension;
use futures_util::Stream;
use postgres::{Config, NoTls};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

/// Listens for notifications on a connection of its own and feeds them to the event streams.
pub fn listen(state: Shared, url: String) {
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

fn listen_once(state: &Shared, config: &Config) -> Result<(), postgres::Error> {
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

/// Whether a change is for this user. Changes to files, devices, activity and alerts of a device are announced with the
/// folder of the device as their key, and are for the owner, whoever can see the master files (for the master's) and
/// the administrators. A source is announced with the folder of its device, so it is for its owner. Everything else
/// (mirrors, users) is for the administrators.
fn is_for(who: &Access, payload: &str) -> bool {
    if who.is_admin() {
        return true;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else { return false };
    let key = value["key"].as_str().unwrap_or("");
    match value["topic"].as_str() {
        // Not about one device, or only a hint to look again.
        Some("resync" | "library") => true,
        Some("files" | "devices" | "activity" | "alerts") => key.is_empty() || who.can_see(key),
        // The key of a mirror is not a folder.
        Some("jobs") => key.contains('/') && who.can_see(key),
        _ => false,
    }
}

pub async fn stream(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = BroadcastStream::new(state.events.subscribe())
        .map(|received| match received {
            Ok(payload) => payload,
            // This browser fell behind: tell it to refresh instead of replaying.
            Err(BroadcastStreamRecvError::Lagged(_)) => r#"{"topic":"resync","key":""}"#.to_string(),
        })
        .filter(move |payload| is_for(&who, payload))
        .map(|payload| Ok(Event::default().data(payload)));
    Sse::new(events).keep_alive(KeepAlive::default())
}
