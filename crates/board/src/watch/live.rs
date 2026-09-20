//! Watching the real thing: turning `~/.claude/` into a board.
//!
//! This is the wiring, and almost none of the thinking. Every decision it needs was made and
//! tested elsewhere — discovery in `mcv_core::discovery`, parsing in `mcv_core::record`,
//! status in `mcv_core::machine`, fitting in `mcv_core::title` — and what happens here is
//! that files are read, the process table is asked, and the results are handed to those.
//!
//! That division is [ADR-0019](../../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)'s,
//! and this file is the half it puts outside: the clock, the filesystem and the process table
//! all live here, and none of them reaches the core except as an argument.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mcv_core::discovery::{Discovery, discover};
use mcv_core::event::{Event, HookSignal, Observation};
use mcv_core::machine::Machine;
use mcv_core::palette::{Status, Theme};
use mcv_core::record::Opening;

/// The most of an opening prompt the board will ever hold.
///
/// A title is a label. A prompt that runs to a paragraph has said what a label needs in its
/// first line, and this caps even that — so what the product builds from what the user wrote
/// is one short line per session (NFR-07, ADR-0030).
const OPENING_LINE_CAP: usize = 120;
use mcv_core::record::{Record, parse_line};
use mcv_core::session::{LiveProcess, RegistryEntry};
use mcv_core::time::Timestamp;

use crate::events::{self, Mode};
use crate::view::{BoardView, GroupState, Inspect, SessionState, build};
use crate::watch::process::ProcessTable;
use crate::watch::tail::{Polled, Tail};

/// Where Claude Code keeps its state.
///
/// `CLAUDE_CONFIG_DIR` first, because that is what Claude Code itself honours; then the home
/// directory. Read from the environment rather than through a crate, since one variable is
/// not worth a dependency.
#[must_use]
pub fn claude_dir() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(explicit));
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(PathBuf::from(home).join(".claude"))
}

/// What the board keeps between polls.
pub struct Watcher {
    claude: PathBuf,
    theme: Theme,
    inspect: Inspect,
    processes: ProcessTable,
    /// One state machine per session, because a status is a fold over that session's own
    /// history — starting a fresh machine each poll would lose everything before it.
    machines: HashMap<String, Machine>,
    /// One tail per session, holding the byte offset already read (NFR-02).
    tails: HashMap<String, Tail>,
    /// When each session last changed status, for the time column (FR-24).
    since: HashMap<String, i64>,
    /// The most recent `ai-title` seen for each session.
    titles: HashMap<String, String>,
    /// Each session's opening human prompt, once it has been seen.
    ///
    /// **A key means it was seen; the value is what it said.** `Some` is the first line,
    /// `None` a prompt that yielded nothing usable — and recording that second case is what
    /// stops the search running on to the next prompt, and the next, until one had text. The
    /// promise is one prompt per session, and it is only a promise if a prompt with nothing
    /// in it also ends the looking (ADR-0030, NFR-07).
    openings: HashMap<String, Option<String>>,
    /// Each live session's process, and the process that started it — the extension host,
    /// which is per VS Code window and is where a raise begins (FR-33, ADR-0009).
    processes_of: HashMap<String, (u32, Option<u32>)>,
    /// What the hook helper has reported, and how far through its log the board has read.
    hooks: HookLog,
    /// Whether our entries are in the user's settings file, and the file's modification time
    /// when that was last decided.
    ///
    /// Cached against the mtime rather than re-read every 750 ms: the settings file measured
    /// on the author's own machine was 120 KB, and parsing it eight times a second to answer
    /// a question that changes about once a year is not a cost the board should carry
    /// (NFR-03).
    installed: (bool, Option<SystemTime>),
}

impl Watcher {
    #[must_use]
    pub fn new(claude: PathBuf, theme: Theme, inspect: Inspect) -> Self {
        Self {
            claude,
            theme,
            inspect,
            processes: ProcessTable::new(),
            machines: HashMap::new(),
            tails: HashMap::new(),
            since: HashMap::new(),
            titles: HashMap::new(),
            openings: HashMap::new(),
            processes_of: HashMap::new(),
            hooks: HookLog::default(),
            installed: (false, None),
        }
    }

