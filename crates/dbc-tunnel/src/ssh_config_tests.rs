use super::*;

const CONFIG: &str = r#"
# defaults for everyone
ServerAliveInterval 30

Host bastion
    HostName bastion.example.com
    User ops
    Port 2222
    IdentityFile ~/.ssh/ops_ed25519

Host db-* !db-legacy
    User dbadmin
    ProxyJump bastion

Host db-prod db-staging
    HostName=%h.internal
    IdentityFile "~/.ssh/my key"

Host db-legacy
    HostName 10.0.0.9

Match host *.example.com
    User ignored

Host *
    User fallback
    IdentityFile ~/.ssh/id_rsa
"#;

fn host<'a>(hosts: &'a [SshConfigHost], alias: &str) -> &'a SshConfigHost {
    hosts.iter().find(|h| h.alias == alias).unwrap()
}

#[test]
fn lists_concrete_aliases_in_file_order() {
    let hosts = parse(CONFIG, "/home/me");

    let aliases: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
    assert_eq!(aliases, ["bastion", "db-prod", "db-staging", "db-legacy"]);
}

#[test]
fn the_first_value_wins_across_matching_blocks() {
    let hosts = parse(CONFIG, "/home/me");

    let bastion = host(&hosts, "bastion");
    assert_eq!(bastion.host_name.as_deref(), Some("bastion.example.com"));
    assert_eq!(bastion.user.as_deref(), Some("ops"));
    assert_eq!(bastion.port, Some(2222));
    assert_eq!(
        bastion.identity_file.as_deref(),
        Some("/home/me/.ssh/ops_ed25519")
    );
    assert_eq!(bastion.proxy_jump, None);

    let prod = host(&hosts, "db-prod");
    assert_eq!(prod.user.as_deref(), Some("dbadmin"));
    assert_eq!(prod.proxy_jump.as_deref(), Some("bastion"));
    assert_eq!(prod.host_name.as_deref(), Some("db-prod.internal"));
    assert_eq!(prod.identity_file.as_deref(), Some("/home/me/.ssh/my key"));
}

#[test]
fn negated_patterns_exclude_and_wildcards_fill_the_rest() {
    let hosts = parse(CONFIG, "/home/me");

    let legacy = host(&hosts, "db-legacy");
    assert_eq!(legacy.user.as_deref(), Some("fallback"));
    assert_eq!(legacy.proxy_jump, None);
    assert_eq!(
        legacy.identity_file.as_deref(),
        Some("/home/me/.ssh/id_rsa")
    );
}

#[test]
fn proxy_jump_none_means_no_jump() {
    let hosts = parse("Host a\n  ProxyJump none\nHost *\n  ProxyJump b\n", "/h");

    assert_eq!(host(&hosts, "a").proxy_jump, None);
}

#[test]
fn glob_matching() {
    assert!(glob("db-*", "db-prod"));
    assert!(glob("db-?", "db-1"));
    assert!(!glob("db-?", "db-12"));
    assert!(glob("*", ""));
    assert!(glob("*.example.com", "a.b.example.com"));
    assert!(!glob("*.example.com", "example.com"));
}

proptest::proptest! {
    #[test]
    fn no_text_panics_the_parser(text in "[ \\t\\n=#\"*?!a-zA-Z~%/.0-9-]{0,200}") {
        let _ = parse(&text, "/h");
    }
}

#[test]
fn a_missing_file_has_no_hosts_and_a_present_one_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config");

    assert_eq!(read(&path, "/h").unwrap(), Vec::new());
    std::fs::write(&path, "Host x\n  User u\n").unwrap();
    assert_eq!(read(&path, "/h").unwrap()[0].user.as_deref(), Some("u"));
}
