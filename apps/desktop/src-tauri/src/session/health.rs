//! Idle connection handling (task 8.15): open sessions are pinged now and then. An idle
//! connection that no longer answers is dropped, so the next use reconnects without the user
//! noticing; a server that cannot be reached is reported, and the report clears when it is back.

use std::sync::Arc;
use std::time::Duration;

use dbc_core::driver::Connection;

use super::{Session, SessionManager};

/// How often open sessions are checked.
pub const PING_INTERVAL: Duration = Duration::from_secs(60);
/// A ping that takes longer counts as no answer.
pub const PING_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    /// Neither the metadata connection nor a new one answers.
    Unreachable,
    /// The SSH tunnel dropped; the session must be closed (task 3.8).
    TunnelLost,
}

async fn answers(connection: &Arc<dyn Connection>) -> bool {
    matches!(
        tokio::time::timeout(PING_TIMEOUT, connection.ping()).await,
        Ok(Ok(()))
    )
}

impl Session {
    /// Pings the idle connections, drops those that do not answer, and checks that the server
    /// can still be reached. Connections with a statement in flight are left alone.
    pub async fn check_health(&self) -> Health {
        if self.tunnel_lost() {
            return Health::TunnelLost;
        }
        for (tab_id, connection) in self.idle_tabs().await {
            if !answers(&connection).await {
                self.forget_tab(&tab_id, &connection).await;
            }
        }
        if self.metadata_alive().await {
            Health::Ok
        } else {
            Health::Unreachable
        }
    }

    async fn idle_tabs(&self) -> Vec<(String, Arc<dyn Connection>)> {
        let running = self.running.lock().map(|r| r.clone()).unwrap_or_default();
        self.tabs
            .lock()
            .await
            .iter()
            .filter(|(id, _)| !running.contains(*id))
            .map(|(id, c)| (id.clone(), Arc::clone(c)))
            .collect()
    }

    /// The metadata connection answers, or a new one could be opened in its place.
    async fn metadata_alive(&self) -> bool {
        let current = self.metadata.lock().await.clone();
        if let Some(connection) = current {
            if answers(&connection).await {
                return true;
            }
            self.drop_metadata(&connection).await;
        }
        self.metadata().await.is_ok()
    }

    async fn drop_metadata(&self, lost: &Arc<dyn Connection>) {
        let mut slot = self.metadata.lock().await;
        if slot.as_ref().is_some_and(|c| Arc::ptr_eq(c, lost)) {
            *slot = None;
        }
        drop(slot);
        // Closing a connection that stopped answering only releases local resources.
        let _ = lost.close().await;
    }
}

impl SessionManager {
    /// The health of every open session, by session id.
    pub async fn check_all(&self) -> Vec<(String, Health)> {
        let sessions: Vec<Arc<Session>> = self.sessions.read().await.values().cloned().collect();
        let mut report = Vec::with_capacity(sessions.len());
        for session in sessions {
            report.push((session.id.clone(), session.check_health().await));
        }
        report
    }
}

#[cfg(test)]
mod tests;