    /// Reads everything that has changed and returns the board to draw.
    ///
    /// `now` is passed in rather than read here, so that the whole of this is replayable and
    /// a test can drive it without waiting.
    pub fn poll(&mut self, now: i64, folded: &dyn Fn(&str) -> bool) -> BoardView {
        let found = self.discover_sessions();
        self.forget_sessions_that_ended(&found);
        // Read once for the whole board: the log is one file, shared by every session on the
        // machine — including terminal ones the board never shows.
        let mut reported = self.read_hook_events();
        let mut claimed: Vec<String> = Vec::new();

        let mut groups: Vec<GroupOwned> = Vec::new();
        for workspace in &found.workspaces {
            let mut sessions = Vec::new();
            for session in &workspace.sessions {
                let signals = reported.remove(&session.session_id).unwrap_or_default();
                let status = self.advance(&session.session_id, now, &signals);
                claimed.push(session.session_id.clone());
                let since = *self.since.entry(session.session_id.clone()).or_insert(now);
                sessions.push(SessionOwned {
                    id: session.session_id.clone(),
                    status,
                    title: self.titles.get(&session.session_id).cloned(),
                    opening: self.openings.get(&session.session_id).cloned().flatten(),
                    // Split by who chose it. A `derived` name is a slug this product should
                    // rank below the opening prompt; anything else was chosen, and outranks
                    // everything the board could work out for itself.
                    registry_name: (session.name_source.as_deref() != Some("derived"))
                        .then(|| session.name.clone())
                        .flatten(),
                    derived_name: (session.name_source.as_deref() == Some("derived"))
                        .then(|| session.name.clone())
                        .flatten(),
                    time: elapsed_label(now.saturating_sub(since)),
                });
            }
            groups.push(GroupOwned {
                key: workspace.key.clone(),
                label: workspace.label.clone(),
                folded: folded(&workspace.key),
                sessions,
            });
        }

        let borrowed: Vec<GroupState<'_>> = groups
            .iter()
            .map(|group| GroupState {
                key: &group.key,
                label: &group.label,
                folded: group.folded,
                sessions: group
                    .sessions
                    .iter()
                    .map(|session| SessionState {
                        id: &session.id,
                        status: session.status,
                        ai_title: session.title.as_deref(),
                        registry_name: session.registry_name.as_deref(),
                        prompt_line: session.opening.as_deref(),
                        derived_name: session.derived_name.as_deref(),
                        time: session.time.clone(),
                    })
                    .collect(),
            })
            .collect();

        // Whatever no session claimed is offered once more. A session registers a moment
        // before discovery sees it, and a `SessionStart` — or a permission prompt in the first
        // second — would otherwise be read past and lost.
        for id in claimed {
            reported.remove(&id);
        }
        self.hooks.keep_unclaimed(reported);

        let mut view = build(&borrowed, self.theme, self.inspect);
        view.mode = self.mode(now).label();
        view
    }

    /// Which mode the board is in, and whether it is still earning the word (FR-52, FR-55).
    ///
    /// Two facts and no more: whether the entries are installed, and when an event last
    /// arrived. Installed and silent is not exact — an installed hook that has stopped
    /// firing is indistinguishable from no hook at all, and the sessions have already
    /// stopped believing it by the time this says so, because both sides read the same
    /// number (`mcv_core::machine::HOOK_QUIET_MS`).
    fn mode(&mut self, now: i64) -> Mode {
        events::mode(self.hooks_installed(), self.hooks.last_at, now)
    }

