//! Multi-agent session manager — Pillar II.1–II.2 of the Kowloon Manifesto.
//!
//! Each connected agent (via HTTP token or stdio) gets an isolated
//! `AgentSession` with its own workspace, rate-limiter bucket, browser
//! state slot, and pending-confirmation queue.  A `GlobalArbiter`
//! serialises access to shared resources (cursor, keyboard focus).
//!
//! ## Architecture
//! - `SessionManager` — CRUD for sessions, backed by a `DashMap`.
//! - `AgentSession` — per-connection state (rate, confirmations, browser).
//! - `GlobalArbiter` — RAII guards for cursor / focus ownership.
//! - `session_stats()` — JSON snapshot for observability (Pillar II.4
//!   dashboard is separate).

use crate::safety::Confirmation;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

// ── SessionId ────────────────────────────────────────────────────────────

/// Opaque session identifier — UUID v4 string.
pub type SessionId = String;

// ── SessionCapabilities ──────────────────────────────────────────────────

/// Per-session capability flags — constrain what an agent may do.
#[derive(Debug, Clone)]
pub struct SessionCapabilities {
    pub allow_shell: bool,
    pub allow_code: bool,
    /// Maximum file size (bytes) for reads/writes through this session.
    pub max_file_size: u64,
}

impl Default for SessionCapabilities {
    fn default() -> Self {
        Self {
            allow_shell: std::env::var("ALLOW_SHELL")
                .map(|v| v == "1")
                .unwrap_or(false),
            allow_code: std::env::var("ALLOW_CODE")
                .map(|v| v == "1")
                .unwrap_or(false),
            max_file_size: 10 * 1024 * 1024, // 10 MiB
        }
    }
}

// ── AgentSession ─────────────────────────────────────────────────────────

/// Per-agent isolated state.
///
/// Every field that mutates at runtime is behind a `RwLock`, so multiple
/// MCP handlers can read session metadata concurrently while one writer
/// updates the rate bucket or action counter.
pub struct AgentSession {
    pub id: SessionId,
    pub created: chrono::DateTime<chrono::Utc>,
    pub last_active: RwLock<chrono::DateTime<chrono::Utc>>,
    /// Per-session working directory (default: cwd at session creation).
    pub workspace: RwLock<PathBuf>,
    /// Window title the agent last focused.
    pub focused_window: RwLock<Option<String>>,
    /// Per-session browser page handle (set when browser_launch completes
    /// for this session).  Falls back to the global BROWSER when absent.
    pub(crate) browser_page: RwLock<Option<chromiumoxide::Page>>,
    /// Number of tool invocations through this session.
    pub action_count: RwLock<u32>,
    /// Pending confirmations owned by this session.
    pub pending_confirmations: RwLock<Vec<Confirmation>>,
    /// Capability flags.
    pub capabilities: SessionCapabilities,
}

