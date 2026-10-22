//! Live updates: every master process announces changes with PostgreSQL `NOTIFY`; the processes that
//! serve the admin API listen for them and pass them on to browsers as server-sent events.

use super::Shared;
use crate::platform::db::CHANNEL;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
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

pub async fn stream(State(state): State<Shared>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = BroadcastStream::new(state.events.subscribe()).map(|received| {
        Ok(Event::default().data(match received {
            Ok(payload) => payload,
            // This browser fell behind: tell it to refresh instead of replaying.
            Err(BroadcastStreamRecvError::Lagged(_)) => r#"{"topic":"resync","key":""}"#.to_string(),
        }))
    });
    Sse::new(events).keep_alive(KeepAlive::default())
}
