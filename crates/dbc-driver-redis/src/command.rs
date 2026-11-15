//! Redis commands as typed in the editor: redis-cli quoting, one command per line, and the
//! read/write classification behind read-only connections and confirmations.

use dbc_core::sql::{Classification, Danger, StatementClass, StatementRange};

/// Splits one line into arguments as redis-cli does: whitespace separates; `"..."` takes
/// `\n \r \t \b \a \\ \" \xHH` escapes; `'...'` takes only `\'`. `None` for an unclosed quote.
pub fn parse_args(line: &str) -> Option<Vec<Vec<u8>>> {
    let mut args = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&first) = chars.peek() else {
            return Some(args);
        };
        let mut arg = Vec::new();
        match first {
            '"' => {
                chars.next();
                double_quoted(&mut chars, &mut arg)?;
            }
            '\'' => {
                chars.next();
                single_quoted(&mut chars, &mut arg)?;
            }
            _ => {
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    push_char(&mut arg, c);
                    chars.next();
                }
            }
        }
        // A closing quote must be followed by whitespace or the end, as in redis-cli.
        if matches!(first, '"' | '\'') && chars.peek().is_some_and(|c| !c.is_whitespace()) {
            return None;
        }
        args.push(arg);
    }
}

fn push_char(arg: &mut Vec<u8>, c: char) {
    let mut buffer = [0; 4];
    arg.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
}

fn double_quoted(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    arg: &mut Vec<u8>,
) -> Option<()> {
    loop {
        match chars.next()? {
            '"' => return Some(()),
            '\\' => match chars.next()? {
                'n' => arg.push(b'\n'),
                'r' => arg.push(b'\r'),
                't' => arg.push(b'\t'),
                'b' => arg.push(0x08),
                'a' => arg.push(0x07),
                'x' => {
                    let high = chars.next()?.to_digit(16)?;
                    let low = chars.next()?.to_digit(16)?;
                    arg.push(u8::try_from(high * 16 + low).ok()?);
                }
                other => push_char(arg, other),
            },
            c => push_char(arg, c),
        }
    }
}

fn single_quoted(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    arg: &mut Vec<u8>,
) -> Option<()> {
    loop {
        match chars.next()? {
            '\'' => return Some(()),
            '\\' if chars.peek() == Some(&'\'') => {
                chars.next();
                arg.push(b'\'');
            }
            c => push_char(arg, c),
        }
    }
}

/// Quotes an argument so `parse_args` reads it back, only when it needs quoting.
pub fn quote_arg(arg: &str) -> String {
    let plain = !arg.is_empty()
        && !arg
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'));
    if plain {
        return arg.to_owned();
    }
    let mut quoted = String::from('"');
    for c in arg.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// One command per non-empty line; lines starting with `#` or `//` are comments.
pub fn split_lines(text: &str) -> Vec<StatementRange> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let trimmed = content.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//") {
            let start = offset + (content.len() - content.trim_start().len());
            ranges.push(StatementRange {
                start,
                end: start + trimmed.len(),
            });
        }
        offset += line.len();
    }
    ranges
}

