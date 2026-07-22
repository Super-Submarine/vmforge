//! Redaction guardrails for diagnostics output.
//!
//! `vmforge diagnose` must never emit secret material. Everything that goes
//! into a bundle — config files, log excerpts, command output — passes through
//! [`redact_text`] first. The rules are deliberately aggressive: a redacted
//! value in a bug report is a minor annoyance; a leaked credential is not.
//!
//! Rules (see `docs/diagnose.md` for the user-facing contract):
//! 1. Lines assigning to a sensitive key (`password`, `secret`, `token`,
//!    `api_key`, `passphrase`, `private_key`, `access_key`, `credential`,
//!    `authorization`, `cookie`, `session_id`, ...) have the value replaced.
//! 2. `Bearer`/`Basic` authorization values are replaced wherever they appear.
//! 3. PEM blocks (`-----BEGIN ... PRIVATE KEY-----` etc.) are dropped whole.
//! 4. Long high-entropy tokens (40+ chars of base64/hex alphabet) are
//!    replaced even without a recognizable key name.

const REDACTED: &str = "[REDACTED]";

/// Key names (matched case-insensitively as substrings of the key part of a
/// `key = value` / `key: value` / `key=value` assignment) that mark a value
/// as secret.
const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "private_key",
    "private-key",
    "privatekey",
    "access_key",
    "access-key",
    "accesskey",
    "credential",
    "authorization",
    "auth_header",
    "cookie",
    "session_id",
    "session-id",
    "client_secret",
    "signing_key",
];

/// Redact a whole multi-line text, including PEM block removal.
pub fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_pem = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if in_pem {
            if trimmed.starts_with("-----END") {
                in_pem = false;
            }
            continue;
        }
        if trimmed.starts_with("-----BEGIN") {
            in_pem = true;
            out.push_str(REDACTED);
            out.push_str(" (PEM block removed)\n");
            continue;
        }
        out.push_str(&redact_line(line));
        out.push('\n');
    }
    out
}

/// Redact a single line.
pub fn redact_line(line: &str) -> String {
    let line = redact_key_value(line);
    let line = redact_auth_schemes(&line);
    redact_long_tokens(&line)
}

/// Rule 1: `key = value` assignments where the key looks sensitive.
fn redact_key_value(line: &str) -> String {
    // Find the first '=' or ':' separator; inspect the key part before it.
    for (i, ch) in line.char_indices() {
        if ch == '=' || ch == ':' {
            let key = line[..i].to_ascii_lowercase();
            let key = key.trim().trim_matches('"').trim_matches('\'');
            // Only look at the last word of the key part so prose like
            // "note: the password field" is not treated as an assignment.
            let last_word = key
                .rsplit(|c: char| c.is_whitespace() || c == ',' || c == '{')
                .next()
                .unwrap_or(key);
            if SENSITIVE_KEYS.iter().any(|k| last_word.contains(k)) {
                return format!("{}{} {}", &line[..i], ch, REDACTED);
            }
            break;
        }
    }
    line.to_string()
}

/// Rule 2: `Bearer <...>` / `Basic <...>` credentials anywhere in a line.
fn redact_auth_schemes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        let lower = rest.to_ascii_lowercase();
        let hit = ["bearer ", "basic "]
            .iter()
            .filter_map(|s| lower.find(s).map(|i| (i, s.len())))
            .min();
        match hit {
            Some((i, kw_len)) => {
                let value_start = i + kw_len;
                let value_end = rest[value_start..]
                    .find(char::is_whitespace)
                    .map(|j| value_start + j)
                    .unwrap_or(rest.len());
                // Only treat as a credential if the value looks like one.
                if value_end - value_start >= 8 {
                    out.push_str(&rest[..value_start]);
                    out.push_str(REDACTED);
                } else {
                    out.push_str(&rest[..value_end]);
                }
                rest = &rest[value_end..];
            }
            None => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

/// Rule 4: bare high-entropy tokens (40+ chars of the base64/hex alphabet
/// with at least one digit) are redacted even without a key name.
fn redact_long_tokens(line: &str) -> String {
    let mut out = String::new();
    for (i, word) in line.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let core = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let is_tokenish = core.len() >= 40
            && core
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '_' || c == '-')
            && core.chars().any(|c| c.is_ascii_digit())
            && core.chars().any(|c| c.is_ascii_alphabetic());
        if is_tokenish {
            out.push_str(&word.replace(core, REDACTED));
        } else {
            out.push_str(word);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_assignments() {
        for line in [
            "password = hunter2",
            "PASSWORD=hunter2",
            "api_key: sk-abc123",
            "\"client_secret\": \"xyz\"",
            "  ssh_private_key = /home/u/.ssh/id_rsa_contents",
            "AUTHORIZATION: something",
        ] {
            let r = redact_line(line);
            assert!(r.contains("[REDACTED]"), "not redacted: {line} -> {r}");
            assert!(!r.contains("hunter2") && !r.contains("sk-abc123") && !r.contains("xyz\""));
        }
    }

    #[test]
    fn keeps_ordinary_lines() {
        for line in [
            "memory_mib = 2048",
            "disks = [\"disk0.qcow2\"]",
            "2026-07-22T10:00:01Z qemu-system-x86_64 started pid=4242",
            "note: enter your password when prompted",
            "kvm:writable",
        ] {
            assert_eq!(redact_line(line), line, "over-redacted: {line}");
        }
    }

    #[test]
    fn redacts_bearer_tokens() {
        let r = redact_line("curl -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload'");
        assert!(!r.contains("eyJhbGci"));
        assert!(r.contains("[REDACTED]"));
    }

    #[test]
    fn short_bearer_words_survive() {
        assert_eq!(
            redact_line("the bearer of bad news"),
            "the bearer of bad news"
        );
    }

    #[test]
    fn drops_pem_blocks() {
        let text = "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----\nafter\n";
        let r = redact_text(text);
        assert!(!r.contains("MIIEowIBAAKCAQEA"));
        assert!(r.contains("before"));
        assert!(r.contains("after"));
        assert!(r.contains("PEM block removed"));
    }

    #[test]
    fn redacts_long_bare_tokens() {
        let tok = "A1b2C3d4E5f6G7h8I9j0A1b2C3d4E5f6G7h8I9j0";
        let r = redact_line(&format!("saw token {tok} in log"));
        assert!(!r.contains(tok));
        // But long words without digits (e.g. paths, prose) survive.
        let word = "aVeryLongWordWithoutAnyDigitsInItAtAllOkay";
        assert_eq!(redact_line(word), word);
    }
}