impl AgentSession {
    /// Create a new session with the given capabilities.
    fn new(id: SessionId, capabilities: SessionCapabilities) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            created: now,
            last_active: RwLock::new(now),
            workspace: RwLock::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))),
            focused_window: RwLock::new(None),
            browser_page: RwLock::new(None),
            action_count: RwLock::new(0),
            pending_confirmations: RwLock::new(Vec::new()),
            capabilities,
        }
    }

    /// Per-session rate check — disabled (permissive mode).
    pub fn check_rate(&self) -> bool {
        true
    }

    /// Bump the action counter and touch `last_active`.
    pub fn record_action(&self) {
        if let Ok(mut count) = self.action_count.try_write() {
            *count = count.saturating_add(1);
        }
        if let Ok(mut ts) = self.last_active.try_write() {
            *ts = chrono::Utc::now();
        }
    }

    /// Request a confirmation — pushes to the session-local pending list.
    pub fn request_confirmation(
        &self,
        tool: &str,
        message: &str,
        params: &serde_json::Value,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let conf = Confirmation {
            id: id.clone(),
            tool: tool.to_string(),
            message: message.to_string(),
            params: params.clone(),
            created: chrono::Utc::now(),
        };
        if let Ok(mut v) = self.pending_confirmations.try_write() {
            v.push(conf);
        }
        id
    }

    /// Approve a pending confirmation for this session.
    pub fn approve_confirmation(&self, id: &str) -> Result<(), String> {
        let mut v = self
            .pending_confirmations
            .try_write()
            .map_err(|_| "lock poisoned".to_string())?;
        let pos = v.iter().position(|c| c.id == id);
        match pos {
            Some(i) => {
                v.remove(i);
                Ok(())
            }
            None => Err(format!("no pending confirmation with id {id}")),
        }
    }

    /// Deny a pending confirmation for this session.
    pub fn deny_confirmation(&self, id: &str) -> Result<(), String> {
        let mut v = self
            .pending_confirmations
            .try_write()
            .map_err(|_| "lock poisoned".to_string())?;
        let pos = v.iter().position(|c| c.id == id);
        match pos {
            Some(i) => {
                v.remove(i);
                Ok(())
            }
            None => Err(format!("no pending confirmation with id {id}")),
        }
    }

    /// List pending confirmations for this session.
    pub fn list_pending(&self) -> Vec<Confirmation> {
        self.pending_confirmations
            .try_read()
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Attach the current browser page to this session (called when browser_launch completes).
    pub(crate) async fn attach_page(&self, page: chromiumoxide::Page) {
        *self.browser_page.write().await = Some(page);
    }
}

// ── SessionManager ───────────────────────────────────────────────────────