    /// Whether the product's own entries are in the user's settings file.
    ///
    /// Re-read only when the file has changed. A parse failure answers "no": FR-53 keeps the
    /// product from writing a guess into a file it cannot read, and the same restraint
    /// applies to claiming exactness on the strength of one.
    fn hooks_installed(&mut self) -> bool {
        let path = self.claude.join("settings.json");
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if mtime == self.installed.1 && self.installed.1.is_some() {
            return self.installed.0;
        }

        let helper = crate::setup::helper_path();
        let installed = match helper {
            Some(helper) => std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|value| crate::hooks::installed_in(&value, &helper).ok())
                .is_some_and(|installed| installed != crate::hooks::Installed::No),
            None => false,
        };
        self.installed = (installed, mtime);
        installed
    }

    /// Whatever the helper has appended since the last poll, grouped by session.
    fn read_hook_events(&mut self) -> HashMap<String, Vec<HookSignal>> {
        self.hooks.poll()
    }

    /// One discovery pass: the registry files, and which of their processes are running.
    fn discover_sessions(&mut self) -> Discovery {
        let entries = read_registry(&self.claude.join("sessions"));
        let pids: Vec<u32> = entries.iter().map(|entry| entry.pid).collect();
        let live: Vec<LiveProcess> = self.processes.live_among(&pids);
        let found = discover(&entries, &live);

        // Remember each session's process while it is known to be running. A click can come
        // at any time, and looking the pid up then would mean re-reading the registry to
        // answer a question already answered here.
        for (_, session) in found.sessions() {
            let parent = self.processes.parent_of(session.pid);
            self.processes_of
                .insert(session.session_id.clone(), (session.pid, parent));
        }
        found
    }

    /// Where the editor lock files are.
    #[must_use]
    pub fn ide_dir(&self) -> PathBuf {
        self.claude.join("ide")
    }

    /// Every session's process, for the board to publish where a click can reach it without
    /// waiting on this watcher's lock.
    #[must_use]
    pub fn session_processes(&self) -> HashMap<String, (u32, Option<u32>)> {
        self.processes_of.clone()
    }

    /// Reads whatever the session's transcript has gained, and folds it in.
    fn advance(&mut self, session_id: &str, now: i64, reported: &[HookSignal]) -> Status {
        let at = Timestamp::from_millis(now);

        if !self.tails.contains_key(session_id) {
            if let Some(path) = self.find_transcript(session_id) {
                self.tails.insert(session_id.to_owned(), Tail::new(path));
            }
        }

        let mut observations: Vec<Observation> = Vec::new();
        if let Some(tail) = self.tails.get_mut(session_id) {
            match tail.poll() {
                Ok(polled) => {
                    for (offset, line) in polled.lines {
                        match parse_line(&line, offset) {
                            Ok(entry) => {
                                if let Record::AiTitle(title) = &entry.record {
                                    self.titles.insert(session_id.to_owned(), title.clone());
                                }
                                // Only until this session's first human prompt has been
                                // seen, so the text of exactly one prompt per session is ever
                                // built (ADR-0030).
                                if !self.openings.contains_key(session_id) {
                                    match mcv_core::record::human_prompt_line(
                                        &line,
                                        OPENING_LINE_CAP,
                                    ) {
                                        Opening::NotAPrompt => {}
                                        Opening::NoLine => {
                                            self.openings.insert(session_id.to_owned(), None);
                                        }
                                        Opening::Line(opening) => {
                                            self.openings
                                                .insert(session_id.to_owned(), Some(opening));
                                        }
                                    }
                                }
                                observations.push(Observation::new(
                                    at,
                                    Event::Record {
                                        offset,
                                        entry: Box::new(entry),
                                    },
                                ));
                            }
                            // A line that cannot be read is reported once, as `unknown`, not
                            // swallowed and not fatal (FR-39).
                            Err(_) => observations.push(Observation::new(at, Event::Unreadable)),
                        }
                    }
                    // A line that is not valid UTF-8 says the same thing as one that will
                    // not parse: the observer has lost the thread here (FR-39).
                    for _ in &polled.undecodable {
                        observations.push(Observation::new(at, Event::Unreadable));
                    }
                }
                Err(_) => {
                    // The transcript went away under us. That is not the session ending —
                    // liveness is decided by the process, never by a file (§3).
                }
            }
        }

        // The hooks last, after the transcript this poll read. Both were written before the
        // poll and neither carries an arrival time the other can be compared against, so
        // there is no true order between them — what there is, is §4.1 rule 4: where a hook
        // reported something, that is the answer. The machine is built so that the *wait*
        // does not depend on this order either way
        // ([ADR-0020](../../../../docs/adr/0020-hook-signals-are-facts-not-status-writes.md)).
        for signal in reported {
            observations.push(Observation::new(at, Event::Hook(*signal)));
        }

        // Time passing is an event, always, so the debounces advance even in a quiet session.
        observations.push(Observation::new(at, Event::Tick));

        let machine = self.machines.entry(session_id.to_owned()).or_default();
        let mut changed = false;
        for observation in &observations {
            if machine.observe(observation).is_some() {
                changed = true;
            }
        }
        if changed {
            self.since.insert(session_id.to_owned(), now);
        }
        machine.status()
    }

    /// Finds a session's transcript.
    ///
    /// By searching for the file rather than computing its directory: the names under
    /// `projects/` are a **lossy** encoding of the workspace path — both `_` and `-` become
    /// `-` — so a path built from the `cwd` would be wrong for any workspace containing an
    /// underscore (`docs/observation-sources.md` §2.3).
    fn find_transcript(&self, session_id: &str) -> Option<PathBuf> {
        let projects = self.claude.join("projects");
        let file = format!("{session_id}.jsonl");
        std::fs::read_dir(projects)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join(&file))
            .find(|candidate| candidate.is_file())
    }

    /// Drops everything held for sessions that are no longer running, so that a board left
    /// open all day does not grow (NFR-03).
    fn forget_sessions_that_ended(&mut self, found: &Discovery) {
        let live: Vec<&str> = found
            .sessions()
            .map(|(_, session)| session.session_id.as_str())
            .collect();
        let keep = |id: &String| live.iter().any(|alive| alive == id);
        self.machines.retain(|id, _| keep(id));
        self.tails.retain(|id, _| keep(id));
        self.since.retain(|id, _| keep(id));
        self.titles.retain(|id, _| keep(id));
        self.openings.retain(|id, _| keep(id));
        self.processes_of.retain(|id, _| keep(id));
    }
}

