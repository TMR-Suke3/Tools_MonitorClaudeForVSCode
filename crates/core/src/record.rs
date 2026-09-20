//! One line of a transcript, parsed defensively.
//!
//! Every format read here is a Claude Code internal (`docs/observation-sources.md`), so the
//! parser assumes it will change: an unknown record type is skipped silently (FR-38), a line
//! that will not parse is reported once and skipped rather than killing the observer
//! (FR-39), and no field is read unless it is named below.
//!
//! **The allow-list is [`Raw`], and it is the whole of it.** Anything not named there is
//! never materialised — which is how `queue-operation.content` (a full prompt),
//! `atis-latch.atis` (a credential) and `error.message` (which was measured carrying an
//! authentication failure verbatim) are dropped *unread* rather than dropped later
//! (NFR-07, TC-43). Adding a field to `Raw` is therefore a decision about what this product
//! is allowed to see, and belongs in review.
//!
//! **One thing is read outside that allow-list, on purpose, by a function of its own.**
//! [`human_prompt_line`] takes the first line of a session's opening human prompt, because
//! that is the name the editor puts on the session's tab until Claude Code has generated
//! one, and a board that showed a different name from the tab is a board the user has to
//! translate ([ADR-0030](../../../docs/adr/0030-a-session-is-named-what-its-tab-is-named.md)).
//! It is deliberately **not** a field on `Raw`: adding one there would build the text of
//! every block of every message, assistant output included. This parses a second, narrower
//! shape, and the caller asks it only until the session's **first** human prompt has been
//! seen — see [`Opening`] for why that is not the same as "until one yields a line". So the
//! text that is built is the first line of one human prompt per session, and nothing else.

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::time::Timestamp;

// ---------------------------------------------------------------- the public shape

/// A parsed transcript line: what it is, plus the fields every record type shares.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub meta: Meta,
    pub record: Record,
    /// The usage limit (FR-15). Not a record type of its own — it rides on an ordinary
    /// `assistant` record, and `status: "rejected"` is the whole signal.
    pub quota: Option<QuotaLimits>,
    /// An API failure. Its human-readable text is deliberately not captured.
    pub error: Option<ApiError>,
}

/// Fields carried by most record types, used for joining and for ordering within a turn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub session_id: Option<String>,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    /// True for a subagent's own transcript. Measured at 0 occurrences inside ordinary
    /// transcripts ([ADR-0007](../../../docs/adr/0007-detect-subagents-from-their-own-file.md)).
    pub is_sidechain: bool,
    pub at: Option<Timestamp>,
    /// The Claude Code version that wrote the line, kept for compatibility triage (NFR-05).
    pub version: Option<String>,
    pub cwd: Option<String>,
    pub entrypoint: Option<String>,
}

/// What the line is. Types this product has no use for become [`Record::Ignored`], which is
/// not an error: FR-38 requires them to be skipped silently.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// A prompt was submitted, or a tool result came back.
    User(Message),
    /// Claude produced output. The stop reason is how the end of a turn is known.
    Assistant(Message),
    System(SystemNotice),
    /// The session title shown on the board. The most recent one wins.
    AiTitle(String),
    /// A prompt was queued or dequeued — an early "work is starting" signal. Only the
    /// operation is read; the record's `content` field is the full prompt and is not.
    QueueOperation(String),
    /// The permission mode changed. A mode that never prompts cannot be waiting on one.
    Mode(String),
    /// Present only as evidence that the session is alive: the prompt text is not read.
    LastPrompt,
    /// Context attached to a turn. Evidence of activity, nothing more.
    Attachment,
    /// A type this product does not use, including types that did not exist when it was
    /// written. Carries the name only so a diagnostic can say what was skipped.
    Ignored {
        kind: Option<String>,
    },
}

/// The parts of a message the board needs: how it ended, and what blocks it held.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Message {
    pub stop_reason: StopReason,
    pub blocks: Vec<Block>,
}

impl Message {
    /// Tool calls in this message, in order.
    pub fn tool_uses(&self) -> impl Iterator<Item = (&str, &str)> {
        self.blocks.iter().filter_map(|b| match b {
            Block::ToolUse { id, name } => Some((id.as_str(), name.as_str())),
            _ => None,
        })
    }

