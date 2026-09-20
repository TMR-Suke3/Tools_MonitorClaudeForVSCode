//! Installing and removing the product's hook entries in the user's `settings.json`.
//!
//! This is the only code in the product that writes to a file the user owns, and everything
//! about it is shaped by that. It is **additive**: it adds entries carrying a marker, it
//! removes entries carrying that marker, and it cannot express any other change (FR-45). It
//! refuses rather than guesses when the file will not parse (FR-53). It takes a backup first
//! and says where it went (FR-44). And nothing here runs without the user having asked for
//! it (FR-54) — the flow in `docs/hook-setup.md` §4 is the caller.
//!
//! The merge is a pure function from one `Value` to another, so the difficult half — *what
//! changes* — is testable without a settings file anywhere near it. The file I/O is the thin
//! layer at the bottom.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// What makes an entry ours.
///
/// Versioned, because an upgrade must recognise and replace the previous version's entries
/// while still never touching anyone else's (FR-46). Matching is on [`MARKER_PREFIX`], so a
/// v1 entry is still recognised as ours by a later build.
pub const MARKER: &str = "# monitor-claude-vscode v1";

/// What every version of our marker starts with.
pub const MARKER_PREFIX: &str = "# monitor-claude-vscode";

/// The events installed, and no others (FR-47).
///
/// Pinned against Claude Code's documentation on 2026-08-27 (FR-53). Every one of these runs
/// *after* the thing it reports, so none of them can block, delay or alter a tool call or a
/// prompt. That is a property of the list itself, not a promise about the helper's
/// behaviour — which is what makes it a guarantee rather than an intention.
pub const EVENTS: [&str; 5] = [
    "SessionStart",
    "UserPromptSubmit",
    "Notification",
    "Stop",
    "SessionEnd",
];

/// The timeout written onto each entry: the second line of defence behind the helper's own
/// "always exit 0" (FR-48). If the helper ever hangs, Claude Code stops waiting for it.
pub const TIMEOUT_SECONDS: u64 = 5;

/// Why the product will not write (FR-53).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The file exists but is not JSON. Very often that is a comma the user is halfway
    /// through fixing, and overwriting it would destroy work in progress.
    Unparseable(String),
    /// The file is JSON, but not an object — so it is not a settings file we understand.
    NotAnObject,
    /// `hooks`, or something under it, is not the shape this build expects. Another tool may
    /// have written it, or the schema may have moved under us.
    UnexpectedShape(&'static str),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unparseable(why) => {
                write!(f, "the settings file could not be read as JSON ({why})")
            }
            Self::NotAnObject => write!(f, "the settings file is not a JSON object"),
            Self::UnexpectedShape(at) => {
                write!(
                    f,
                    "`{at}` in the settings file is not the shape this version expects"
                )
            }
        }
    }
}

/// Whether our entries are in a settings file already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    /// No entry of ours anywhere.
    No,
    /// Exactly this build's entries: every event, at this version, once each.
    Current,
    /// Entries of ours are present, but not that set — an older version, a partial removal,
    /// or a duplicate. Setup replaces them, since remove-then-add reaches the same end state
    /// from all of these (FR-46).
    Other,
}

/// The command string for one event.
///
/// The helper path is quoted because it contains spaces on a normal Windows install, and the
/// marker trails it. On Windows the shell does not treat `#` as a comment, so the marker
/// reaches the helper as two extra arguments — which it ignores.
#[must_use]
pub fn command_for(event: &str, helper: &Path) -> String {
    format!("\"{}\" {event} {MARKER}", helper.display())
}

/// True for a handler that is ours, at any version.
fn is_ours(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(MARKER_PREFIX))
}