struct GroupOwned {
    key: String,
    label: String,
    folded: bool,
    sessions: Vec<SessionOwned>,
}

struct SessionOwned {
    id: String,
    status: Status,
    title: Option<String>,
    opening: Option<String>,
    registry_name: Option<String>,
    derived_name: Option<String>,
    time: String,
}

/// Every `sessions/<pid>.json` that parses.
///
/// Files that do not are skipped in silence: the directory holds entries for processes that
/// died long ago — measured at 3 live of 17 — and that is its normal state, not a fault
/// (FR-40).
#[must_use]
pub fn read_registry(dir: &Path) -> Vec<RegistryEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<RegistryEntry> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            RegistryEntry::parse(&text).ok()
        })
        .collect();
    // Oldest first, so a workspace keeps the position it first appeared in (FR-23).
    found.sort_by_key(|entry| entry.started_at.unwrap_or(i64::MAX));
    found
}

/// Milliseconds as the time column shows them: coarse, and never more precise than it is
/// (FR-24).
///
/// The granularity coarsens because a board is read at a glance: `12s` and `13s` differ by
/// nothing a user acts on, and a column that ticks every second is movement that means
/// nothing.
#[must_use]
pub fn elapsed_label(millis: i64) -> String {
    let seconds = millis / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h", seconds / 3600)
    }
}

/// The wall clock, in milliseconds. The one place the board reads it.
#[must_use]
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The hook helper's event log, followed by byte offset exactly as a transcript is (NFR-02).
///
/// Separate from [`Watcher`] because the part worth getting right is not the reading — the
/// [`Tail`] does that — but what is done with a line once it has been read, and that part is
/// testable on its own.
#[derive(Debug, Default)]
pub struct HookLog {
    /// **Opened at the end of the file, not the beginning.**
    ///
    /// Starting from the beginning would replay the whole history every time the board
    /// starts: a permission prompt reported and answered three days ago would summon the
    /// user again, because the log records that Claude asked and nothing in it records that
    /// the user replied. What is worth having is what happens from now on — and a wait that
    /// began before the board did is one the board was not there to show anyway.
    tail: Option<Tail>,
    /// The newest event already folded in, and the lines that carried that exact stamp.
    ///
    /// The byte offset covers everything except one case: the log being trimmed or replaced
    /// under the tail, which puts the offset back to zero and would otherwise re-deliver
    /// what is left in the file (`Polled::restarted`).
    ///
    /// The timestamp alone is not enough to tell the re-delivered lines from new ones. Several
    /// processes append here and the helper stamps in whole milliseconds, so two events
    /// sharing a stamp is ordinary — dropping everything at `newest_at` would throw away a
    /// genuinely new line that happened to land in the same millisecond as the last one read.
    /// So the lines at that stamp are kept, and compared exactly.
    newest_at: i64,
    newest_lines: Vec<String>,
    /// When any event last arrived — the half of the mode FR-52 turns on.
    pub last_at: Option<i64>,
    /// Signals nobody had a session for, kept for exactly one more poll.
    ///
    /// A session registers a moment before the board discovers it, and hook events do not
    /// wait for that: `SessionStart` fires at once, and a permission prompt in the first
    /// second of a session is not hypothetical. Dropping those would lose them for good — the
    /// log is tailed by byte offset, so nothing is read twice.
    ///
    /// One poll, and no longer. The file is shared with terminal sessions the board never
    /// shows, and their lines arrive forever; keeping them would be a leak with a slow fuse.
    unclaimed: HashMap<String, Vec<HookSignal>>,
}

impl HookLog {
    /// Reads whatever has been appended, and returns it grouped by session.
    ///
    /// The file is shared by every Claude Code session on the machine, including terminal
    /// ones the board never shows. That is by design — the helper cannot know which sessions
    /// the board displays — and the lines the board has no session for are simply left in the
    /// map, which is dropped when the poll ends.
    pub fn poll(&mut self) -> HashMap<String, Vec<HookSignal>> {
        let fresh = self.read();
        self.offer(fresh)
    }