    /// Tool results in this message, with the id they answer and whether they failed.
    pub fn tool_results(&self) -> impl Iterator<Item = (&str, bool)> {
        self.blocks.iter().filter_map(|b| match b {
            Block::ToolResult {
                tool_use_id,
                is_error,
            } => Some((tool_use_id.as_str(), *is_error)),
            _ => None,
        })
    }
}

/// A content block, reduced to its type and its joining ids. **No block carries text**:
/// `text`, `thinking`, a tool's `input` and a result's `content` are all dropped unread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text,
    Thinking,
    Image,
    ToolUse { id: String, name: String },
    ToolResult { tool_use_id: String, is_error: bool },
    Other(String),
}

/// How a turn ended.
///
/// `null` is **not** "ended without a reason": one logical turn is written as several
/// records and only the last carries a reason, so a null means mid-stream
/// (`docs/observation-sources.md` §2.3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StopReason {
    /// Absent or null — the record is part of a turn still being written.
    #[default]
    MidStream,
    EndTurn,
    ToolUse,
    StopSequence,
    Other(String),
}

/// An out-of-band notice, distinguished by its subtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemNotice {
    /// An API call failed and is being retried. Counts as activity, not as a pause.
    ApiError {
        retry_attempt: Option<u32>,
        max_retries: Option<u32>,
        retry_in_ms: Option<u64>,
    },
    /// A compaction ran. Counts as activity: the silence around it is work, not idleness.
    CompactBoundary {
        trigger: Option<String>,
        duration_ms: Option<u64>,
    },
    /// A `Stop` hook reported. Recorded because it says a hook is installed at all.
    StopHookSummary {
        prevented_continuation: Option<bool>,
    },
    Other(String),
}

/// The failure attached to an API error, without its text.
///
/// `message` and `formatted` are not captured: one of them was measured containing an
/// OAuth failure verbatim, and none of the board's behaviour depends on the wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    /// The machine-readable code, when the record carried one instead of an object —
    /// measured as `authentication_failed` and `rate_limit`.
    ///
    /// Kept only when it still looks like a code: short and with no whitespace. A future
    /// version putting a sentence here would be recorded as "an error happened" rather than
    /// quoted, because this field is not allowed to become a channel for prose (NFR-07).
    pub kind: Option<String>,
    /// HTTP status, when there was one. Absent for a transport failure.
    pub status: Option<i64>,
    /// Whether the failure was a transport-level one (`error.connection` present).
    pub is_connection: bool,
    pub is_network_down: Option<bool>,
}

/// The usage limit (FR-15), decidable from structure alone — the human-readable message
/// alongside it is not read, so nothing may key on its wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaLimits {
    pub status: Option<String>,
    /// Unix seconds at which the session may continue.
    pub resets_at: Option<i64>,
    /// The window that was exhausted — `five_hour` in every observed case.
    pub rate_limit_type: Option<String>,
}

impl QuotaLimits {
    /// The whole signal: the session has been cut off until [`Self::resets_at`].
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        self.status.as_deref() == Some("rejected")
    }
}

/// A line that could not be read as JSON at all. Distinct from an unknown record type,
/// which is not an error: this one is worth exactly one warning (FR-39, TC-39).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed {
    /// Bytes into the transcript at which the line started, so a diagnostic can point at it.
    pub offset: u64,
    pub reason: String,
}

// ---------------------------------------------------------------- the allow-list

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(rename = "parentUuid", default)]
    parent_uuid: Option<String>,
    #[serde(rename = "isSidechain", default)]
    is_sidechain: Option<bool>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    entrypoint: Option<String>,
    #[serde(default)]
    message: Option<RawMessage>,
    #[serde(rename = "aiTitle", default)]
    ai_title: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(rename = "retryAttempt", default)]
    retry_attempt: Option<u32>,
    #[serde(rename = "maxRetries", default)]
    max_retries: Option<u32>,
    #[serde(rename = "retryInMs", default)]
    retry_in_ms: Option<u64>,
    #[serde(rename = "preventedContinuation", default)]
    prevented_continuation: Option<bool>,
    #[serde(rename = "compactMetadata", default)]
    compact: Option<RawCompact>,
    #[serde(rename = "quotaLimits", default)]
    quota: Option<RawQuota>,
    #[serde(default)]
    error: Option<RawErrorField>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    content: Option<RawContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawContent {
    Blocks(Vec<RawBlock>),
    /// Content in any other shape — most often a bare string instead of an array. Read as
    /// `IgnoredAny` so the text is stepped over rather than built: this is one of the places
    /// NFR-07's "dropped unread" is literally true, and the compiler enforces it by
    /// complaining the moment the variant starts holding something.
    Opaque(IgnoredAny),
}

