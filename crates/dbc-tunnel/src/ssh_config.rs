//! Reading OpenSSH's `~/.ssh/config` (deferred feature 4), so a connection's SSH settings can be
//! filled from a host alias. Only what a tunnel needs is read: `HostName`, `User`, `Port`,
//! `IdentityFile` and `ProxyJump`. As in OpenSSH, the first value found for a key wins, `Host`
//! patterns take `*`, `?` and `!negation`, and settings before the first `Host` apply to all.
//! `Match` blocks are skipped and `Include` is not followed.

/// One alias from a `Host` line, with the settings that apply to it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SshConfigHost {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// With `~` expanded.
    pub identity_file: Option<String>,
    /// As written: an alias, or `[user@]host[:port]`; only the first hop of a list.
    pub proxy_jump: Option<String>,
}

/// The hosts in `path`, with `~` expanded against `home`. A missing file has none.
pub fn read(path: &std::path::Path, home: &str) -> std::io::Result<Vec<SshConfigHost>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(parse(&text, home)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// `~/.ssh/config` of the current user, if there is a home directory.
pub fn user_config() -> std::io::Result<Vec<SshConfigHost>> {
    let Some(home) = std::env::home_dir() else {
        return Ok(Vec::new());
    };
    read(&home.join(".ssh").join("config"), &home.to_string_lossy())
}

struct Block {
    /// `None` for the settings before the first `Host`, and for `Match` (never applied).
    patterns: Option<Vec<String>>,
    applies_to_all: bool,
    settings: Vec<(String, String)>,
}

/// Every concrete alias (no wildcard, no negation) with its resolved settings, in file order.
pub fn parse(text: &str, home: &str) -> Vec<SshConfigHost> {
    let blocks = blocks(text);
    let mut aliases: Vec<String> = Vec::new();
    for block in &blocks {
        for pattern in block.patterns.iter().flatten() {
            let concrete = !pattern.contains(['*', '?', '!']);
            if concrete && !aliases.contains(pattern) {
                aliases.push(pattern.clone());
            }
        }
    }
    aliases
        .into_iter()
        .map(|alias| resolve(&blocks, alias, home))
        .collect()
}

fn blocks(text: &str) -> Vec<Block> {
    let mut blocks = vec![Block {
        patterns: None,
        applies_to_all: true,
        settings: Vec::new(),
    }];
    for line in text.lines() {
        let Some((key, value)) = key_value(line) else {
            continue;
        };
        match key.as_str() {
            "host" => blocks.push(Block {
                patterns: Some(words(&value)),
                applies_to_all: false,
                settings: Vec::new(),
            }),
            "match" => blocks.push(Block {
                patterns: None,
                applies_to_all: false,
                settings: Vec::new(),
            }),
            _ => {
                if let Some(block) = blocks.last_mut() {
                    block.settings.push((key, value));
                }
            }
        }
    }
    blocks
}

/// `Key value`, `Key=value` or `Key = value`, without comments; the key lowercased.
fn key_value(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let split = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let (key, rest) = line.split_at(split);
    let value = rest.trim_start().strip_prefix('=').unwrap_or(rest).trim();
    Some((key.to_ascii_lowercase(), value.to_owned()))
}

/// Space-separated words; `"double quoted"` words keep their spaces.
fn words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in value.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn applies(block: &Block, alias: &str) -> bool {
    let Some(patterns) = &block.patterns else {
        return block.applies_to_all;
    };
    let negated = patterns
        .iter()
        .filter_map(|p| p.strip_prefix('!'))
        .any(|p| glob(p, alias));
    !negated
        && patterns
            .iter()
            .filter(|p| !p.starts_with('!'))
            .any(|p| glob(p, alias))
}

fn resolve(blocks: &[Block], alias: String, home: &str) -> SshConfigHost {
    let mut found = SshConfigHost {
        alias,
        ..SshConfigHost::default()
    };
    for block in blocks.iter().filter(|b| applies(b, &found.alias)) {
        for (key, value) in &block.settings {
            let first = words(value).into_iter().next().unwrap_or_default();
            match key.as_str() {
                "hostname" if found.host_name.is_none() => {
                    found.host_name = Some(first.replace("%h", &found.alias));
                }
                "user" if found.user.is_none() => found.user = Some(first),
                "port" if found.port.is_none() => found.port = first.parse().ok(),
                "identityfile" if found.identity_file.is_none() => {
                    found.identity_file = Some(expand_home(&first, home));
                }
                "proxyjump" if found.proxy_jump.is_none() => {
                    let hop = first.split(',').next().unwrap_or_default().to_owned();
                    // `none` is recorded so later blocks cannot set a jump.
                    found.proxy_jump = Some(hop);
                }
                _ => {}
            }
        }
    }
    if found
        .proxy_jump
        .as_deref()
        .is_some_and(|j| j.is_empty() || j.eq_ignore_ascii_case("none"))
    {
        found.proxy_jump = None;
    }
    found
}

fn expand_home(path: &str, home: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", home.trim_end_matches('/')),
        None if path == "~" => home.to_owned(),
        None => path.to_owned(),
    }
}

/// OpenSSH patterns: `*` any run of characters, `?` exactly one.
pub(crate) fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    // Iterative matching with backtracking to the last `*`.
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ti));
                pi += 1;
            }
            Some(&c) if c == '?' || Some(&c) == t.get(ti) => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    pi = sp + 1;
                    ti = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    p[pi.min(p.len())..].iter().all(|&c| c == '*')
}

#[cfg(test)]
#[path = "ssh_config_tests.rs"]
mod tests;
