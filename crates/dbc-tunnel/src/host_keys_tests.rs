use rstest::rstest;

use super::*;

const KEY_A: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMIOC5tq5up0pZsINADAvPX6yNu+oa+LTUZ4cm62tNk1";
const KEY_B: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJLlTZ0fqz9Deq7WI3tac8+NXeUnUqvc1f5QZ6miOspM";
const KEY_C: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBLwf1TlFimCm6YqRBKpgSdA29nKJY7tLkieMjsimHx7FMJt4hptOOBPPoZY1mnkVzz0nbqyLqdOR630bs+w44MU=";

/// KEY_A for a plain host, a host on port 2222, and a hashed entry for `hashed.example.com`.
const KNOWN_HOSTS: &str = "# a comment
plain.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMIOC5tq5up0pZsINADAvPX6yNu+oa+LTUZ4cm62tNk1
[ported.example.com]:2222 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMIOC5tq5up0pZsINADAvPX6yNu+oa+LTUZ4cm62tNk1
|1|t/k4Hg2CSL9uTvKm3sdfJZMiul8=|pvUTQg5rEJRWIZYjrZTvUhzeO4o= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMIOC5tq5up0pZsINADAvPX6yNu+oa+LTUZ4cm62tNk1
";

fn key(openssh: &str) -> PublicKey {
    PublicKey::from_openssh(openssh).unwrap()
}

fn known_hosts() -> (tempfile::TempDir, KnownHostsFile) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    std::fs::write(&path, KNOWN_HOSTS).unwrap();
    (dir, KnownHostsFile::new(path))
}

#[rstest]
#[case::plain("plain.example.com", 22, KEY_A, HostKeyStatus::Trusted)]
#[case::port("ported.example.com", 2222, KEY_A, HostKeyStatus::Trusted)]
#[case::hashed("hashed.example.com", 22, KEY_A, HostKeyStatus::Trusted)]
#[case::changed("plain.example.com", 22, KEY_B, HostKeyStatus::Changed)]
#[case::changed_hashed("hashed.example.com", 22, KEY_B, HostKeyStatus::Changed)]
#[case::other_algorithm("plain.example.com", 22, KEY_C, HostKeyStatus::Unknown)]
#[case::other_port("plain.example.com", 2222, KEY_A, HostKeyStatus::Unknown)]
#[case::unknown_host("other.example.com", 22, KEY_A, HostKeyStatus::Unknown)]
fn known_hosts_file_checks_keys(
    #[case] host: &str,
    #[case] port: u16,
    #[case] presented: &str,
    #[case] expected: HostKeyStatus,
) {
    let (_dir, file) = known_hosts();

    assert_eq!(file.check(host, port, &key(presented)), expected);
}

#[test]
fn a_missing_file_knows_no_hosts() {
    let file = KnownHostsFile::new("/nonexistent/known_hosts");

    assert_eq!(
        file.check("plain.example.com", 22, &key(KEY_A)),
        HostKeyStatus::Unknown
    );
}

fn trusted(host: &str, port: u16, openssh: &str) -> TrustedKey {
    let key = key(openssh);
    TrustedKey {
        host: host.into(),
        port,
        algorithm: algorithm(&key),
        fingerprint: fingerprint(&key),
    }
}

#[test]
fn fingerprint_list_compares_per_algorithm() {
    let list = FingerprintList {
        entries: vec![trusted("db.example.com", 22, KEY_A)],
    };

    assert_eq!(
        list.check("DB.example.com", 22, &key(KEY_A)),
        HostKeyStatus::Trusted
    );
    assert_eq!(
        list.check("db.example.com", 22, &key(KEY_B)),
        HostKeyStatus::Changed
    );
    assert_eq!(
        list.check("db.example.com", 22, &key(KEY_C)),
        HostKeyStatus::Unknown
    );
    assert_eq!(
        list.check("db.example.com", 2222, &key(KEY_A)),
        HostKeyStatus::Unknown
    );
}

#[test]
fn composite_prefers_changed_over_trusted_over_unknown() {
    let (_dir, file) = known_hosts();
    let app = FingerprintList {
        entries: vec![trusted("plain.example.com", 22, KEY_B)],
    };
    let composite = CompositeHostKeyStore::new(vec![Box::new(file), Box::new(app)]);

    // The file trusts KEY_A, the app table has KEY_B for the same host: a conflict is refused.
    assert_eq!(
        composite.check("plain.example.com", 22, &key(KEY_A)),
        HostKeyStatus::Changed
    );
    assert_eq!(
        composite.check("hashed.example.com", 22, &key(KEY_A)),
        HostKeyStatus::Trusted
    );
    assert_eq!(
        composite.check("new.example.com", 22, &key(KEY_A)),
        HostKeyStatus::Unknown
    );
}

#[test]
fn fingerprints_look_like_ssh_keygen_output() {
    let printed = fingerprint(&key(KEY_A));

    assert!(printed.starts_with("SHA256:"), "{printed}");
    assert_eq!(algorithm(&key(KEY_A)), "ssh-ed25519");
}