#[derive(Deserialize)]
struct RawBlock {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tool_use_id: Option<String>,
    #[serde(default)]
    is_error: Option<bool>,
}

/// What one transcript line turned out to be, when asked for a session's opening prompt.
///
/// **Three answers, not two, and the middle one is why.** A human prompt that yields nothing
/// usable — an image with no text, a line of spaces — has still been *seen*, and the caller
/// must stop looking. Folding it in with "not a prompt" would let the search run on to the
/// second prompt, the third, and so on until one had text: the stored line would then be "the
/// first human prompt that parsed", not the opening one, and the promise of reading one
/// prompt per session would quietly not be a promise
/// ([ADR-0030](../../../docs/adr/0030-a-session-is-named-what-its-tab-is-named.md)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opening {
    /// Not a human prompt: an assistant record, a tool result, another type, or a line that
    /// would not parse. **Nothing was read**, and the caller should keep looking.
    NotAPrompt,
    /// A human prompt with no usable first line. It was seen; there is nothing to show. The
    /// session falls back to whatever it was called before, which is what it was called
    /// before this existed at all.
    NoLine,
    /// The first line of the opening human prompt.
    Line(String),
}

/// The opening line of a human prompt (NFR-07).
///
/// **Deliberately narrow, and the narrowness is the point.** It reads a line only when the
/// record is a `user` record whose `origin.kind` is `human` — a tool result is a `user`
/// record too, and its content is not a prompt and not a name. It then takes the first text
/// block's **first line**, trimmed, and caps it by characters: a title is a label, and a
/// prompt that runs to a paragraph has said everything a label needs before its first line
/// break.
///
/// A line break is `\n` or a bare `\r`. `str::lines` does not treat a lone carriage return as
/// one, so a prompt written on a machine that uses them would have come back whole — several
/// lines held where the promise says one.
#[must_use]
pub fn human_prompt_line(line: &str, cap: usize) -> Opening {
    let Ok(raw) = serde_json::from_str::<RawHumanPrompt>(line) else {
        return Opening::NotAPrompt;
    };
    if raw.kind.as_deref() != Some("user")
        || raw.origin.and_then(|o| o.kind).as_deref() != Some("human")
    {
        return Opening::NotAPrompt;
    }

    let text = raw
        .message
        .and_then(|m| m.content)
        .into_iter()
        .flatten()
        .filter(|block| block.kind.as_deref() == Some("text"))
        .filter_map(|block| block.text)
        .find(|text| !is_injected_context(text));
    // Past this point the record *was* a human prompt, so every answer is `NoLine` rather
    // than `NotAPrompt`: it has been seen, and the caller stops looking.
    let Some(text) = text else {
        return Opening::NoLine;
    };

    let first = text.split(['\n', '\r']).next().unwrap_or("").trim();
    if first.is_empty() {
        return Opening::NoLine;
    }
    Opening::Line(first.chars().take(cap).collect())
}