/// Adds this build's entries, having first taken out every entry of ours at any version.
///
/// Remove-then-add is what makes setup idempotent and an upgrade exact (FR-46): running it
/// twice leaves one entry per event, and running a new build over an old one leaves the new
/// build's entries only. Everything without our marker survives untouched and in its
/// original order (FR-45).
///
/// # Errors
///
/// [`Refusal`] if the value is not an object, or if `hooks` is not the shape expected. In
/// both cases nothing is written and the caller explains instead (FR-53).
pub fn install_into(settings: &Value, helper: &Path) -> Result<Value, Refusal> {
    let mut next = remove_from(settings)?;
    let root = next.as_object_mut().ok_or(Refusal::NotAnObject)?;

    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or(Refusal::UnexpectedShape("hooks"))?;

    for event in EVENTS {
        let entry = json!({
            "hooks": [{
                "type": "command",
                "command": command_for(event, helper),
                "timeout": TIMEOUT_SECONDS,
            }]
        });
        // Appended, so anything the user already has on this event keeps both its content
        // and its position — including the right to run first.
        hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or(Refusal::UnexpectedShape("hooks.<event>"))?
            .push(entry);
    }

    Ok(next)
}

/// Takes out every entry carrying our marker, at any version, and nothing else (FR-51).
///
/// A file with no entries of ours comes back unchanged, which is what makes removal safe to
/// offer whatever state the file is in — and why removal never needs the backup to exist.
///
/// # Errors
///
/// [`Refusal`] if the value is not an object, or `hooks` is not the shape expected.
pub fn remove_from(settings: &Value) -> Result<Value, Refusal> {
    let mut next = settings.clone();
    let root = next.as_object_mut().ok_or(Refusal::NotAnObject)?;

    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(next);
    };
    let hooks = hooks
        .as_object_mut()
        .ok_or(Refusal::UnexpectedShape("hooks"))?;

    // Which event keys *we* emptied. A key that was already empty when we arrived is
    // somebody else's business, and leaving it is the difference between removing our
    // entries and tidying the user's file (FR-45).
    let mut emptied: Vec<String> = Vec::new();

    for (event, groups) in hooks.iter_mut() {
        let groups = groups
            .as_array_mut()
            .ok_or(Refusal::UnexpectedShape("hooks.<event>"))?;
        let before = groups.len();
        let mut removed_any = false;

        for group in groups.iter_mut() {
            // A group without a `hooks` array holds nothing of ours by construction, so it
            // is left exactly as found rather than judged.
            let Some(handlers) = group.get_mut("hooks") else {
                continue;
            };
            let handlers = handlers
                .as_array_mut()
                .ok_or(Refusal::UnexpectedShape("hooks.<event>[].hooks"))?;
            let was = handlers.len();
            handlers.retain(|handler| !is_ours(handler));
            removed_any |= handlers.len() != was;
        }

        if removed_any {
            // A group we emptied is a group we created: ours are written with exactly one
            // handler in them. A group we share with the user keeps their handlers, so it is
            // not empty, so it stays.
            groups.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_none_or(|handlers| !handlers.is_empty())
            });
            if groups.is_empty() && before > 0 {
                emptied.push(event.clone());
            }
        }
    }

    let we_emptied_something = !emptied.is_empty();
    for event in emptied {
        hooks.remove(&event);
    }
    // Likewise a `hooks` key that is empty only because we emptied it: removing our entries
    // should leave the file the way we found it, not a step further.
    if hooks.is_empty() && we_emptied_something {
        root.remove("hooks");
    }

    Ok(next)
}

