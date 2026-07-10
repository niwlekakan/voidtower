//! Redaction of secret material from any response that can reach an AI context —
//! MCP tool-call output (`api/mcp.rs`), the Studio MCP panel (`api/studio.rs`), and
//! the `get_context` bundle (`api/ai_context.rs`). Single source of truth so those
//! ingress points don't each reimplement redaction (see P0.5 in gap-analysis.md).
//!
//! Two layers, applied in order:
//! 1. `redact_known_values` — exact-match redaction of every value currently stored
//!    in the `secrets` table (decrypted). This is the primary defense: it catches a
//!    registered VoidTower secret verbatim, regardless of surrounding context.
//! 2. `redact_patterns` — conservative heuristic redaction of secret-*shaped*
//!    substrings (keyword=value pairs, PEM private-key blocks, AWS access keys,
//!    Bearer tokens) that aren't registered VoidTower secrets at all — e.g. a
//!    third-party API key baked into a managed container's startup banner.
//!    Deliberately narrow: it only fires next to a secret-indicating keyword or a
//!    well-known structural marker, so ordinary content that merely looks random
//!    (hashes, UUIDs) is left alone.

use crate::AppState;

const REDACTED: &str = "[REDACTED]";
const REDACTED_PEM: &str = "[REDACTED PEM BLOCK]";

/// Secret values shorter than this are skipped during known-value redaction — a
/// short value (e.g. a placeholder "x" someone stored) would otherwise blow away
/// unrelated short substrings throughout the response.
const MIN_KNOWN_VALUE_LEN: usize = 6;

const SECRET_KEYWORDS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "api_key",
    "apikey",
    "api-key",
    "secret_key",
    "secretkey",
    "secret-key",
    "secret",
    "access_key",
    "accesskey",
    "access-key",
    "private_key",
    "privatekey",
    "private-key",
    "auth_token",
    "authtoken",
    "token",
    "credential",
];

/// Fetch and decrypt every stored secret's value, for use as a known-value
/// redaction set. Decryption failures are skipped rather than surfaced — redaction
/// must never fail (or panic) the response it's protecting.
pub async fn known_secret_values(state: &AppState) -> Vec<String> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT value_enc FROM secrets")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    rows.into_iter()
        .filter_map(|(enc,)| crate::api::secrets::decrypt(&state.secrets_key, &enc).ok())
        .collect()
}

/// Replace every verbatim occurrence of a known secret value with `[REDACTED]`.
pub fn redact_known_values(text: &str, known_values: &[String]) -> String {
    let mut out = text.to_string();
    for value in known_values {
        if value.len() < MIN_KNOWN_VALUE_LEN {
            continue;
        }
        if out.contains(value.as_str()) {
            out = out.replace(value.as_str(), REDACTED);
        }
    }
    out
}

/// Full redaction pipeline: known values first, then pattern heuristics.
pub fn redact(text: &str, known_values: &[String]) -> String {
    redact_patterns(&redact_known_values(text, known_values))
}

/// Convenience wrapper for call sites that only have `state` and the raw text —
/// looks up the current known-secret-value set and applies the full pipeline.
pub async fn redact_for_ai(state: &AppState, text: &str) -> String {
    let known = known_secret_values(state).await;
    redact(text, &known)
}

/// Heuristic redaction of secret-shaped substrings (see module docs).
pub fn redact_patterns(text: &str) -> String {
    let text = redact_pem_blocks(text);
    let text = redact_keyword_values(&text);
    let text = redact_bearer_tokens(&text);
    redact_aws_access_keys(&text)
}

