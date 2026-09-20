//! Turning a registry snapshot into the board's rows (FR-01 … FR-07, FR-40).
//!
//! Discovery is a **pure function of a snapshot**: the registry entries the outer layer
//! read, plus the processes it found. Nothing here touches the filesystem or the process
//! table, so the whole of it is testable from `tests/fixtures/session-registry.json`
//! (NFR-10, NFR-11).
//!
//! Sessions that fail validation are simply absent. They are not errors and are not
//! reported as any (FR-40): 14 of the 17 measured entries were stale, which is the normal
//! state of that directory rather than a fault.

use crate::session::{LiveProcess, RegistryEntry};

/// One live session, as the board will draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: String,
    pub pid: u32,
    /// Milliseconds since the epoch, from the registry.
    pub started_at: Option<i64>,
    /// The Claude Code version that owns this session, kept for triage (NFR-05).
    pub version: Option<String>,
    /// The registry's own name for the session.
    pub name: Option<String>,
    /// Who chose that name. `"derived"` means Claude Code made a slug from the folder and
    /// the id (`notes-e8`); anything else means somebody chose it, which ranks it above
    /// everything the board can work out
    /// ([ADR-0030](../../../docs/adr/0030-a-session-is-named-what-its-tab-is-named.md)).
    pub name_source: Option<String>,
}

/// A workspace group: several sessions in one folder, drawn under one header (FR-05, FR-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// The grouping key: the `cwd` exactly as the registry gave it (FR-04).
    pub key: String,
    /// What the header shows — the folder's last segment, never the whole path and never
    /// the lossy directory name under `projects/`.
    pub label: String,
    pub sessions: Vec<Session>,
}

/// The result of one discovery pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Groups in first-seen order. **Not alphabetical**: FR-23 requires the board's order to
    /// be stable across restarts, which means the order a workspace first appeared in, and
    /// sorting here would make that impossible to preserve.
    pub workspaces: Vec<Workspace>,
    /// Entries dropped because their process is not running. Counted, never reported as an
    /// error (FR-40) — this number is normally most of the directory.
    pub stale: usize,
    /// Entries dropped because they are not VS Code sessions a person is using (FR-02).
    pub filtered: usize,
}

impl Discovery {
    /// Every live session, flattened, in group order.
    pub fn sessions(&self) -> impl Iterator<Item = (&Workspace, &Session)> {
        self.workspaces
            .iter()
            .flat_map(|w| w.sessions.iter().map(move |s| (w, s)))
    }

    /// How many sessions the board would draw.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.workspaces.iter().map(|w| w.sessions.len()).sum()
    }
}

/// Runs one discovery pass over a registry snapshot.
///
/// `entries` is what the outer layer read from `~/.claude/sessions/`; `live` is what it
/// found in the process table. Both are data, so this function is deterministic and needs
/// no clock (NFR-11, TC-45a).
#[must_use]
pub fn discover(entries: &[RegistryEntry], live: &[LiveProcess]) -> Discovery {
    let mut out = Discovery::default();

    for entry in entries {
        if !entry.is_editor_session() {
            out.filtered += 1;
            continue;
        }
        if !live.iter().any(|p| p.matches(entry)) {
            out.stale += 1;
            continue;
        }

        let session = Session {
            session_id: entry.session_id.clone(),
            pid: entry.pid,
            started_at: entry.started_at,
            version: entry.version.clone(),
            name: entry.name.clone(),
            name_source: entry.name_source.clone(),
        };

        match out.workspaces.iter_mut().find(|w| w.key == entry.cwd) {
            Some(workspace) => workspace.sessions.push(session),
            None => out.workspaces.push(Workspace {
                key: entry.cwd.clone(),
                label: workspace_label(&entry.cwd),
                sessions: vec![session],
            }),
        }
    }

    out
}

/// The last segment of a path, for the group header (FR-04).
///
/// Both separators are accepted because the `cwd` is taken verbatim from the record, and
/// trailing separators are ignored. A path that has no segment at all — a bare root — keeps
/// its own text rather than becoming empty, since a header with no name is worse than an
/// ugly one.
#[must_use]
pub fn workspace_label(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches(['/', '\\']);
    let last = trimmed
        .rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty());
    match last {
        Some(segment) => segment.to_owned(),
        None => cwd.to_owned(),
    }
}