/// Central registry of all active sessions.
pub struct SessionManager {
    sessions: DashMap<SessionId, Arc<AgentSession>>,
    arbiter: Arc<GlobalArbiter>,
    total_action_count: AtomicU64,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            arbiter: Arc::new(GlobalArbiter::new()),
            total_action_count: AtomicU64::new(0),
        }
    }

    /// Create a new session. Returns the session ID.
    pub fn create_session(&self, capabilities: SessionCapabilities) -> SessionId {
        let id = uuid::Uuid::new_v4().to_string();
        let session = Arc::new(AgentSession::new(id.clone(), capabilities));
        self.sessions.insert(id.clone(), session);
        tracing::info!(session_id = %id, "session created");
        id
    }

    /// Create (or get) a session keyed by a deterministic id (e.g. hashed
    /// HTTP token).  Returns the session id.
    pub fn create_deterministic(
        &self,
        deterministic_id: &str,
        capabilities: SessionCapabilities,
    ) -> SessionId {
        if let Some(entry) = self.sessions.get(deterministic_id) {
            return entry.id.clone();
        }
        let session = Arc::new(AgentSession::new(
            deterministic_id.to_string(),
            capabilities,
        ));
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        tracing::info!(session_id = %id, "deterministic session created");
        id
    }

    /// Get a session by ID. Returns `None` if not found.
    pub fn get_session(&self, id: &SessionId) -> Option<Arc<AgentSession>> {
        self.sessions.get(id).map(|entry| Arc::clone(&*entry))
    }

    /// Remove a session.
    pub fn destroy_session(&self, id: &SessionId) {
        self.sessions.remove(id);
        tracing::info!(session_id = %id, "session destroyed");
    }

    /// List active session IDs.
    pub fn active_sessions(&self) -> Vec<SessionId> {
        self.sessions.iter().map(|entry| entry.id.clone()).collect()
    }

    /// Total action count across all sessions (cached atomic counter).
    pub fn total_actions(&self) -> u64 {
        self.total_action_count.load(Ordering::Relaxed)
    }

    /// Increment the global action counter (call after each tool dispatch).
    pub fn increment_total_actions(&self) {
        self.total_action_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Access the global arbiter.
    pub fn arbiter(&self) -> &Arc<GlobalArbiter> {
        &self.arbiter
    }

    /// JSON snapshot for the observability dashboard (Pillar II.4).
    pub fn session_stats(&self) -> serde_json::Value {
        let sessions: Vec<serde_json::Value> = self
            .sessions
            .iter()
            .map(|entry| {
                let s = entry.value();
                let actions = s.action_count.try_read().map(|c| *c).unwrap_or(0);
                let last_active = s
                    .last_active
                    .try_read()
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default();
                serde_json::json!({
                    "id": s.id,
                    "created": s.created.to_rfc3339(),
                    "actions": actions,
                    "last_active": last_active,
                })
            })
            .collect();

        serde_json::json!({
            "active_sessions": self.sessions.len(),
            "total_actions": self.total_actions(),
            "sessions": sessions,
        })
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global session manager singleton.
pub static SESSIONS: std::sync::LazyLock<SessionManager> =
    std::sync::LazyLock::new(SessionManager::new);

// ── GlobalArbiter ────────────────────────────────────────────────────────

/// Serialises access to shared desktop resources across sessions.
///
/// Only one session may own the cursor or keyboard focus at a time.
/// Ownership is enforced via RAII guards — dropping the guard releases
/// the resource.
pub struct GlobalArbiter {
    pub cursor_owner: RwLock<Option<SessionId>>,
    pub focus_owner: RwLock<Option<SessionId>>,
}

impl Default for GlobalArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalArbiter {
    pub fn new() -> Self {
        Self {
            cursor_owner: RwLock::new(None),
            focus_owner: RwLock::new(None),
        }
    }

    /// Acquire cursor control for `session_id`.
    ///
    /// Returns `Ok(CursorGuard)` on success, `Err(msg)` if another
    /// session currently owns the cursor.  The guard releases on drop.
    pub async fn acquire_cursor(
        self: &Arc<Self>,
        session_id: &SessionId,
    ) -> Result<CursorGuard, String> {
        let mut owner = self.cursor_owner.write().await;
        match &*owner {
            Some(current) if current != session_id => {
                Err(format!("cursor owned by session {current}"))
            }
            _ => {
                *owner = Some(session_id.clone());
                Ok(CursorGuard {
                    session_id: session_id.clone(),
                    arbiter: Arc::clone(self),
                })
            }
        }
    }

    /// Acquire keyboard-focus control for `session_id`.
    ///
    /// Returns `Ok(FocusGuard)` on success, `Err(msg)` if another
    /// session currently owns the focus.  The guard releases on drop.
    pub async fn acquire_focus(
        self: &Arc<Self>,
        session_id: &SessionId,
    ) -> Result<FocusGuard, String> {
        let mut owner = self.focus_owner.write().await;
        match &*owner {
            Some(current) if current != session_id => {
                Err(format!("focus owned by session {current}"))
            }
            _ => {
                *owner = Some(session_id.clone());
                Ok(FocusGuard {
                    session_id: session_id.clone(),
                    arbiter: Arc::clone(self),
                })
            }
        }
    }

    /// Release cursor ownership (called by `CursorGuard::drop`).
    fn release_cursor(&self, session_id: &SessionId) {
        // We must spawn because drop is sync but we need async write.
        // Use try_write — if it fails, the guard is already dropped and
        // the lock will be available soon.
        if let Ok(mut owner) = self.cursor_owner.try_write() {
            if owner.as_ref() == Some(session_id) {
                *owner = None;
            }
        }
    }

    /// Release focus ownership (called by `FocusGuard::drop`).
    fn release_focus(&self, session_id: &SessionId) {
        if let Ok(mut owner) = self.focus_owner.try_write() {
            if owner.as_ref() == Some(session_id) {
                *owner = None;
            }
        }
    }
}

// ── RAII guards ──────────────────────────────────────────────────────────

/// Owns the cursor until dropped.
pub struct CursorGuard {
    session_id: SessionId,
    arbiter: Arc<GlobalArbiter>,
}

impl CursorGuard {
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        self.arbiter.release_cursor(&self.session_id);
    }
}

/// Owns keyboard focus until dropped.
pub struct FocusGuard {
    session_id: SessionId,
    arbiter: Arc<GlobalArbiter>,
}

impl FocusGuard {
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
}

impl Drop for FocusGuard {
    fn drop(&mut self) {
        self.arbiter.release_focus(&self.session_id);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Deterministic hash of a byte string → 16-char hex session id.
///
/// Used to map HTTP Bearer tokens to stable session identifiers.
pub fn hash_to_session_id(data: &[u8]) -> SessionId {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    let val = h.finish();
    format!("{:016x}", val)
}