/// Commands that only read. Anything not listed (or unknown) counts as a write.
const READS: &[&str] = &[
    "GET",
    "MGET",
    "STRLEN",
    "GETRANGE",
    "SUBSTR",
    "LCS",
    "EXISTS",
    "TYPE",
    "TTL",
    "PTTL",
    "EXPIRETIME",
    "PEXPIRETIME",
    "DBSIZE",
    "KEYS",
    "SCAN",
    "HSCAN",
    "SSCAN",
    "ZSCAN",
    "RANDOMKEY",
    "DUMP",
    "OBJECT",
    "MEMORY",
    "TOUCH",
    "SORT_RO",
    "HGET",
    "HMGET",
    "HGETALL",
    "HKEYS",
    "HVALS",
    "HLEN",
    "HEXISTS",
    "HSTRLEN",
    "HRANDFIELD",
    "HTTL",
    "HPTTL",
    "LRANGE",
    "LINDEX",
    "LLEN",
    "LPOS",
    "SMEMBERS",
    "SISMEMBER",
    "SMISMEMBER",
    "SCARD",
    "SRANDMEMBER",
    "SINTER",
    "SUNION",
    "SDIFF",
    "SINTERCARD",
    "ZRANGE",
    "ZRANGEBYSCORE",
    "ZREVRANGE",
    "ZREVRANGEBYSCORE",
    "ZRANGEBYLEX",
    "ZREVRANGEBYLEX",
    "ZSCORE",
    "ZMSCORE",
    "ZRANK",
    "ZREVRANK",
    "ZCARD",
    "ZCOUNT",
    "ZLEXCOUNT",
    "ZRANDMEMBER",
    "ZINTER",
    "ZUNION",
    "ZDIFF",
    "ZINTERCARD",
    "XRANGE",
    "XREVRANGE",
    "XLEN",
    "XINFO",
    "XPENDING",
    "XREAD",
    "BITCOUNT",
    "BITPOS",
    "GETBIT",
    "BITFIELD_RO",
    "PFCOUNT",
    "GEOPOS",
    "GEODIST",
    "GEOHASH",
    "GEOSEARCH",
    "GEORADIUS_RO",
    "GEORADIUSBYMEMBER_RO",
    "EVAL_RO",
    "EVALSHA_RO",
    "FCALL_RO",
    "INFO",
    "PING",
    "ECHO",
    "TIME",
    "LASTSAVE",
    "COMMAND",
    "ROLE",
    "LOLWUT",
    "SLOWLOG",
    "LATENCY",
    "JSON.GET",
    "JSON.MGET",
    "JSON.TYPE",
    "JSON.STRLEN",
    "JSON.OBJKEYS",
    "JSON.OBJLEN",
    "JSON.ARRLEN",
];
/// `CONFIG GET`, `CLIENT LIST`, ...: reads among commands with subcommands.
const READ_SUBCOMMANDS: &[(&str, &[&str])] = &[
    ("CONFIG", &["GET"]),
    ("CLIENT", &["LIST", "INFO", "GETNAME", "ID", "TRACKINGINFO"]),
    (
        "CLUSTER",
        &[
            "INFO",
            "NODES",
            "SLOTS",
            "SHARDS",
            "MYID",
            "KEYSLOT",
            "COUNTKEYSINSLOT",
        ],
    ),
    ("SCRIPT", &["EXISTS"]),
    ("FUNCTION", &["LIST", "DUMP", "STATS"]),
    ("ACL", &["WHOAMI", "CAT", "LIST", "USERS", "GETUSER", "LOG"]),
    (
        "PUBSUB",
        &[
            "CHANNELS",
            "NUMSUB",
            "NUMPAT",
            "SHARDCHANNELS",
            "SHARDNUMSUB",
        ],
    ),
];
const SESSION: &[&str] = &[
    "SELECT",
    "AUTH",
    "HELLO",
    "RESET",
    "QUIT",
    "READONLY",
    "READWRITE",
];
const TRANSACTION: &[&str] = &["MULTI", "EXEC", "DISCARD", "WATCH", "UNWATCH"];
const DANGERS: &[(&str, &str)] = &[
    ("FLUSHALL", "FLUSHALL deletes every key in every database"),
    ("FLUSHDB", "FLUSHDB deletes every key in the database"),
    ("SWAPDB", "SWAPDB exchanges two whole databases"),
    ("SHUTDOWN", "SHUTDOWN stops the server"),
    ("DEBUG", "DEBUG can crash or stall the server"),
    (
        "KEYS",
        "KEYS scans every key and blocks the server meanwhile; prefer SCAN",
    ),
];

/// Commands that take over the connection (push replies); the console does not support them.
pub const UNSUPPORTED: &[&str] = &[
    "SUBSCRIBE",
    "PSUBSCRIBE",
    "SSUBSCRIBE",
    "UNSUBSCRIBE",
    "PUNSUBSCRIBE",
    "SUNSUBSCRIBE",
    "MONITOR",
    "SYNC",
    "PSYNC",
];

pub fn classify(statement: &str) -> Classification {
    let args = parse_args(statement).unwrap_or_default();
    let word = |i: usize| {
        args.get(i)
            .map(|a| String::from_utf8_lossy(a).to_ascii_uppercase())
            .unwrap_or_default()
    };
    let (name, sub) = (word(0), word(1));
    let danger = DANGERS
        .iter()
        .find(|(command, _)| *command == name)
        .map(|(_, reason)| Danger::Command(reason))
        .or_else(|| {
            (matches!(
                (name.as_str(), sub.as_str()),
                ("SCRIPT" | "FUNCTION", "FLUSH")
            ))
            .then_some(Danger::Command(
                "This deletes every script or function on the server",
            ))
        });
    let class = if name.is_empty() || UNSUPPORTED.contains(&name.as_str()) {
        StatementClass::Unknown
    } else if READS.contains(&name.as_str())
        || READ_SUBCOMMANDS
            .iter()
            .any(|(command, subs)| *command == name && subs.contains(&sub.as_str()))
    {
        StatementClass::Read
    } else if SESSION.contains(&name.as_str()) {
        StatementClass::Session
    } else if TRANSACTION.contains(&name.as_str()) {
        StatementClass::Transaction
    } else {
        StatementClass::Write
    };
    Classification { class, danger }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
