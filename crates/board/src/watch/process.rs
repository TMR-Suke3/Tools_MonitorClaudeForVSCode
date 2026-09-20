//! Asking the operating system which processes are running (FR-40, and the PID-reuse guard).
//!
//! The outer layer again: the core decides what a live session *is* and cannot look
//! ([ADR-0019](../../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
//! This module answers the one question the core needs — for these PIDs, which are running
//! and when did each start — and hands back
//! [`mcv_core::session::LiveProcess`] values.
//!
//! **Read-only, and narrower than read-only.** `docs/observation-sources.md` §1 allows the
//! process table to be read for "process ids and image names, never command lines", and
//! nothing here is ever signalled or injected. Only the id and the start time are taken;
//! even the image name is not, because nothing needs it.
//!
//! A start time is what makes the guard work: a PID on its own is not an identity, since
//! PIDs are reused. A stale registry entry whose number now belongs to an unrelated process
//! would otherwise resurrect a dead session on the board.

use std::path::Path;

use mcv_core::session::LiveProcess;
use std::path::PathBuf;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// A view of the process table, refreshed on demand.
///
/// Held across polls rather than rebuilt, because building one is the expensive part and the
/// board does this every couple of seconds all day (NFR-03).
pub struct ProcessTable {
    system: System,
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    /// Refreshes the table and returns the live processes among `pids`.
    ///
    /// Only the named PIDs are refreshed. The board knows which sessions it is watching, and
    /// enumerating every process on the machine to answer a question about a handful of them
    /// is the kind of cost a resident monitor cannot justify.
    pub fn live_among(&mut self, pids: &[u32]) -> Vec<LiveProcess> {
        if pids.is_empty() {
            return Vec::new();
        }

        let wanted: Vec<Pid> = pids.iter().map(|&p| Pid::from_u32(p)).collect();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&wanted),
            true,
            // Nothing but the start time is wanted, and asking for less is what keeps this
            // cheap: no command line, no environment, no disk or memory accounting.
            ProcessRefreshKind::nothing(),
        );

        wanted
            .iter()
            .filter_map(|pid| {
                let process = self.system.process(*pid)?;
                Some(LiveProcess {
                    pid: pid.as_u32(),
                    // sysinfo reports whole seconds since the Unix epoch, which is the
                    // precision the comparison in `mcv-core` is written for.
                    started_unix_seconds: i64::try_from(process.start_time()).ok(),
                })
            })
            .collect()
    }

    /// Whether `pid` is `ancestor`, or was started by it, however indirectly.
    ///
    /// Used to tell a real editor window from a window that merely mentions the workspace in
    /// its title — a browser showing a pull request for a repository of the same name will
    /// match the title and must not be raised.
    ///
    /// The walk is bounded. A process tree should not be a thousand deep, and a cycle in the
    /// reported parents — which a pid reused mid-walk can produce — must not hang a click.
    pub fn is_descendant_of(&mut self, pid: u32, ancestor: u32) -> bool {
        // Deep enough for any real chain — an editor window is a handful of hops from its
        // instance, and a launcher or a sandbox wrapper adds a few more. Sixteen was too
        // mean: it would have answered "not the editor" for an unusual but legitimate tree,
        // and a raise that fails for a reason nobody can see is the failure this whole path
        // exists to avoid.
        const LIMIT: usize = 64;

        let mut seen: Vec<u32> = Vec::with_capacity(8);
        let mut current = pid;
        for _ in 0..LIMIT {
            if current == ancestor {
                return true;
            }
            // A cycle is possible: a PID reused mid-walk can point back at something already
            // visited. Stopping at the repeat costs one comparison and saves the rest of the
            // limit in parent lookups, for every candidate window on the desktop.
            if seen.contains(&current) {
                return false;
            }
            seen.push(current);

            match self.parent_of(current) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
    }

    /// The process that started `pid`.
    ///
    /// For a session this is the **extension host**, and the extension host is per VS Code
    /// window — which is the whole reason the window can be identified at all
    /// ([ADR-0009](../../../../docs/adr/0009-map-a-session-to-its-window.md)). The lock
    /// file's own `pid` names the editor instance and cannot tell two windows apart.
    ///
    /// Measured only for sessions the extension launched. A `claude` started by hand in an
    /// integrated terminal descends from the terminal's host instead, and whether that host
    /// is per window was never tested — so a raise for such a session may find nothing,
    /// which it will say rather than hide.
    pub fn parent_of(&mut self, pid: u32) -> Option<u32> {
        let wanted = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[wanted]),
            true,
            ProcessRefreshKind::nothing(),
        );
        self.system.process(wanted)?.parent().map(Pid::as_u32)
    }

    /// Where a process's executable lives.
    ///
    /// The **image name**, which `docs/observation-sources.md` §1 allows this module to read
    /// and which the comment at the top says nothing needed — until now. Revealing a session
    /// means running the editor's own command-line launcher, and that launcher sits beside
    /// the editor's executable: this is how the board finds *the installation whose window it
    /// just raised*, rather than whichever one the machine has associated with a URL scheme
    /// ([ADR-0029](../../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)).
    ///
    /// Still not the command line, which stays forbidden and is not needed.
    pub fn exe_of(&mut self, pid: u32) -> Option<PathBuf> {
        let wanted = Pid::from_u32(pid);
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[wanted]),
            true,
            ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
        );
        self.system.process(wanted)?.exe().map(Path::to_path_buf)
    }
}
