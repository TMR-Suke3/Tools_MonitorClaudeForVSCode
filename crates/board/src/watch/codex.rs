//! Read-only observation of recent Codex IDE turns from local rollout files.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use mcv_core::time::Timestamp;
use serde::Deserialize;

use super::tail::Tail;

const DISCOVERY_MS: i64 = 2_000;
const WORKING_MS: i64 = 30 * 60_000;
const COMPLETE_MS: i64 = 5 * 60_000;
pub const ROW_PREFIX: &str = "codex:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    AwaitingUser,
    Working,
    Complete,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub activity: Activity,
    pub title: Option<String>,
    pub at: i64,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    SessionMeta {
        payload: Metadata,
    },
    EventMsg {
        timestamp: String,
        payload: Event,
    },
    ResponseItem {
        timestamp: String,
        payload: Response,
    },
}

#[derive(Deserialize)]
struct Metadata {
    id: String,
    cwd: String,
    source: String,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    turn_id: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    FunctionCall { name: String, call_id: String },
    FunctionCallOutput { call_id: String, output: String },
    Message { role: String, content: Vec<Content> },
}

#[derive(Deserialize)]
struct Content {
    text: Option<String>,
}

struct Question {
    at: i64,
    asynchronous: bool,
}

#[derive(Deserialize)]
struct Acknowledgement {
    accepted: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuestionReply {
    question_item_id: String,
}

struct Rollout {
    tail: Tail,
    metadata: Option<Metadata>,
    turn: Option<(String, Activity, i64)>,
    rejected: bool,
    questions: HashMap<String, Question>,
}

impl Rollout {
    fn update(&mut self) {
        let Ok(polled) = self.tail.poll() else {
            return;
        };
        if polled.restarted {
            self.metadata = None;
            self.turn = None;
            self.questions.clear();
            self.rejected = false;
        }
        for (_, line) in polled.lines {
            if self.rejected {
                break;
            }
            match serde_json::from_str::<Record>(&line) {
                Ok(Record::SessionMeta { payload }) if self.metadata.is_none() => {
                    if payload.source == "vscode"
                        && valid_id(&payload.id)
                        && !payload.cwd.is_empty()
                    {
                        self.metadata = Some(payload);
                    } else {
                        self.rejected = true;
                    }
                }
                Ok(Record::ResponseItem { timestamp, payload }) if self.metadata.is_some() => {
                    self.observe_response(&timestamp, payload);
                }
                Ok(Record::EventMsg { timestamp, payload }) if self.metadata.is_some() => {
                    let Some(at) = rollout_timestamp(&timestamp) else {
                        continue;
                    };
                    match payload.kind.as_str() {
                        "task_started" => {
                            if self
                                .turn
                                .as_ref()
                                .is_none_or(|(id, _, _)| *id != payload.turn_id)
                            {
                                self.questions.clear();
                                self.turn = Some((payload.turn_id, Activity::Working, at));
                            }
                        }
                        "task_complete" | "turn_aborted" => {
                            if let Some((id, activity, since)) = &mut self.turn
                                && *id == payload.turn_id
                                && *activity == Activity::Working
                            {
                                *activity = Activity::Complete;
                                *since = at;
                                self.questions.clear();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn observe_response(&mut self, timestamp: &str, response: Response) {
        match response {
            Response::FunctionCall { name, call_id }
                if matches!(
                    name.as_str(),
                    "request_user_input" | "request_user_input_async"
                ) =>
            {
                if let Some((_, Activity::Working, started)) = self.turn.as_ref()
                    && let Some(at) = rollout_timestamp(timestamp)
                    && at >= *started
                {
                    self.questions.entry(call_id).or_insert(Question {
                        at,
                        asynchronous: name == "request_user_input_async",
                    });
                }
            }
            Response::FunctionCallOutput { call_id, output } => {
                let accepted =
                    serde_json::from_str::<Acknowledgement>(&output).is_ok_and(|ack| ack.accepted);
                if self
                    .questions
                    .get(&call_id)
                    .is_some_and(|q| !q.asynchronous || !accepted)
                {
                    self.questions.remove(&call_id);
                }
            }
            Response::Message { role, content } if role == "user" && !self.questions.is_empty() => {
                for text in content.iter().filter_map(|part| part.text.as_deref()) {
                    if let Some((_, reply)) = text.split_once("<send_user_message_question_reply>")
                        && let Some((reply, _)) =
                            reply.split_once("</send_user_message_question_reply>")
                    {
                        let Ok(replies) = serde_json::from_str::<Vec<QuestionReply>>(reply) else {
                            continue;
                        };
                        for reply in replies {
                            if let Ok((name, id, _)) = serde_json::from_str::<(String, String, usize)>(
                                &reply.question_item_id,
                            ) && name == "request_user_input_async"
                                && self.questions.get(&id).is_some_and(|q| q.asynchronous)
                            {
                                self.questions.remove(&id);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn row(&mut self, now: i64) -> Option<Session> {
        let metadata = self.metadata.as_ref()?;
        self.questions
            .retain(|_, q| now.saturating_sub(q.at) < WORKING_MS);
        if let Some(at) = self
            .questions
            .values()
            .map(|q| q.at)
            .filter(|at| *at <= now)
            .min()
        {
            return Some(Session {
                id: metadata.id.clone(),
                cwd: metadata.cwd.clone(),
                activity: Activity::AwaitingUser,
                title: None,
                at,
            });
        }
        let (_, activity, at) = self.turn.as_ref()?;
        let limit = match activity {
            Activity::Working | Activity::AwaitingUser => WORKING_MS,
            Activity::Complete => COMPLETE_MS,
        };
        (0..limit)
            .contains(&now.saturating_sub(*at))
            .then(|| Session {
                id: metadata.id.clone(),
                cwd: metadata.cwd.clone(),
                activity: *activity,
                title: None,
                at: *at,
            })
    }
}

pub struct CodexWatch {
    root: PathBuf,
    rollouts: HashMap<PathBuf, Rollout>,
    last_scan: Option<i64>,
    title_index: Option<Tail>,
    titles: HashMap<String, String>,
}

impl CodexWatch {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let title_index = root
            .parent()
            .map(|parent| Tail::new(parent.join("session_index.jsonl")));
        Self {
            root,
            title_index,
            titles: HashMap::new(),
            rollouts: HashMap::new(),
            last_scan: None,
        }
    }

    pub fn poll(&mut self, now: i64) -> Vec<Session> {
        if self
            .last_scan
            .is_none_or(|last| now < last || now.saturating_sub(last) >= DISCOVERY_MS)
        {
            self.last_scan = Some(now);
            let oldest = now.saturating_sub(WORKING_MS);
            let mut candidates = Vec::new();
            recent_rollouts(&self.root, oldest, 0, &mut candidates);
            self.rollouts.retain(|path, _| candidates.contains(path));
            for path in candidates {
                self.rollouts
                    .entry(path.clone())
                    .or_insert_with(|| Rollout {
                        tail: Tail::new(path),
                        metadata: None,
                        turn: None,
                        rejected: false,
                        questions: HashMap::new(),
                    });
            }
        }

        let mut sessions = BTreeMap::<String, Session>::new();
        for rollout in self.rollouts.values_mut() {
            // A CLI rollout can be discarded without examining any subsequent content.
            if !rollout.rejected {
                rollout.update();
            }
            if let Some(row) = rollout.row(now) {
                let entry = sessions
                    .entry(row.id.clone())
                    .or_insert_with(|| row.clone());
                if row.at > entry.at || (row.at == entry.at && row.activity == Activity::Complete) {
                    *entry = row;
                }
            }
        }
        if let Some(index) = &mut self.title_index
            && let Ok(polled) = index.poll()
        {
            if polled.restarted {
                self.titles.clear();
            }
            for (_, line) in polled.lines {
                if let Ok(title) = serde_json::from_str::<IndexTitle>(&line)
                    && valid_id(&title.id)
                {
                    let name = title.thread_name.lines().next().unwrap_or_default().trim();
                    if name.is_empty() {
                        self.titles.remove(&title.id);
                    } else {
                        self.titles
                            .insert(title.id, name.chars().take(120).collect());
                    }
                }
            }
        }
        sessions
            .into_values()
            .map(|mut session| {
                session.title = self.titles.get(&session.id).cloned();
                session
            })
            .collect()
    }
}

#[derive(Deserialize)]
struct IndexTitle {
    id: String,
    thread_name: String,
}

/// Only a distinct workspace title is enough to select a window.
pub fn matching_window<'a>(
    workspace: &str,
    titles: impl Iterator<Item = &'a str>,
) -> Option<usize> {
    let mut matches = titles.enumerate().filter(|(_, title)| {
        title
            .strip_suffix(" - Visual Studio Code - Insiders")
            .or_else(|| title.strip_suffix(" - Visual Studio Code"))
            .is_some_and(|context| {
                context.eq_ignore_ascii_case(workspace)
                    || context
                        .get(context.len().saturating_sub(workspace.len() + 3)..)
                        .is_some_and(|suffix| {
                            suffix
                                .strip_prefix(" - ")
                                .is_some_and(|name| name.eq_ignore_ascii_case(workspace))
                        })
            })
    });
    let (index, _) = matches.next()?;
    matches.next().is_none().then_some(index)
}

#[must_use]
pub fn sessions_dir() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join(".codex"))
        })?;
    Some(home.join("sessions"))
}

fn recent_rollouts(dir: &Path, oldest: i64, depth: usize, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth < 3 {
            recent_rollouts(&entry.path(), oldest, depth + 1, paths);
        } else if kind.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            && entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|age| i64::try_from(age.as_millis()).ok())
                .is_some_and(|modified| modified >= oldest)
        {
            paths.push(entry.path());
        }
    }
}

fn rollout_timestamp(s: &str) -> Option<i64> {
    let utc = s.strip_suffix('Z')?;
    let (seconds, fraction) = utc.split_once('.').unwrap_or((utc, ""));
    if !fraction.bytes().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let fraction: String = fraction.chars().chain(['0'; 3]).take(3).collect();
    Timestamp::parse_iso8601(&format!("{seconds}.{fraction}Z")).map(Timestamp::as_millis)
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, byte)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

#[must_use]
pub fn session_uri(scheme: &str, id: &str) -> Option<String> {
    (matches!(scheme, "vscode" | "vscode-insiders") && valid_id(id))
        .then(|| format!("{scheme}://openai.chatgpt/local/{id}"))
}