/// Redacts a PEM private-key block as a whole span, from its `-----BEGIN ...
/// PRIVATE KEY-----` header through the matching `-----END ... PRIVATE
/// KEY-----` footer. Deliberately marker-based rather than line-based: the text
/// handed to us is frequently already-serialized JSON (every mcp.rs tool
/// returns `serde_json::to_string(...)` before redaction runs), where the
/// block's real newlines have become the two-byte escape sequence `\n` rather
/// than an actual line break. Scanning for the markers as plain substrings
/// works the same regardless of how — or whether — the block is line-broken.
fn redact_pem_blocks(text: &str) -> String {
    const OPEN: &str = "-----BEGIN";
    const CLOSE_TAG: &str = "PRIVATE KEY-----";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(begin_idx) = rest.find(OPEN) {
        let after_begin = &rest[begin_idx..];
        // Confirm this BEGIN header names a private key, within a short window
        // (a real PEM header line is short).
        // Bound by match position rather than pre-slicing `after_begin`, so we
        // never risk cutting a multi-byte UTF-8 char mid-sequence.
        let header_close = match after_begin.find(CLOSE_TAG) {
            Some(p) if p < 48 => p + CLOSE_TAG.len(),
            _ => {
                // Not a private-key header; emit through "-----BEGIN" and keep scanning.
                out.push_str(&rest[..begin_idx + OPEN.len()]);
                rest = &rest[begin_idx + OPEN.len()..];
                continue;
            }
        };
        let after_header = &after_begin[header_close..];
        match after_header.find("-----END") {
            Some(end_rel) => {
                let after_end_marker = &after_header[end_rel..];
                let footer_close = after_end_marker
                    .find(CLOSE_TAG)
                    .map(|p| p + CLOSE_TAG.len())
                    .unwrap_or_else(|| "-----END".len());
                out.push_str(&rest[..begin_idx]);
                out.push_str(REDACTED_PEM);
                rest = &after_end_marker[footer_close..];
            }
            None => {
                // Unterminated block — redact through the end of the text.
                out.push_str(&rest[..begin_idx]);
                out.push_str(REDACTED_PEM);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Finds `<keyword><optional space>[=:]<optional space/quote><value>` and redacts
/// just the value, preserving the keyword so the surrounding text stays readable.
/// Requires a word boundary before the keyword and an explicit `=`/`:` separator
/// right after it (mod whitespace) — this is what keeps it from firing on plain
/// English sentences that merely contain the word "token" or "secret".
fn redact_keyword_values(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;

    'outer: while idx < text.len() {
        for kw in SECRET_KEYWORDS {
            // `.get()` rather than direct slicing: `idx + kw.len()` isn't
            // necessarily a char boundary when `text` contains multi-byte
            // UTF-8 (e.g. an em dash) near a would-be keyword position —
            // `.get()` returns `None` there instead of panicking.
            let matches = text
                .get(idx..idx + kw.len())
                .is_some_and(|w| w.eq_ignore_ascii_case(kw));
            if matches {
                let prev_ok = idx == 0 || !(bytes[idx - 1] as char).is_ascii_alphanumeric();
                if !prev_ok {
                    continue;
                }
                let mut p = idx + kw.len();
                while p < text.len() && (bytes[p] as char).is_whitespace() {
                    p += 1;
                }
                if p < text.len() && (bytes[p] == b'=' || bytes[p] == b':') {
                    p += 1;
                    // Skip whitespace and opening quotes. `\` is included because
                    // the text handed to us is frequently already-serialized JSON
                    // (see `redact_pem_blocks`'s doc comment) — there a literal
                    // `"` around the value shows up as the two-byte escape `\"`.
                    while p < text.len()
                        && ((bytes[p] as char).is_whitespace()
                            || bytes[p] == b'"'
                            || bytes[p] == b'\''
                            || bytes[p] == b'\\')
                    {
                        p += 1;
                    }
                    let value_start = p;
                    while p < text.len()
                        && !(bytes[p] as char).is_whitespace()
                        && bytes[p] != b'"'
                        && bytes[p] != b'\''
                        && bytes[p] != b','
                        && bytes[p] != b'\\'
                    {
                        p += 1;
                    }
                    if p - value_start >= 4 {
                        out.push_str(&text[idx..value_start]);
                        out.push_str(REDACTED);
                        idx = p;
                        continue 'outer;
                    }
                }
            }
        }
        let ch = text[idx..].chars().next().expect("idx < text.len()");
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

/// Redacts `Bearer <token>` (case-insensitive on the word "Bearer").
fn redact_bearer_tokens(text: &str) -> String {
    const MARKER: &str = "bearer ";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;

    while idx < text.len() {
        let remaining = &text[idx..];
        // `.get()` rather than direct slicing — see the identical comment in
        // `redact_keyword_values` for why a fixed-length slice here can't
        // assume `MARKER.len()` bytes ahead is a char boundary.
        let matches_marker = remaining
            .get(..MARKER.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(MARKER));
        if matches_marker {
            let prev_ok = idx == 0 || !(bytes[idx - 1] as char).is_ascii_alphanumeric();
            if prev_ok {
                let value_start = idx + MARKER.len();
                let mut p = value_start;
                while p < text.len() && !(bytes[p] as char).is_whitespace() {
                    p += 1;
                }
                if p > value_start {
                    out.push_str(&text[idx..value_start]);
                    out.push_str(REDACTED);
                    idx = p;
                    continue;
                }
            }
        }
        let ch = remaining.chars().next().expect("idx < text.len()");
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

/// Redacts AWS-style access key IDs: `AKIA` followed by 16 uppercase alnum chars.
fn redact_aws_access_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;
    while idx < text.len() {
        let remaining = &text[idx..];
        if let Some(after_prefix) = remaining.strip_prefix("AKIA") {
            let tail: String = after_prefix
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if tail.len() >= 16 {
                out.push_str(REDACTED);
                idx += 4 + tail.len();
                continue;
            }
        }
        let ch = remaining.chars().next().expect("idx < text.len()");
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_keyword_value_pairs() {
        let out = redact_patterns(r#"api_key=fakevendor_abcdef1234567890 startup ok"#);
        assert!(!out.contains("fakevendor_abcdef1234567890"));
        assert!(out.contains("api_key=[REDACTED]"));
    }

    #[test]
    fn redacts_password_with_colon_and_quotes() {
        let out = redact_patterns(r#"config: password: "hunter2reallylongpassword""#);
        assert!(!out.contains("hunter2reallylongpassword"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_pem_private_key_block() {
        let text = "before\n-----BEGIN TEST PRIVATE KEY-----\nMIIBogIBAAKCAQEA...\nmore-fake-key-bytes\n-----END TEST PRIVATE KEY-----\nafter";
        let out = redact_patterns(text);
        assert!(!out.contains("MIIBogIBAAKCAQEA"));
        assert!(out.contains(REDACTED_PEM));
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn redacts_bearer_tokens() {
        let out = redact_patterns("Authorization: Bearer eyJabc123.def456.ghi789 sent");
        assert!(!out.contains("eyJabc123.def456.ghi789"));
        assert!(out.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_aws_access_keys() {
        let out = redact_patterns("found AKIAABCDEFGHIJKLMNOP in env");
        assert!(!out.contains("AKIAABCDEFGHIJKLMNOP"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redaction_does_not_break_non_secret_content() {
        // A git-style commit hash / long hex value with no secret-indicating
        // keyword nearby must survive untouched.
        let text = "commit 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08\nDeploy succeeded for service web-01. This is a secret feature we shipped quietly.";
        let out = redact_patterns(text);
        assert_eq!(out, text);
    }

    #[test]
    fn does_not_redact_short_keyword_matches_without_separator() {
        let text = "tokenizer_version = 3\nbroken_pipe detected\naccess denied";
        let out = redact_patterns(text);
        assert_eq!(out, text);
    }

    #[test]
    fn redact_known_values_matches_exact_secret_regardless_of_context() {
        let known = vec!["s3cr3t-db-connection-string-xyz".to_string()];
        let text = "conn=postgres://user:s3cr3t-db-connection-string-xyz@host/db";
        let out = redact_known_values(text, &known);
        assert!(!out.contains("s3cr3t-db-connection-string-xyz"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redact_known_values_skips_trivially_short_values() {
        let known = vec!["ab".to_string()];
        let text = "the value ab appears here and there, ab, ab";
        let out = redact_known_values(text, &known);
        assert_eq!(out, text);
    }

    #[tokio::test]
    async fn known_secret_values_decrypts_stored_secrets() {
        let pool = crate::api::mcp::test_support::setup_db().await;

        let key: [u8; 32] = [9u8; 32];
        let secret_value = "super-secret-registered-value-001";
        let enc = crate::api::secrets::encrypt(&key, secret_value).unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) VALUES ('s1', 'test', NULL, ?, 0, 0)",
        )
        .bind(&enc)
        .execute(&pool)
        .await
        .unwrap();

        let mut state = crate::api::mcp::test_support::build(pool);
        // Overwrite the key so it matches what we encrypted with above.
        state.secrets_key = std::sync::Arc::new(key);

        let values = known_secret_values(&state).await;
        assert_eq!(values, vec![secret_value.to_string()]);

        let text = format!("container env: DB_PASSWORD={secret_value}");
        let redacted = redact_for_ai(&state, &text).await;
        assert!(!redacted.contains(secret_value));
    }
}
