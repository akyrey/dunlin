//! Edits from the key view as Redis commands (D-032). The same plan is previewed and run, so
//! what the user approves is exactly what is sent.

use dbc_core::kv::KeyEdit;
use dbc_core::{DbError, DbErrorCode};

use crate::command::quote_arg;

/// One command's arguments.
pub type Command = Vec<Vec<u8>>;

fn command(parts: &[&[u8]]) -> Command {
    parts.iter().map(|p| p.to_vec()).collect()
}

fn invalid(message: &str) -> DbError {
    DbError::new(DbErrorCode::Internal, message)
}

/// The commands for `edit` on `key`.
pub fn plan(key: &[u8], edit: &KeyEdit) -> Result<Vec<Command>, DbError> {
    let one = |parts: &[&[u8]]| Ok(vec![command(parts)]);
    match edit {
        KeyEdit::SetString { value } => {
            // KEEPTTL: changing the value does not make an expiring key permanent.
            one(&[b"SET", key, value.as_bytes(), b"KEEPTTL"])
        }
        KeyEdit::HashSet { field, value } => {
            one(&[b"HSET", key, field.as_bytes(), value.as_bytes()])
        }
        KeyEdit::HashDelete { field } => one(&[b"HDEL", key, field.as_bytes()]),
        KeyEdit::ListSet { index, value } => {
            one(&[b"LSET", key, index.to_string().as_bytes(), value.as_bytes()])
        }
        KeyEdit::ListPush { value, head } => {
            let push: &[u8] = if *head { b"LPUSH" } else { b"RPUSH" };
            one(&[push, key, value.as_bytes()])
        }
        KeyEdit::ListRemove { value } => one(&[b"LREM", key, b"1", value.as_bytes()]),
        KeyEdit::SetAdd { member } => one(&[b"SADD", key, member.as_bytes()]),
        KeyEdit::SetRemove { member } => one(&[b"SREM", key, member.as_bytes()]),
        KeyEdit::ZsetAdd { member, score } => {
            if !score.is_finite() {
                return Err(invalid("The score must be a finite number"));
            }
            one(&[
                b"ZADD",
                key,
                score.to_string().as_bytes(),
                member.as_bytes(),
            ])
        }
        KeyEdit::ZsetRemove { member } => one(&[b"ZREM", key, member.as_bytes()]),
        KeyEdit::Expire { ttl_ms: None } => one(&[b"PERSIST", key]),
        KeyEdit::Expire { ttl_ms: Some(ttl) } => {
            if *ttl <= 0 {
                return Err(invalid(
                    "The time to live must be positive; delete the key instead",
                ));
            }
            one(&[b"PEXPIRE", key, ttl.to_string().as_bytes()])
        }
        KeyEdit::Rename { new_name } => {
            if new_name.is_empty() {
                return Err(invalid("The new name is empty"));
            }
            one(&[b"RENAMENX", key, new_name.as_bytes()])
        }
        KeyEdit::Delete => one(&[b"DEL", key]),
    }
}

/// A command as it would be typed in the console.
pub fn render(command: &Command) -> String {
    command
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let text = String::from_utf8_lossy(arg);
            if i == 0 {
                text.into_owned()
            } else {
                quote_arg(&text)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a reply means the edit did what it was asked: integer replies that count changes
/// are checked where zero means nothing was there to change.
pub fn changed_nothing(edit: &KeyEdit, reply: &redis::Value) -> Option<&'static str> {
    let zero = matches!(reply, redis::Value::Int(0));
    match edit {
        KeyEdit::ListRemove { .. } if zero => Some("The value is no longer in the list"),
        KeyEdit::Rename { .. } if zero => Some("A key with the new name already exists"),
        // PERSIST also answers 0 for a key that had no expiry; only PEXPIRE means "missing".
        KeyEdit::Delete | KeyEdit::Expire { ttl_ms: Some(_) } if zero => {
            Some("The key no longer exists")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn preview(edit: KeyEdit) -> String {
        plan(b"user:1", &edit)
            .unwrap()
            .iter()
            .map(render)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn edits_become_the_commands_they_preview() {
        assert_eq!(
            preview(KeyEdit::SetString {
                value: "a b".into()
            }),
            r#"SET user:1 "a b" KEEPTTL"#
        );
        assert_eq!(
            preview(KeyEdit::HashSet {
                field: "name".into(),
                value: "Ada".into()
            }),
            "HSET user:1 name Ada"
        );
        assert_eq!(
            preview(KeyEdit::ListSet {
                index: -1,
                value: "z".into()
            }),
            "LSET user:1 -1 z"
        );
        assert_eq!(
            preview(KeyEdit::ListPush {
                value: "x".into(),
                head: true
            }),
            "LPUSH user:1 x"
        );
        assert_eq!(
            preview(KeyEdit::ListRemove { value: "x".into() }),
            "LREM user:1 1 x"
        );
        assert_eq!(
            preview(KeyEdit::ZsetAdd {
                member: "m".into(),
                score: 1.5
            }),
            "ZADD user:1 1.5 m"
        );
        assert_eq!(
            preview(KeyEdit::Expire {
                ttl_ms: Some(60_000)
            }),
            "PEXPIRE user:1 60000"
        );
        assert_eq!(preview(KeyEdit::Expire { ttl_ms: None }), "PERSIST user:1");
        assert_eq!(
            preview(KeyEdit::Rename {
                new_name: "user:2".into()
            }),
            "RENAMENX user:1 user:2"
        );
        assert_eq!(preview(KeyEdit::Delete), "DEL user:1");
    }

    #[test]
    fn meaningless_edits_are_refused() {
        assert!(
            plan(
                b"k",
                &KeyEdit::ZsetAdd {
                    member: "m".into(),
                    score: f64::NAN
                }
            )
            .is_err()
        );
        assert!(plan(b"k", &KeyEdit::Expire { ttl_ms: Some(0) }).is_err());
        assert!(
            plan(
                b"k",
                &KeyEdit::Rename {
                    new_name: String::new()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn a_zero_count_means_the_target_was_gone() {
        let zero = redis::Value::Int(0);
        assert!(
            changed_nothing(
                &KeyEdit::Rename {
                    new_name: "x".into()
                },
                &zero
            )
            .is_some()
        );
        assert!(changed_nothing(&KeyEdit::HashDelete { field: "f".into() }, &zero).is_none());
        assert!(changed_nothing(&KeyEdit::Delete, &redis::Value::Int(1)).is_none());
        assert!(changed_nothing(&KeyEdit::Expire { ttl_ms: None }, &zero).is_none());
    }
}