/// Whether a text block is the editor's own context rather than something the user typed.
///
/// **Measured, not guessed.** VS Code puts what it wants Claude to know — the file on
/// screen, the lines selected — in a text block of its own, *before* the one the user
/// wrote:
///
/// ```text
/// content: [ {"type":"text","text":"<ide_opened_file>…</ide_opened_file>"},
///            {"type":"text","text":"…the prompt the user typed…"} ]
/// ```
///
/// Taking the first text block therefore named 41 of the 69 recorded sessions after the
/// editor's note instead of after the prompt — a board reading `<ide opened file>The user
/// ope…` where the tab reads the question. A block that leads with an `image` was already
/// skipped, because its `type` is not `text`; this is the same case arriving as text.
///
/// **The rule is the shape, not the tag names.** A block is context when removing every
/// complete `<tag>…</tag>` from it leaves nothing behind. Listing `ide_opened_file` and
/// `ide_selection` — the only two in the corpus — would break again the day a third is
/// added, and this is a Claude Code internal with no stability promise. A prompt that is
/// *entirely* one XML element and nothing else would be misread, and is a trade worth
/// making: nobody names a session that way, and the cost is a fallback to the previous
/// name rather than a wrong one.
fn is_injected_context(text: &str) -> bool {
    let mut rest = text.trim();
    let mut stripped_any = false;
    while let Some(after) = strip_one_element(rest) {
        rest = after.trim();
        stripped_any = true;
    }
    stripped_any && rest.is_empty()
}

/// Removes one complete `<tag>…</tag>` from the front, or answers `None`.
///
/// Deliberately not an XML parser: it matches a closing tag with the same name as the
/// opening one and nothing more. Anything it cannot make sense of is left alone, which
/// leaves the text looking like a prompt — the safe direction to fail in.
fn strip_one_element(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('<')?;
    let (name, rest) = rest.split_once('>')?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return None;
    }
    let close = format!("</{name}>");
    let at = rest.find(&close)?;
    Some(&rest[at + close.len()..])
}

/// The second, narrower shape [`human_prompt_line`] reads. Nothing else uses it, and nothing
/// else may: this is the one place the product looks at what the user wrote.
#[derive(Deserialize)]
struct RawHumanPrompt {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    origin: Option<RawOrigin>,
    #[serde(default)]
    message: Option<RawHumanMessage>,
}

#[derive(Deserialize)]
struct RawOrigin {
    #[serde(default)]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct RawHumanMessage {
    #[serde(default)]
    content: Option<Vec<RawTextBlock>>,
}

#[derive(Deserialize)]
struct RawTextBlock {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct RawCompact {
    #[serde(default)]
    trigger: Option<String>,
    #[serde(rename = "durationMs", default)]
    duration_ms: Option<u64>,
}

#[derive(Deserialize)]
struct RawQuota {
    #[serde(default)]
    status: Option<String>,
    #[serde(rename = "resetsAt", default)]
    resets_at: Option<i64>,
    #[serde(rename = "rateLimitType", default)]
    rate_limit_type: Option<String>,
}

/// `error` occurs in two shapes at the top level of a record: an object, and a bare code
/// string. Accepting both is FR-38 in miniature — a format that widened under us must cost
/// the fields it changed, not the whole line.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawErrorField {
    Structured(RawError),
    Code(String),
}

#[derive(Deserialize)]
struct RawError {
    #[serde(default)]
    status: Option<i64>,
    /// Presence only. The object holds a driver message; `IgnoredAny` reads past it without
    /// building it.
    #[serde(default)]
    connection: Option<IgnoredAny>,
    #[serde(rename = "isNetworkDown", default)]
    is_network_down: Option<bool>,
}

// ---------------------------------------------------------------- parsing

