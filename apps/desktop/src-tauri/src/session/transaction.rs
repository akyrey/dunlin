//! Manual transaction mode per editor tab (deferred feature 2). In manual mode the tab's
//! connection opens a transaction before the first statement, and it stays open until the user
//! commits or rolls back. Statements the user writes (`BEGIN`, `COMMIT`, ...) are followed too.

use std::collections::HashMap;

use dbc_core::result::ExecuteOptions;
use dbc_core::{DbError, DbErrorCode};
use serde::Serialize;
use ts_rs::TS;

use super::Session;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionState {
    /// Statements do not commit by themselves.
    pub manual: bool,
    /// A transaction is open on the tab's connection.
    pub open: bool,
}

pub type Transactions = std::sync::Mutex<HashMap<String, TransactionState>>;

/// What a statement does to the transaction, from its first word.
fn effect(statement: &str) -> Option<bool> {
    let first = statement
        .split(|c: char| !c.is_ascii_alphabetic())
        .find(|w| !w.is_empty())?
        .to_ascii_uppercase();
    match first.as_str() {
        "BEGIN" | "START" => Some(true),
        "COMMIT" | "ROLLBACK" | "END" | "ABORT" => Some(false),
        _ => None,
    }
}

fn unavailable() -> DbError {
    DbError::new(
        DbErrorCode::Internal,
        "Transaction state is unavailable after a panic",
    )
}

impl Session {
    pub fn transaction(&self, tab_id: &str) -> TransactionState {
        self.transactions
            .lock()
            .ok()
            .and_then(|t| t.get(tab_id).copied())
            .unwrap_or_default()
    }

    fn set_transaction(&self, tab_id: &str, state: TransactionState) -> Result<(), DbError> {
        let mut all = self.transactions.lock().map_err(|_| unavailable())?;
        if state == TransactionState::default() {
            all.remove(tab_id);
        } else {
            all.insert(tab_id.to_owned(), state);
        }
        Ok(())
    }

    /// Turns manual mode on or off. Turning it off with a transaction open is refused, so
    /// nothing is committed or lost without the user saying so.
    pub fn set_manual(&self, tab_id: &str, manual: bool) -> Result<TransactionState, DbError> {
        let state = self.transaction(tab_id);
        if !manual && state.open {
            return Err(DbError::new(
                DbErrorCode::Internal,
                "Commit or roll back the open transaction first",
            ));
        }
        let next = TransactionState { manual, ..state };
        self.set_transaction(tab_id, next)?;
        Ok(next)
    }

    /// Runs one statement, opening the manual transaction first when needed, and follows the
    /// transaction statements the user runs.
    pub async fn execute_in_transaction(
        &self,
        tab_id: &str,
        sql: &str,
        options: &ExecuteOptions,
    ) -> Result<dbc_core::result::StatementOutcome, DbError> {
        let state = self.transaction(tab_id);
        let effect = effect(sql);
        if state.manual && !state.open && effect.is_none() {
            self.execute(tab_id, "BEGIN", &ExecuteOptions::default())
                .await?;
            self.set_transaction(
                tab_id,
                TransactionState {
                    open: true,
                    ..state
                },
            )?;
        }
        let outcome = self.execute(tab_id, sql, options).await;
        if outcome.is_ok()
            && let Some(open) = effect
        {
            let now = self.transaction(tab_id);
            self.set_transaction(tab_id, TransactionState { open, ..now })?;
        }
        outcome
    }

    /// `COMMIT` or `ROLLBACK` of the tab's open transaction.
    pub async fn finish_transaction(
        &self,
        tab_id: &str,
        commit: bool,
    ) -> Result<TransactionState, DbError> {
        let state = self.transaction(tab_id);
        if !state.open {
            return Ok(state);
        }
        let sql = if commit { "COMMIT" } else { "ROLLBACK" };
        self.execute(tab_id, sql, &ExecuteOptions::default())
            .await?;
        let next = TransactionState {
            open: false,
            ..state
        };
        self.set_transaction(tab_id, next)?;
        Ok(next)
    }

    /// The tab's connection is gone, and its transaction with it.
    pub(super) fn transaction_lost(&self, tab_id: &str) {
        let state = self.transaction(tab_id);
        // Failing here only leaves a stale "open" flag, which the next commit clears.
        let _ = self.set_transaction(
            tab_id,
            TransactionState {
                open: false,
                ..state
            },
        );
    }
}

#[cfg(test)]
mod tests;
