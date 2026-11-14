//! The background task that checks open sessions (task 8.15) and tells the frontend when one
//! changes state, through the `session_health` event.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::session::{Health, PING_INTERVAL};
use crate::state::AppState;

pub const EVENT: &str = "session_health";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Ok,
    Unreachable,
    TunnelLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionHealthEvent {
    pub session_id: String,
    pub status: SessionStatus,
}

fn status(health: Health) -> SessionStatus {
    match health {
        Health::Ok => SessionStatus::Ok,
        Health::Unreachable => SessionStatus::Unreachable,
        Health::TunnelLost => SessionStatus::TunnelLost,
    }
}

/// The events for a new report: a session is announced when its status changes, and a new
/// session only when it is not fine (the frontend assumes a new session is). Returns the
/// statuses to compare the next report with.
pub fn changes(
    last: &HashMap<String, SessionStatus>,
    report: Vec<(String, Health)>,
) -> (Vec<SessionHealthEvent>, HashMap<String, SessionStatus>) {
    let mut events = Vec::new();
    let mut next = HashMap::new();
    for (session_id, health) in report {
        let now = status(health);
        let before = last.get(&session_id).copied().unwrap_or(SessionStatus::Ok);
        if now != before {
            events.push(SessionHealthEvent {
                session_id: session_id.clone(),
                status: now,
            });
        }
        if now != SessionStatus::TunnelLost {
            next.insert(session_id, now);
        }
    }
    (events, next)
}

/// Starts checking every `PING_INTERVAL` for the life of the app.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticks = tokio::time::interval(PING_INTERVAL);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick is immediate; sessions just opened need no check.
        ticks.tick().await;
        let mut last = HashMap::new();
        loop {
            ticks.tick().await;
            let state = app.state::<AppState>();
            let (events, next) = changes(&last, state.sessions.check_all().await);
            last = next;
            for event in events {
                if event.status == SessionStatus::TunnelLost {
                    // Unusable either way; a failure to close it changes nothing for the user.
                    let _ = state.sessions.close(&event.session_id).await;
                }
                if let Err(error) = app.emit(EVENT, &event) {
                    tracing::warn!(%error, "session health was not sent to the window");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changes_are_announced_and_a_lost_tunnel_once() {
        let first = vec![
            ("a".to_owned(), Health::Ok),
            ("b".to_owned(), Health::Unreachable),
        ];
        let (events, last) = changes(&HashMap::new(), first);
        assert_eq!(
            events,
            vec![SessionHealthEvent {
                session_id: "b".into(),
                status: SessionStatus::Unreachable
            }]
        );

        let second = vec![
            ("a".to_owned(), Health::TunnelLost),
            ("b".to_owned(), Health::Ok),
        ];
        let (events, last) = changes(&last, second);
        let statuses: Vec<_> = events
            .iter()
            .map(|e| (e.session_id.as_str(), e.status))
            .collect();
        assert_eq!(
            statuses,
            vec![("a", SessionStatus::TunnelLost), ("b", SessionStatus::Ok)]
        );
        assert!(!last.contains_key("a"));
    }
}