    /// Puts what was carried over in front of what was just read.
    ///
    /// Separate from the reading so it can be driven without a file: the ordering is the part
    /// worth asserting, since a `SessionStart` carried from the previous poll happened before
    /// anything in this one.
    pub fn offer(
        &mut self,
        fresh: HashMap<String, Vec<HookSignal>>,
    ) -> HashMap<String, Vec<HookSignal>> {
        let mut carried = std::mem::take(&mut self.unclaimed);
        for (session, signals) in fresh {
            carried.entry(session).or_default().extend(signals);
        }
        carried
    }

    /// Hands back whatever the board had no session for, to be offered once more.
    ///
    /// The board takes what it can use out of the map [`Self::poll`] returns; what is left is
    /// either a terminal session this board never shows, or a session it has not discovered
    /// yet, and only the next poll can tell those apart.
    pub fn keep_unclaimed(&mut self, leftover: HashMap<String, Vec<HookSignal>>) {
        self.unclaimed = leftover;
    }

    fn read(&mut self) -> HashMap<String, Vec<HookSignal>> {
        if self.tail.is_none() {
            let Some(path) = events::events_path() else {
                return HashMap::new();
            };
            // What is already in the log is not replayed, but it does answer one question:
            // when an event last arrived. Without it a board started beside perfectly healthy
            // hooks would report them as having gone quiet until the next one fired — which
            // in an idle session can be an hour away, and a board that cries wolf about its
            // own accuracy is worse than one that says nothing (FR-52).
            //
            // It is also the baseline for the restart check below, so a trim cannot make the
            // history the board deliberately skipped look new.
            // The stamp *and* the lines carrying it. Taking only the stamp left a hole with
            // a narrow entrance and a real drop: if the log were rewritten before anything
            // new was appended, a line already in the file at exactly that stamp would not be
            // recognised as history and would be folded in — an old `Notification` summoning
            // the user about a prompt answered days ago.
            let (newest, lines) = events::newest_line_group(&path);
            self.newest_at = newest;
            self.newest_lines = lines;
            self.last_at = (self.newest_at > 0).then_some(self.newest_at);
            let end = std::fs::metadata(&path).map_or(0, |m| m.len());
            self.tail = Some(Tail::resuming_at(path, end));
        }
        let Some(tail) = self.tail.as_mut() else {
            return HashMap::new();
        };
        match tail.poll() {
            Ok(polled) => self.absorb(&polled),
            // The log went away under us, or could not be read. Hooks are optional; their
            // absence is the ordinary case rather than a fault, and the sessions fall back to
            // inference on their own once the silence passes `T_hook_quiet` (FR-52).
            Err(_) => HashMap::new(),
        }
    }

    /// What one poll of the log means, once the bytes are in hand.
    ///
    /// Public so it can be driven with lines rather than with a file: the reading is the
    /// [`Tail`]'s, already tested, and this is the half with the judgement in it.
    pub fn absorb(&mut self, polled: &Polled) -> HashMap<String, Vec<HookSignal>> {
        let mut by_session: HashMap<String, Vec<HookSignal>> = HashMap::new();
        for (_, line) in &polled.lines {
            let Ok(event) = serde_json::from_str::<events::HookEvent>(line) else {
                // Several processes append to this file at once, so a torn line is a normal
                // thing to meet rather than a fault (FR-40).
                continue;
            };
            // The log was trimmed or replaced under the tail, so the offset is back at zero
            // and everything before is being handed over again. An old `Notification`
            // delivered a second time is a summons nobody asked for.
            //
            // Older than the newest line already read is old news. *Equal* to it is decided
            // by the line itself, because the helper stamps in whole milliseconds and several
            // processes write here: two events can share a stamp, and one of them can be new.
            if polled.restarted
                && (event.at < self.newest_at
                    || (event.at == self.newest_at && self.newest_lines.contains(line)))
            {
                continue;
            }
            if event.at > self.newest_at {
                self.newest_at = event.at;
                self.newest_lines.clear();
            }
            if event.at == self.newest_at {
                self.newest_lines.push(line.clone());
            }
            // Every line counts towards "the hooks are alive", including the ones with no
            // meaning in the state model: FR-52 asks whether events are *arriving*, not
            // whether this build understood them.
            self.last_at = Some(self.last_at.map_or(event.at, |at| at.max(event.at)));
            if let Some(signal) = event.signal() {
                by_session.entry(event.session).or_default().push(signal);
            }
        }
        by_session
    }
}