/// Parses one transcript line.
///
/// `offset` is the byte position the line started at, carried only so that a [`Malformed`]
/// report can name it. An unknown record type is **not** an error — it comes back as
/// [`Record::Ignored`].
///
/// # Errors
///
/// Returns [`Malformed`] when the line is not valid JSON, or is valid JSON that is not an
/// object. Both are skipped by the caller, with one warning.
pub fn parse_line(line: &str, offset: u64) -> Result<Entry, Malformed> {
    let raw: Raw = serde_json::from_str(line).map_err(|e| Malformed {
        offset,
        reason: e.to_string(),
    })?;

    let meta = Meta {
        session_id: raw.session_id,
        uuid: raw.uuid,
        parent_uuid: raw.parent_uuid,
        is_sidechain: raw.is_sidechain.unwrap_or(false),
        at: raw.timestamp.as_deref().and_then(Timestamp::parse_iso8601),
        version: raw.version,
        cwd: raw.cwd,
        entrypoint: raw.entrypoint,
    };

    let record = match raw.kind.as_deref() {
        Some("user") => Record::User(message(raw.message)),
        Some("assistant") => Record::Assistant(message(raw.message)),
        Some("system") => Record::System(notice(
            raw.subtype.as_deref(),
            raw.retry_attempt,
            raw.max_retries,
            raw.retry_in_ms,
            raw.prevented_continuation,
            raw.compact,
        )),
        Some("ai-title") => match raw.ai_title {
            Some(t) => Record::AiTitle(t),
            None => Record::Ignored { kind: raw.kind },
        },
        Some("queue-operation") => match raw.operation {
            Some(op) => Record::QueueOperation(op),
            None => Record::Ignored { kind: raw.kind },
        },
        Some("mode") => match raw.mode {
            Some(m) => Record::Mode(m),
            None => Record::Ignored { kind: raw.kind },
        },
        Some("last-prompt") => Record::LastPrompt,
        Some("attachment") => Record::Attachment,
        _ => Record::Ignored { kind: raw.kind },
    };

    Ok(Entry {
        meta,
        record,
        quota: raw.quota.map(|q| QuotaLimits {
            status: q.status,
            resets_at: q.resets_at,
            rate_limit_type: q.rate_limit_type,
        }),
        error: raw.error.map(|e| match e {
            RawErrorField::Structured(e) => ApiError {
                kind: None,
                status: e.status,
                is_connection: e.connection.is_some(),
                is_network_down: e.is_network_down,
            },
            RawErrorField::Code(code) => ApiError {
                kind: code_like(code),
                status: None,
                is_connection: false,
                is_network_down: None,
            },
        }),
    })
}

fn message(raw: Option<RawMessage>) -> Message {
    let Some(raw) = raw else {
        return Message::default();
    };
    let stop_reason = match raw.stop_reason.as_deref() {
        None => StopReason::MidStream,
        Some("end_turn") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("stop_sequence") => StopReason::StopSequence,
        Some(other) => StopReason::Other(other.to_owned()),
    };
    let blocks = match raw.content {
        None => Vec::new(),
        Some(RawContent::Opaque(_)) => vec![Block::Text],
        Some(RawContent::Blocks(blocks)) => blocks.into_iter().map(block).collect(),
    };
    Message {
        stop_reason,
        blocks,
    }
}

fn block(raw: RawBlock) -> Block {
    match raw.kind.as_deref() {
        Some("text") => Block::Text,
        Some("thinking") | Some("redacted_thinking") => Block::Thinking,
        Some("image") => Block::Image,
        Some("tool_use") => Block::ToolUse {
            id: raw.id.unwrap_or_default(),
            name: raw.name.unwrap_or_default(),
        },
        Some("tool_result") => Block::ToolResult {
            tool_use_id: raw.tool_use_id.unwrap_or_default(),
            is_error: raw.is_error.unwrap_or(false),
        },
        Some(other) => Block::Other(other.to_owned()),
        None => Block::Other(String::new()),
    }
}

fn notice(
    subtype: Option<&str>,
    retry_attempt: Option<u32>,
    max_retries: Option<u32>,
    retry_in_ms: Option<u64>,
    prevented_continuation: Option<bool>,
    compact: Option<RawCompact>,
) -> SystemNotice {
    match subtype {
        Some("api_error") => SystemNotice::ApiError {
            retry_attempt,
            max_retries,
            retry_in_ms,
        },
        Some("compact_boundary") => SystemNotice::CompactBoundary {
            trigger: compact.as_ref().and_then(|c| c.trigger.clone()),
            duration_ms: compact.as_ref().and_then(|c| c.duration_ms),
        },
        Some("stop_hook_summary") => SystemNotice::StopHookSummary {
            prevented_continuation,
        },
        Some(other) => SystemNotice::Other(other.to_owned()),
        None => SystemNotice::Other(String::new()),
    }
}

/// A bare `error` string is kept only while it still looks like a machine code. The moment
/// it looks like a sentence it is dropped, because this product does not quote error text
/// (NFR-07) and nothing in the state model keys on wording.
fn code_like(code: String) -> Option<String> {
    let plausible = !code.is_empty() && code.len() <= 64 && !code.contains(char::is_whitespace);
    plausible.then_some(code)
}
