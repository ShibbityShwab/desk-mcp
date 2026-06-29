//! Bearer token authentication for HTTP transport.
//!
//! On first start without `$DESK_MCP_TOKEN`, generates a random 32-char token,
//! saves it to `~/.config/desk-mcp/token`, and prints it to stderr.
//! All subsequent HTTP requests must include the token via:
//!   - `Authorization: Bearer <token>` header, or
//!   - `?token=<token>` query parameter.
//!     Requests without a valid token receive HTTP 401.

use std::path::PathBuf;
use std::sync::OnceLock;

static TOKEN: OnceLock<String> = OnceLock::new();

/// Get or generate the auth token.
///
/// Returns the token string. On first call, reads from
/// `~/.config/desk-mcp/token`; if not found, generates a new one,
/// saves it, and prints it to stderr.
pub fn get_token() -> &'static str {
    TOKEN.get_or_init(|| {
        let path = token_path();

        // Read existing token
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let t = existing.trim().to_string();
            if t.len() >= 16 {
                tracing::info!("loaded auth token from {}", path.display());
                return t;
            }
        }

        // Generate new token
        let token = format!("dmcp_{}", generate_random(24));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, &token) {
            tracing::warn!("failed to save token to {}: {e}", path.display());
        }
        // Print to stderr so the user/LLM can copy it
        eprintln!(
            "[desk-mcp] 🔑 New auth token saved to {}: {}",
            path.display(),
            token
        );
        eprintln!("[desk-mcp] Include it in requests: Authorization: Bearer {token}");

        token
    })
}

/// Validate a request against the stored token.
///
/// `bearer` is the value of the `Authorization: Bearer <...>` header
/// (or `?token=<...>` query parameter).
pub fn validate(bearer: Option<&str>) -> bool {
    let expected = get_token();
    matches!(bearer, Some(provided) if provided == expected)
}

/// Extract the bearer token from an Authorization header value.
pub fn from_header(value: &str) -> Option<&str> {
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
}

fn token_path() -> PathBuf {
    let mut p = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("desk-mcp");
    p.push("token");
    p
}

/// Generate a random alphanumeric string of `len` characters.
fn generate_random(len: usize) -> String {
    let mut s = String::with_capacity(len);
    let mut buf = [0u8; 1];
    while s.len() < len {
        // Use /dev/urandom for simple randomness
        if std::fs::File::open("/dev/urandom")
            .and_then(|mut f| std::io::Read::read(&mut f, &mut buf))
            .is_ok()
        {
            let idx = (buf[0] as usize) % 62;
            let c = if idx < 10 {
                (b'0' + idx as u8) as char
            } else if idx < 36 {
                (b'a' + (idx - 10) as u8) as char
            } else {
                (b'A' + (idx - 36) as u8) as char
            };
            s.push(c);
        } else {
            // Fallback: use time-based
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            s.push(((t % 62) as u8 + b'a') as char);
        }
    }
    s
}