/// What a settings file currently holds.
///
/// # Errors
///
/// [`Refusal`] if `hooks` is not the shape expected.
pub fn installed_in(settings: &Value, helper: &Path) -> Result<Installed, Refusal> {
    let Some(hooks) = settings.get("hooks") else {
        return Ok(Installed::No);
    };
    let hooks = hooks.as_object().ok_or(Refusal::UnexpectedShape("hooks"))?;

    let ours_under = |event: &str| -> Vec<&Value> {
        hooks
            .get(event)
            .and_then(Value::as_array)
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(|group| group.get("hooks").and_then(Value::as_array))
                    .flatten()
                    .filter(|handler| is_ours(handler))
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut ours = 0usize;
    let mut current = 0usize;
    for event in EVENTS {
        let want = command_for(event, helper);
        let found = ours_under(event);
        ours += found.len();
        if let [handler] = found[..]
            && handler.get("command").and_then(Value::as_str) == Some(want.as_str())
        {
            current += 1;
        }
    }
    // An entry of ours on an event this build does not install means an older version chose
    // a different set. It is still ours, and setup still has to replace it.
    let strays: usize = hooks
        .keys()
        .filter(|event| !EVENTS.contains(&event.as_str()))
        .map(|event| ours_under(event).len())
        .sum();

    Ok(if ours + strays == 0 {
        Installed::No
    } else if strays == 0 && current == EVENTS.len() && ours == EVENTS.len() {
        Installed::Current
    } else {
        Installed::Other
    })
}

// ---------------------------------------------------------------- the preview

/// The change, as the user is shown it before it happens (FR-43).
#[derive(Debug, Clone)]
pub struct Change {
    /// The file as it would be written.
    pub after: String,
    /// The change, line by line: `+` added, `-` removed, ` ` context.
    pub diff: String,
    /// True when the whole file will be re-printed rather than only added to.
    ///
    /// The product parses the settings and prints them back, so a file laid out any other
    /// way comes back in this printer's layout — a different indentation, different line
    /// endings, or no whitespace at all. The values are identical either way, but "your file
    /// will be rewritten" is exactly the sort of thing a user should hear *before* agreeing,
    /// not discover in `git diff` afterwards. Claude Code's own two-space output round-trips
    /// through this untouched, so normally it is false.
    pub reformats: bool,
    /// True when the change is empty: nothing to install, or nothing of ours to remove.
    pub nothing_to_do: bool,
    /// How many *entries* the change adds and removes.
    ///
    /// The unit the user was told about one screen earlier is "five entries", not "53
    /// lines", so the preview restates the change in that unit as well. Counted from the
    /// marker in the diff, like everything else here, so it cannot claim a number the pane
    /// does not show.
    pub entries_added: usize,
    pub entries_removed: usize,
    /// How many lines the change adds and removes.
    ///
    /// So that the screen can say "22 lines added, none removed" in words — the one fact
    /// FR-45 is a promise about, stated rather than left to whether the reader knows what a
    /// diff is. Counted from the diff itself, so it cannot disagree with what is shown.
    pub added: usize,
    pub removed: usize,
}

/// Works out what installing (`helper: Some`) or removing (`helper: None`) would do to this
/// text, without doing it.
///
/// # Errors
///
/// [`Refusal`] if the text will not parse, or is not the shape this build expects. Nothing
/// is written in that case, and the caller shows the reason (FR-53).
pub fn preview(text: &str, helper: Option<&Path>) -> Result<Change, Refusal> {
    let settings: Value =
        serde_json::from_str(text).map_err(|e| Refusal::Unparseable(e.to_string()))?;
    if !settings.is_object() {
        return Err(Refusal::NotAnObject);
    }

    let next = match helper {
        Some(helper) => install_into(&settings, helper)?,
        None => remove_from(&settings)?,
    };

    let before = print(&settings);
    let after = print(&next);
    let diff = diff(&before, &after);
    Ok(Change {
        added: diff.lines().filter(|line| line.starts_with('+')).count(),
        removed: diff.lines().filter(|line| line.starts_with('-')).count(),
        entries_added: count_entries(&diff, '+'),
        entries_removed: count_entries(&diff, '-'),
        diff,
        nothing_to_do: before == after,
        // The file we were handed is not what our own printer produces, so agreeing to this
        // change also means agreeing to be reformatted.
        reformats: before.trim_end() != text.trim_end(),
        after,
    })
}

/// Entries of ours on one side of the diff. One marker, one entry.
fn count_entries(diff: &str, sign: char) -> usize {
    diff.lines()
        .filter(|line| line.starts_with(sign) && line.contains(MARKER_PREFIX))
        .count()
}

/// How the file is written: a two-space indent, which is what Claude Code itself writes — so
/// a settings file it produced round-trips through this byte for byte.
fn print(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

/// A line diff good enough to read: the lines shared at each end, and everything between.
///
/// Not a general diff algorithm, and it does not need to be. The change is one insertion
/// into a file whose other lines are untouched, so a common prefix and a common suffix
/// describe it exactly. A file that is also being reformatted produces one large hunk —
/// which is the honest rendering of what is about to happen to it.
#[must_use]
pub fn diff(before: &str, after: &str) -> String {
    // No change is no diff — not three lines of context around nothing, which would render
    // as a change to a reader and to a screen that asks "apply this?".
    if before == after {
        return String::new();
    }

    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();

    let mut head = 0;
    while head < old.len() && head < new.len() && old[head] == new[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < old.len() - head
        && tail < new.len() - head
        && old[old.len() - 1 - tail] == new[new.len() - 1 - tail]
    {
        tail += 1;
    }

    /// How many unchanged lines are shown on each side of the change.
    const CONTEXT: usize = 3;

    let mut out = String::new();
    for line in &old[head.saturating_sub(CONTEXT)..head] {
        out.push_str(&format!(" {line}\n"));
    }
    for line in &old[head..old.len() - tail] {
        out.push_str(&format!("-{line}\n"));
    }
    for line in &new[head..new.len() - tail] {
        out.push_str(&format!("+{line}\n"));
    }
    for line in old[old.len() - tail..].iter().take(CONTEXT) {
        out.push_str(&format!(" {line}\n"));
    }
    out
}

// ------------------------------------------------------------------ the file

/// What a completed write left behind, for the screen that reports it.
#[derive(Debug, Clone)]
pub struct Written {
    /// Where the backup went, and the user is shown it (FR-44). Empty when the settings file
    /// did not exist and there was therefore nothing to back up.
    pub backup: PathBuf,
    /// The change that was made, as it was previewed.
    pub change: Change,
}

/// Something that stopped the write.
#[derive(Debug)]
pub enum WriteError {
    /// The product declined (FR-53).
    Refused(Refusal),
    /// The filesystem did.
    Io(std::io::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => write!(f, "{refusal}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl From<Refusal> for WriteError {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}

impl From<std::io::Error> for WriteError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Installs (`helper: Some`) or removes (`helper: None`), taking a backup first.
///
/// The order is the whole point: read, decide, **back up**, write. If any step fails, the
/// one before it changed nothing — so the settings file is either its original self, or its
/// original self with a backup beside it.
///
/// A settings file that does not exist yet is a normal first install: there is nothing to
/// lose, so the backup is skipped and the caller is told by an empty path.
///
/// # Errors
///
/// [`WriteError`] if the file will not parse (nothing is written), or if the backup or the
/// write fails.
pub fn write(path: &Path, helper: Option<&Path>, now: i64) -> Result<Written, WriteError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        // A first install writes the file the product would have found.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_owned(),
        Err(e) => return Err(e.into()),
    };
    let change = preview(&text, helper)?;

    let backup = if path.exists() {
        let backup = backup_path(path, now);
        std::fs::copy(path, &backup)?;
        backup
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        PathBuf::new()
    };

    std::fs::write(path, change.after.as_bytes())?;
    Ok(Written { backup, change })
}

/// Where a backup goes: beside the file, stamped, and never on top of an existing one.
///
/// Beside it rather than in the product's own data directory, because the user has to be
/// able to find it without us — including after uninstalling the product (FR-44).
#[must_use]
pub fn backup_path(path: &Path, now: i64) -> PathBuf {
    let stamp = stamp(now);
    let name = path.file_name().map_or_else(
        || "settings.json".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut candidate = path.with_file_name(format!("{name}.backup-{stamp}"));
    // Two installs in the same second must not leave one backup.
    let mut nth = 2;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{name}.backup-{stamp}-{nth}"));
        nth += 1;
    }
    candidate
}

/// `YYYYMMDD-HHMMSS`, in UTC.
fn stamp(now: i64) -> String {
    let t = mcv_core::time::Timestamp::from_millis(now).utc();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}
