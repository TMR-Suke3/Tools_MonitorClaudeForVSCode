# Requirements

> Status: **draft**. Nothing in this document is implemented yet.
> Requirement levels follow RFC 2119: **MUST**, **SHOULD**, **MAY**. They describe the
> product when it is complete — **§11 says which of them the first release has to
> satisfy**, which is a much shorter list.

Related documents:

- [session-state-model.md](session-state-model.md) — what each status means and how it is decided
- [observation-sources.md](observation-sources.md) — where the data comes from
- [ui-overlay.md](ui-overlay.md) — how the overlay looks and behaves
- [hook-setup.md](hook-setup.md) — the assisted setup for exact status detection
- [adr/](adr/) — why the key decisions were made

---

## 1. Purpose

Give a developer who runs **many Claude Code for VS Code sessions at once** a single,
always-visible answer to one question:

> *Which of my sessions needs me right now?*

The product is a small always-on-top overlay window — comparable to the SteamVR status
window — that shows one coloured indicator per live Claude Code session, grouped by the
workspace folder it belongs to.

## 2. Background

Developers commonly keep several VS Code windows open, each on a different repository, and
often more than one Claude Code session inside a single window.

VS Code cannot show all of those sessions at the same time:

- A session that is not the active tab / panel is invisible.
- Forcing every session onto one screen produces an unusable layout.
- A session that finished, crashed, or is waiting for a permission answer stays silently
  blocked until the developer happens to look at it.

The cost is not the missing information; it is the **polling the developer has to do by
hand** — cycling through windows to find out who is idle. This project removes that
polling.

## 3. Scope

### 3.1 In scope

- Observing Claude Code sessions started from the **VS Code extension** on the local
  machine (`entrypoint` = `claude-vscode`).
- Deriving a coarse status per session (see [session-state-model.md](session-state-model.md)).
- Showing those statuses in an always-on-top, freely positioned overlay window.
- One display density — indicator, short title, elapsed time — in which each workspace group
  **folds to a single row** and expands again, so the board's height is the user's choice
  ([ADR-0012](adr/0012-one-density-that-folds.md)).

### 3.2 Out of scope (for now)

- Sessions started from a plain terminal (`claude` CLI), other IDEs, the web app, or
  another machine. The internal model **SHOULD** stay general enough to add them later,
  but the product does not show them.
- Reading, replaying, or archiving conversation content. This is a *status* board, not a
  transcript viewer or an activity log.
- Sending prompts to, controlling, or interrupting a session from the overlay.
- Any network communication. See [NFR-08](#7-non-functional-requirements).
- Team / multi-machine aggregation.
- **Remaining-quota display** ("you have *n* % of the weekly limit left"). Considered and
  declined on measurement: the amount consumed is fully observable locally — 8,786 records
  carry per-turn token counts — but nothing on disk publishes the limit those counts run
  against. A quota record appears only at the moment a request is *rejected*, and even then
  states the reset time and nothing about headroom. The denominator exists only on the
  server, so the feature would cost an outbound request that NFR-08 forbids, plus a read of
  the stored credential. What the product does instead is FR-15: say precisely when a
  session is cut off and when it comes back.
- **An early warning that a limit is near.** Wanted, and not possible from local data: no
  record of any type precedes the rejection. The board reports the wall, not the approach.

## 4. Definitions

| Term | Meaning |
|---|---|
| **Session** | One Claude Code conversation, identified by its `sessionId` (UUID). |
| **Workspace** | The folder a session runs in (`cwd`). Sessions are grouped by workspace. |
| **Workspace group** | One row group in the overlay; the unit the user thinks of as "that VS Code window". |
| **Status** | The coarse state of a session (working / waiting / …), see the state model. |
| **Session title** | The short AI-generated name of the conversation, in the user's own language. |
| **Board** | The overlay window itself, i.e. the whole product UI. |
| **Observer** | The part of the product that reads local state and produces status events. |

## 5. Users and stories

Primary (and, for now, only) user: **a solo developer driving 2–10 concurrent Claude Code
sessions on one desktop.**

- **S-1** — As a developer, I glance at a corner of my screen and immediately see that
  session *X* is waiting for a permission answer, so I switch to it instead of discovering
  it five minutes later.
- **S-2** — As a developer, I see at a glance which sessions are still working, so I do not
  interrupt them and do not sit watching a session that is going to run for two more
  minutes.
- **S-3** — As a developer, I see which sessions have stopped and how long ago, so I can
  decide which one to go back to. The board does not tell me whether the work was
  finished; that is deliberate ([ADR-0005](adr/0005-no-goal-achievement-status.md)).
- **S-4** — As a developer, I see that a session stopped abnormally — an error killed the
  turn, or the process died — so I do not wait indefinitely for output that will never
  come.
- **S-5** — As a developer, I see that I have hit the usage limit, so I stop queueing work
  and do something else.
- **S-6** — As a developer, I drag the board to a free corner of any monitor, fold away the
  workspaces I am not working in, and forget about it; it stays where I put it, folded the way
  I left it, across restarts.

## 6. Functional requirements

### 6.1 Discovery

- **FR-01** The product **MUST** discover every live Claude Code session on the local
  machine without the user registering anything by hand.
- **FR-02** It **MUST** show only sessions whose `entrypoint` marks them as VS Code
  extension sessions. Other entrypoints **MUST** be ignored by default. Note that
  `entrypoint` is **inherited from the environment, not derived from how the process was
  launched** (measured): a CLI run started from inside a VS Code session's shell also
  reports `claude-vscode`. The filter is therefore a strong hint, not proof, and the
  requirement is "sessions that identify themselves as VS Code sessions" — not "every and
  only extension-owned session".
- **FR-03** It **MUST** detect sessions that start, end, or change status **while the board
  is already running**, with the latency of [NFR-01](#7-non-functional-requirements).
- **FR-04** It **MUST** group sessions by workspace, and label each group with the
  workspace folder's own name (its last path segment), not the full path.
- **FR-05** It **MUST** handle several sessions inside one workspace group, and several
  workspace groups at the same time.
- **FR-06** A session that is not running **MUST NOT** occupy space on the board, with the
  single exception of the abnormal-stop status (see FR-59).
- **FR-07** Two VS Code windows opened on the *same* folder **MUST** be presented as one
  group. Distinguishing them is explicitly not attempted — see the limitation in
  [observation-sources.md](observation-sources.md).

### 6.2 Status

- **FR-08** Every displayed session **MUST** carry exactly one status from the set defined
  in [session-state-model.md](session-state-model.md).
- **FR-09** Status **MUST** be derived from locally observable facts only, with no network
  call and no model invocation.
- **FR-10** When the available evidence does not identify a status, the session **MUST**
  fall back to a defined neutral status rather than guessing or disappearing.
- **FR-11** The product **MUST NOT** claim to know whether a stopped session achieved what
  it was asked to do. A stopped session has one status, and no wording in the UI may
  suggest a verdict on the work ([ADR-0005](adr/0005-no-goal-achievement-status.md)).
- **FR-12** The board **MUST** show how long a session has been stopped, since that is the
  part of "is this finished?" that can be answered honestly. It **MUST NOT** ask the user
  to classify sessions by hand.
- **FR-13** "Waiting for the user" **MUST** cover the cases where Claude Code has put a
  blocking UI in front of the user (tool permission request, question prompt, plan
  approval) — i.e. cases where the session cannot progress until the user acts.
- **FR-14** The abnormal-stop status **MUST** cover both ways work stops without anyone
  asking it to: an unrecovered error ending the turn while the session is still running,
  and the session's process disappearing mid-work. The first is the common case and the
  reason the status exists — a session that died halfway looks exactly like one that
  finished, so the user waits for output that will never arrive.
- **FR-58** An error that is still being retried **MUST NOT** be reported as an abnormal
  stop; retrying is activity. Only an exhausted or unrecovered error qualifies.
- **FR-59** Where the process is gone, the status **MUST** stay visible after the session
  has disappeared, until the user dismisses it or a configurable timeout expires (default:
  10 minutes), because there is otherwise nothing left for the user to notice.
- **FR-15** The usage-limit status **MUST** clear itself automatically once the session
  resumes, and **MUST** show the reset time, which is published exactly. Detection **MUST**
  be structural — a `rejected` quota record, never a match against the wording of a message
  ([observation-sources.md](observation-sources.md) §2.3). The product **MUST NOT** claim to
  know how much quota is left: the amount consumed is observable, the limit it counts
  against is not, and a figure derived from only one of the two would be a guess presented
  as a measurement.
- **FR-56** Context compaction **MUST** be treated as activity: a session that is compacting
  reads as working, not as stopped.
- **FR-57** A session that has dispatched a subagent **MUST** read as working for as long as
  that subagent is active. Subagents **MUST NOT** appear as sessions of their own — the
  board shows what the user opened, not what Claude spawned.

### 6.3 Presentation

- **FR-16** Status **MUST** be encoded primarily by colour, following the palette in
  [ui-overlay.md](ui-overlay.md) §3.2 — which also states, per pair, the separation the
  palette has to keep under greyscale and under the common colour-vision deficiencies.
  [session-state-model.md](session-state-model.md) §1 carries the same colours alongside
  their meanings.
- **FR-17** The palette **MUST** carry the status on its own, and **MUST** be measured
  doing it: every pair keeps the separation §3.3 states for it under greyscale and under
  simulated protanopia, deuteranopia and tritanopia. Any status distinguished by **motion**
  **MUST** have a static substitute for reduced-motion mode. The **shape** of the mark
  **MUST NOT** encode status — it is reserved for hierarchy, so that a workspace's mark and
  its sessions' marks can be told apart (§3.1). The single exception is `unknown`, which
  keeps an outline of its own because it **MUST** carry the least ink of the six: it is an
  admission that the observer cannot read the session, and must not read as an alarm.

  *Amended 2026-08-27, and it is a real loosening.* This required a second, non-colour cue
  per status and forbade two statuses sharing a silhouette. Six silhouettes turned out to be
  six things to learn, and the board is glanced at rather than studied — the shapes were
  costing more than the redundancy was worth. What makes the loosening affordable is that
  the palette was **derived** to carry this alone rather than picked
  ([ADR-0010](adr/0010-derive-the-palette-from-a-luminance-ladder.md)), and TC-49 … TC-51
  measure it on every run: the second channel was insurance on a number that is already
  checked. What is genuinely lost is defence in depth — a display, a screenshot pipeline or
  a condition that mangles hue in a way the three simulations do not model now has nothing
  behind it. ([ADR-0024](adr/0024-colour-carries-the-status-shape-carries-the-hierarchy.md).)
- **FR-18** The board has **one** density: every session is a row carrying an indicator, the
  title truncated to fit, and elapsed time. Instead of a second density it **MUST** be able to
  **fold**: a workspace group **MUST** collapse to a single row and expand again in one
  action, and the fold state **MUST** be remembered per group (FR-28). A folded group
  **MUST** still show its roll-up status, so folding hides *which* session needs the user and
  never *whether* one does. A workspace with a single session **MUST** be drawn as one row
  rather than a group header above a row. The board **MUST** keep its own height bounded: past
  a configurable row count it folds groups automatically, least urgent first, and **MUST NOT**
  fold a group containing a session that is waiting for the user. A **fully folded** board
  **MUST** also narrow: it **MUST NOT** draw any session title, and its width **MUST** follow
  its content rather than a constant, so the least the board can occupy is one row per
  workspace at the width of the longest workspace name.
  ([ADR-0012](adr/0012-one-density-that-folds.md) — this replaces the earlier
  minimal / compact pair.)
- **FR-19** Session titles **MUST** be displayed as Claude Code produced them. The product
  **MUST NOT** summarise, rewrite, translate, strip words from, or otherwise shorten a
  title: doing that well needs a model, and this product does not use one
  ([ADR-0005](adr/0005-no-goal-achievement-status.md) applies the same reasoning to
  statuses).

  *Clarified 2026-08-28.* Claude Code produces more than one name for a session, and this now
  says which: **the one the editor puts on the session's tab**. Measured, that is the
  generated `ai-title` once there is one and the session's opening human prompt until then —
  never `sessions/<pid>.json`'s `name`, which is a slug (`notes-e8`) the editor does not show
  ([ADR-0030](adr/0030-a-session-is-named-what-its-tab-is-named.md)). The board showing a
  different name from the tab six inches away is a translation the user should not have to
  do. Nothing here is summarised or rewritten: the opening prompt is taken as its first line
  and truncated to fit, which is FR-20.
- **FR-20** Where a title does not fit the available width, it **MUST** be truncated
  mechanically — by *rendered width*, not character count, so East Asian full-width text is
  not clipped mid-glyph — and nothing else.
- **FR-21** Truncation **MUST** respect grapheme clusters (no broken surrogate pairs,
  no split combining sequences) and **MUST** mark the cut with an ellipsis.
- **FR-22** The full title **MUST** remain reachable on demand (the hover tooltip), so
  truncation never destroys information.
- **FR-23** The order of groups and sessions **MUST** be stable: a status change alone
  **MUST NOT** reorder the board. Urgency is expressed by colour and motion, not by
  position, so the user can build muscle memory for where each session sits.
- **FR-24** The board **SHOULD** show how long the session has been in its current status,
  at a coarse granularity (seconds → minutes → hours).
- **FR-25** The UI's own strings **MUST** be externalised and translatable, and Japanese and
  English **MUST** both be supported. Session titles are user content and are passed through
  untranslated, so the font stack **MUST** render CJK correctly.

### 6.4 Window behaviour

- **FR-26** The board **MUST** be a standalone always-on-top window, independent of any
  VS Code window, and **MUST** stay on top of full-screen-maximised editors.
- **FR-27** The board **MUST** be draggable to any position on any connected display, by
  dragging its body (it has no title bar).
- **FR-28** Position, display assignment, fold state, **the size the board is drawn at, the
  global shortcuts, and whether a click also opens the session's own tab** **MUST** persist
  across restarts, and the position **MUST** be restored to a visible one if the display it
  was on no longer exists — **at any time, not only at the next start**.

  *Amended 2026-09-06.* The restore ran once, in the startup path. That met the letter of this
  requirement and missed what it is for: a display can be unplugged, put to sleep, or taken
  away by a remote session while the board is running, and a frameless window with no taskbar
  button is then somewhere no mouse can go. What *visible* means is stated with it —
  **reachable**, meaning enough of the board overlaps some display's work area to be seen and
  taken hold of. Not *entirely on one display*, which would move a board its owner had
  deliberately straddled across two or parked half over an edge.

  *Amended 2026-08-28.* "Density" left this list when the second density did
  ([ADR-0012](adr/0012-one-density-that-folds.md)); the size and the shortcuts joined it with
  the board's menu, and the session-reveal preference with
  [ADR-0029](adr/0029-reveal-the-session-through-the-editors-url-handler.md). What the list is
  **for** has not moved: it is the complete statement of what the board writes down, and
  NFR-07 is checked against it — nothing here is session content, and a workspace key
  belonging to a fold state is the most any entry says about what the user is working on. The
  newest entry is a boolean, which says less than any of the others.
- **FR-29** The board **MUST** behave correctly under mixed-DPI multi-monitor setups.
- **FR-30** The board **MUST NOT** steal focus. Appearing, updating, or changing status
  **MUST NOT** interrupt what the user is typing.
- **FR-31** The board **SHOULD** support an adjustable opacity and a click-through mode, so
  it can sit over other content without getting in the way.
- **FR-61** The global shortcuts **MUST** be changeable and **MUST** be clearable from inside
  the product, and what became of each **MUST** be reported per key. A key that another
  application already holds **MUST** say so; a key the user cleared **MUST NOT** be presented
  as a fault.

  *Added 2026-09-06.* The two accelerators had been settings with no way to edit them. Where
  one of them collides with a key the user had already given to something else, pressing it
  opens that other application while the board's menu calls the shortcut dead — and the only
  remedy was to hand-edit a JSON file the product never mentions. Whether a key is free is
  not decidable from the key: the board asks the operating system for it and reports what
  came back ([ADR-0033](adr/0033-a-key-that-is-taken-is-a-choice-to-undo.md)).
- **FR-60** **One board per machine.** A second launch **MUST NOT** start a second board; it
  **MUST** reach the one already running, and what it asks of it is the summons of FR-32.

  *Added 2026-09-06, from a running desk.* Two boards had been open for some time without
  either of them saying so. They restore to the same remembered position and are drawn on top
  of each other, they write the same settings file, and only the first can register the
  global shortcuts — so the second reports them as "not registered" while the keys work
  perfectly well, for the other board. Every symptom of that points somewhere other than its
  cause ([ADR-0032](adr/0032-one-board-per-machine.md)).

  *What is delivered, precisely.* A launch that finds the board **running** is stopped, and
  that is every launch a person makes. A launch that arrives while the board is itself
  starting — inside the sub-millisecond gap between the guard being taken and the channel that
  carries the summons existing — is **not** stopped, and becomes a second board. Two processes
  have to reach the same instruction within that gap, which a double-click cannot do and
  simultaneous launches can. The hole is in the mechanism rather than in this requirement, it
  is described in ADR-0032, and closing it locally would mean a second synchronisation scheme
  of our own beside the one already there. It is named here rather than left for someone to
  discover, because a **MUST NOT** that is quietly 99.99 % true is worse than one that says
  where it ends.
- **FR-32** The board **SHOULD** be reachable from a tray icon when hidden, and **SHOULD**
  offer start-on-login as an opt-in.

  *The tray is met* (`ui-overlay.md` §5): a left-click **brings the board to the display the
  pointer is on**, or takes it away when it is already in front of the user, the icon carries
  the board's own menu, and it is drawn in the board's roll-up colour so that a hidden board
  still answers "is anyone waiting for me?". It stopped being a convenience the day
  `Ctrl+Alt+B` shipped — a shortcut that hides the board leaves the only way back inside the
  user's memory.

  *Amended 2026-09-06.* The click was a plain show / hide, which is what a tray icon usually
  does and is not enough for this one: showing a window positioned on a display that no longer
  exists shows nothing, so the product's only recovery route was itself a silent no-op. The
  icon now decides from **where the board is**
  ([ADR-0031](adr/0031-one-corner-for-every-summons.md)), and its menu **MUST** say which of
  the three states that is — hidden, off every display, or on a named display. They cannot be
  told apart by looking and each has a different repair, so a user who cannot see the board
  cannot otherwise report what is wrong with it. **Start-on-login is not built**: it means writing to the user's `Run`
  key, and this product does not touch the user's configuration without the
  explain-consent-undo flow FR-54 requires. §11 is where that is decided, as for everything
  else.

### 6.5 Interaction

- **FR-33** Clicking a session **MUST** bring the VS Code window that owns it to the
  foreground. Two limits are part of the requirement rather than defects against it:
  raising a window cannot select *which session inside it* is shown, and where two windows
  have the same folder open the session data does not say which of them owns the session
  (limitation L-1), so either may be raised. A window with **no folder open** carries nothing
  to match on at all and is named by elimination among the editor's own windows
  ([ADR-0028](adr/0028-name-the-last-window-by-elimination.md)); where that leaves more than
  one candidate (L-7) nothing is raised, and the board says so.

  *Extended 2026-08-28, as an opt-in.* The first limit above — that raising cannot select
  which session inside the window is shown — **can** be lifted where the editor exposes a way
  to ask for one, and Claude Code's VS Code extension does: a URL naming the session reveals
  its tab and puts the cursor in its message box
  ([ADR-0029](adr/0029-reveal-the-session-through-the-editors-url-handler.md)). It is **off by
  default** and stays a limit of the requirement rather than a promise of it, because the
  editor decides which of its windows answers the URL and the board cannot tell it, and a
  session sent to a window whose workspace it does not belong to opens a *duplicate* there
  rather than doing nothing. The board therefore sends it only while it can prove the window
  it raised is still the foreground one **and** that the session belongs to that window's
  workspace — the editor's own documented precondition — and the user is told what can go
  wrong before they turn it on. The board **MUST** therefore still carry
  enough information that the user rarely needs to jump at all — jumping is a shortcut, not
  the way the board is read.

  *Clarified 2026-08-27.* Raising a window **MUST NOT** change anything else about it —
  its size, its position, or its maximised state. This was always the intent and was never
  written down, and the gap had a real consequence: the raise sent `SW_RESTORE`
  unconditionally, so clicking a row un-maximised the editor the user was working in
  (TC-101). A board that rearranges the windows it points at costs more than the glance is
  worth.

  *Amended 2026-08-28.* The clause above is now stated over the three states a window can be
  in, because one of them cannot satisfy it:

  | The window was | It **MUST** end up |
  |---|---|
  | Minimised | **Maximised** |
  | At its own size | At its own size, untouched |
  | Maximised | Maximised, untouched |

  The two "untouched" rows are the 2026-08-27 clarification unchanged, and they remain the
  rule: a window already on screen is brought forward and nothing else. A **minimised** window
  is the case the clause could never cover — it has to be un-minimised before it can be the
  foreground window at all, so the only question is what it comes back as. It comes back
  maximised rather than at whatever size it had before it was put away: a window the user
  minimised and is now being *sent* to is one they are about to read, and handing back a small
  one they must then resize is the same cost the clause exists to avoid, paid in the other
  direction. TC-106 and TC-106b hold both halves against a real window.
- **FR-34** The board **MUST** be able to emit an optional, **off-by-default** notification
  (sound or flash) when a session enters *waiting for the user* or *stopped abnormally*.
  Measurement is the reason this is in v1 rather than deferred: across 78 transcripts the
  user left a *finished* session sitting for more than two minutes on **271 of 503** turn
  ends. An ambient board only helps while it is being looked at; a notification is what
  covers the case where it is not. It stays off by default — an alert the user did not ask
  for is worse than no alert (§9 of the state model applies to sound as much as to colour).
- **FR-35** The board **SHOULD** offer a "hide titles" mode that shows indicators only, for
  screen sharing and recording.

### 6.6 Configuration and resilience

- **FR-36** All configuration **MUST** live in the product's own directory. The product
  **MUST NOT** write to Claude Code's own state files as part of normal operation.
- **FR-37** Optional hook installation (see [ADR-0001](adr/0001-observe-local-state-files.md))
  **MUST** be explicit opt-in, **MUST** back up the file it edits, **MUST** be reversible
  from the UI, and **MUST NOT** disturb hooks the user already configured. The full
  specification of that flow is §6.7.
- **FR-38** The product **MUST** tolerate unknown, malformed, partially written, or
  newly-added records in the files it reads: it skips them and keeps running.
- **FR-39** When the observed data no longer matches what the product understands, it
  **MUST** degrade to the neutral status and **MUST** surface one visible, non-blocking
  warning, rather than showing a confidently wrong board.
- **FR-40** Stale leftovers (records of processes that no longer exist) **MUST** be
  detected and ignored; they **MUST NOT** appear as live sessions.

### 6.7 Guided hook setup

Hooks are what make the *waiting for you* status exact rather than inferred, and the user
is assumed to have no interest in configuring them by hand. The product therefore ships the
setup, not instructions. Full specification: [hook-setup.md](hook-setup.md).

- **FR-41** The product **MUST** offer to set hooks up itself, and the offer **MUST** be
  phrased in terms of what the user gets ("make *waiting for you* exact"), not in terms of
  hooks, JSON, or file paths. Completing the setup **MUST NOT** require the user to
  understand what a hook is or to open any file.
- **FR-42** Before anything is changed, the product **MUST** state in one screen what
  improves, what is being changed on the user's machine, and that it is reversible.
- **FR-43** The product **MUST** show the exact change it will make before making it.

  *Met (recorded 2026-08-28).* The preview step shows the change as a **rendered
  line-by-line diff** — `+`, `-` and context — counted in both lines and entries, so the
  sentence above the pane and the pane itself cannot disagree (`crates/board/src/hooks.rs`,
  `hook-setup.md` §4 step 3). The wording of the requirement has not changed; what was out of
  date was §11, which still described a lesser form as what the first release would carry.
- **FR-44** It **MUST** write a timestamped backup of the settings file first, and **MUST**
  show where that backup is.
- **FR-45** The edit **MUST** be purely additive: it **MUST NOT** modify, reorder, or remove
  any entry it does not own, and **MUST NOT** touch any setting other than the hook entries
  it adds.
- **FR-46** Its own entries **MUST** carry a unique, versioned marker. Setup **MUST** be
  idempotent, and an upgrade **MUST** replace only entries carrying that marker.
- **FR-47** The product **MUST** install the smallest event set that satisfies the state
  model, and **MUST NOT** install a hook on any event that is able to block, delay, or
  alter a tool call or a prompt. Nothing it installs may change what a session does.
- **FR-48** The hook helper **MUST** exit successfully and silently in every circumstance —
  including when the board is not running — **MUST NOT** emit output that Claude Code could
  interpret, and **MUST** complete within a strict time budget with a timeout configured as
  a second line of defence.
- **FR-49** The helper **MUST** discard everything in the event payload except the event
  kind, the session id, a timestamp it stamps itself, and — for `Notification` only — the
  notification kind. Prompt text, assistant messages, tool inputs, file contents, and paths
  **MUST NOT** be written anywhere (NFR-07).

  *Amended 2026-08-27.* This previously required "a coarse tool category". No such field
  exists: FR-47 forbids hooking any event that runs before a tool call, and none of the five
  events that remain carries a tool name (`docs/observation-sources.md` §5). The requirement
  named a fact the product is structurally unable to obtain. What the payloads do offer is
  `notification_type`, which distinguishes a permission prompt from an idle one — the
  distinction the state model actually needs.
- **FR-50** Setup **MUST NOT** report success on a successful write alone: it **MUST** wait
  for a real event to arrive and show that it did. If none arrives, it **MUST** say so and
  offer to undo.
- **FR-51** Removal **MUST** be available in one action, **MUST** remove only the product's
  own entries, and **MUST NOT** depend on the backup existing.
- **FR-52** If hooks are installed but events stop arriving, the product **MUST** fall back
  to inference on its own and **MUST** show that it has done so. It **MUST NOT** keep
  presenting exact-mode confidence it is no longer earning.
- **FR-53** If the settings file cannot be parsed, or the expected event names do not match
  the installed Claude Code, the product **MUST** write nothing and explain, rather than
  writing a guess.
- **FR-54** The product **MUST NOT** install, re-install, or update hooks without an
  explicit decision by the user, and a declined offer **MUST** stay declined.
- **FR-55** The current mode — exact or inferred — **MUST** be visible from the board's
  menu at all times, together with a diagnosis view that reports each precondition and the
  single action that repairs it.

  *Met.* The menu's first item is the mode, disabled because it is a statement rather than an
  action (`ui-overlay.md` §6.2), and *Notice waiting instantly* carries a tick saying the same
  thing where the reader is looking — one place decides for both, so they cannot disagree. It
  is also in the board's tooltip and at the top of the setup window. The diagnosis is the menu item below it — *Check the reporting…* — and reports the
  four preconditions of [hook-setup.md](hook-setup.md) §5, each with one action and no more
  than one ([ADR-0026](adr/0026-the-diagnosis-lives-in-the-setup-window.md)). The row that
  cannot be decided from the outside — entries installed and nothing arriving — offers to
  **watch for the next event** rather than name a cause, which is the same proof FR-50
  requires of the install.

## 7. Non-functional requirements

- **NFR-01 Latency** — A status change **MUST** be visible within **2 s** of the underlying
  event *becoming observable*, measured from the moment the evidence exists on disk or in
  the process table — not from the moment the user was blocked.
  The distinction matters, because the *waiting for user* transition is deliberately
  debounced before it is believed: `T_pending_interactive` (1.5 s) for an always-interactive
  tool, `T_pending_probe` (2 s) before an absent process counts as evidence. Those
  debounces are the specification, so the earlier "500 ms for *working → waiting for user*"
  target was unsatisfiable by construction and has been removed. In hook mode the
  notification event is authoritative on arrival and the 2 s budget applies from there.
- **NFR-02 Cost of observing** — Observation **MUST** be incremental. Transcript files grow
  to megabytes; the product **MUST NOT** re-read a file from the start to learn what
  changed.
- **NFR-03 Footprint** — With 10 live sessions and 30 known workspaces, the product
  **SHOULD** stay below 1 % CPU while idle and below 150 MB RSS. It **MUST NOT** measurably
  slow down the sessions it watches.
- **NFR-04 Non-interference** — In normal operation the product **MUST** be a read-only
  observer. It **MUST NOT** hold exclusive locks, rewrite, truncate, or delete anything
  Claude Code owns, and **MUST NOT** connect to Claude Code's internal IPC endpoints. The
  **only** exception is the hook entries of §6.7: written solely on the user's explicit
  instruction, additive, and removable in one action. There is no other circumstance in
  which this product writes to anything Claude Code owns.
- **NFR-05 Robustness of parsing** — The files being read are **undocumented internals of
  Claude Code and may change in any release**. Parsing **MUST** be defensive, **MUST**
  record the observed Claude Code version, and failures **MUST** be contained to the
  affected session.
- **NFR-06 Availability** — The board **MUST** survive Claude Code sessions starting and
  ending, VS Code restarting, sleep/resume, display hot-plug, and the machine changing
  its monitor layout, without a restart.
- **NFR-07 Privacy — content** — Conversation content **MUST NOT** be persisted. Only the
  minimum needed for the board (session id, workspace path, title, status, timestamps)
  may be kept, and only for the lifetime of the session. Logs **MUST NOT** contain prompt
  or response text.
- **NFR-08 Privacy — network** — The product **MUST NOT** make any outbound network
  request: no telemetry, no crash reporting, no update check that transmits state.
- **NFR-09 Transparency** — The product **MUST** document exactly which files it reads and
  what it derives from them ([observation-sources.md](observation-sources.md)), because it
  reads files that contain the user's source code and prompts.
- **NFR-10 Portability of the core** — The observation and status logic **MUST** be kept
  free of OS-specific code, so that only the UI layer has to be replaced to support another
  platform later. See [ADR-0002](adr/0002-target-windows-first.md).
- **NFR-11 Testability** — Status derivation **MUST** be a pure function of a recorded
  event sequence, so it can be tested from fixtures without a running Claude Code.
  **The event sequence is not only transcript records.** It is the ordered merge of:
  transcript records, session-registry appearances and disappearances, process
  liveness changes, tool-process start/stop events (FR-13,
  [ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md)), subagent-file
  activity ([ADR-0007](adr/0007-detect-subagents-from-their-own-file.md)), hook events, and
  timer ticks. Every one of those is an **input** to the pure function; none of them may be
  read from the ambient system inside it. In particular the core **MUST NOT** call the clock
  or the OS directly — time advances only by a tick in the sequence — because otherwise
  FR-59, §7.2 of the state model, and ADR-0007 are not replayable and the purity claim is
  decorative.
  Ordering within that merge is by observation order, never by any timestamp the records
  carry ([ADR-0006](adr/0006-order-events-by-append-position.md)).

## 8. Constraints and assumptions

- **C-1** Windows 11 is the only supported platform for the first release
  ([ADR-0002](adr/0002-target-windows-first.md)).
- **C-2** Everything the product needs is on the local filesystem
  ([ADR-0001](adr/0001-observe-local-state-files.md)). No Claude Code API, extension API, or
  public schema is available for this purpose.
- **C-3** The formats read are internal and unversioned. Breakage on a Claude Code update is
  a *when*, not an *if*; the design treats it as a normal operating condition (NFR-05).
- **C-4** Session titles are produced by Claude Code in the user's language and are already
  short. The product displays them as-is and only truncates to fit (FR-19, FR-20).
- **C-5** The product is single-user and single-machine.
- **C-6** The implementation is **Rust + Tauri**
  ([ADR-0004](adr/0004-use-tauri-and-rust.md)): a Rust core, a WebView2-rendered UI, and a
  small Rust hook helper binary.

## 9. Open decisions

| # | Question | Blocking |
|---|---|---|
| ~~D-1~~ | ~~Implementation runtime / UI toolkit~~ | **Decided** — Rust + Tauri, [ADR-0004](adr/0004-use-tauri-and-rust.md) |
| ~~D-2~~ | ~~Exact palette and motion for each status; final layout of both densities~~ | **Decided** — settled in the design pass. Geometry, palette and motion are in [ui-overlay.md](ui-overlay.md); the palette is derived rather than picked ([ADR-0010](adr/0010-derive-the-palette-from-a-luminance-ladder.md)). "Both densities" no longer applies: review of the design pass replaced the pair with **one density that folds** ([ADR-0012](adr/0012-one-density-that-folds.md)), which superseded [ADR-0011](adr/0011-answer-which-session-by-position-and-hover.md) |
| ~~D-3~~ | ~~Whether "click to focus the VS Code window" can reliably reach the right window when one process hosts several windows~~ | **Decided — yes, and FR-33 stands unchanged.** The spike ran before the design pass fixed a click target. A session maps to its window *exactly*, through the extension host that spawned it and the lock file whose port that process listens on — not by comparing paths, and not defeated by two windows sharing a folder. Turning that window into a raisable handle is only folder-accurate, which is precisely what FR-33 already promised, so the fallback wording was not needed. `SetForegroundWindow` **is** refused for a board that never takes focus, and the workaround and the success check are specified ([ADR-0009](adr/0009-map-a-session-to-its-window.md)) |
| D-4 | Packaging and update mechanism (installer, portable zip) | No |
| D-5 | Where the hook helper binary is installed from, so that its path stays valid across updates | No — affects §6.7 only |
| ~~D-6~~ | ~~How a parent session is known to be running a subagent~~ | **Decided** — the subagent's own `agent-*.jsonl` is the evidence, [ADR-0007](adr/0007-detect-subagents-from-their-own-file.md) |

## 10. Acceptance

The first release is done when, with 5+ sessions live across 3+ VS Code windows:

1. Every live VS Code session appears exactly once, grouped by folder name.
2. Each of the five displayed statuses can be produced on demand and is shown within 2 s.
3. The board can be dragged, shrunk, restarted, and comes back where it was.
4. A turn that dies on an unrecovered error shows the abnormal-stop status while the
   session is still running; killing a *working* session's process shows it too; and
   closing the window instead does **not** — a normal close is never reported as a crash.
5. Clicking any session on the board raises a VS Code window that has that session's
   workspace open, from every one of the 3+ windows, without the board taking focus itself
   (FR-30). Where two windows share a folder, raising either one passes.
6. Nothing under Claude Code's own state directory is modified except the hook entries the
   user explicitly asked for, and no network connection is opened, over a full working day
   of use.
7. The hook setup can be completed, verified, and fully undone from the board alone — no
   file is opened by hand — and the settings file after removal is equivalent to what it
   was before installation.

## 11. Release scope

The requirement levels above describe the finished product. This section says what the
**first release** has to satisfy — everything else is real, agreed, and deferred.

**No individual requirement, and no other document, states its own release timing.** This
section is the only place release scope is decided, so there is exactly one thing to keep
current when the scope changes.

**v1 must satisfy**

| Area | Requirements |
|---|---|
| Discovery | FR-01 … FR-07 |
| Status | FR-08 … FR-15, FR-56 … FR-59 |
| Presentation | FR-16, FR-17, FR-18, FR-19, FR-20, FR-21, FR-22, FR-23, FR-24 |
| Window | FR-26, FR-27, FR-28, FR-29, FR-30 |
| Interaction | FR-18's fold / unfold, dragging (FR-27), click-to-raise (FR-33), and the opt-in alert (FR-34) |
| Resilience | FR-38, FR-39, FR-40 |
| Hook layer | FR-36, FR-37, FR-41 … FR-55 |
| Non-functional | NFR-01 … NFR-11 (all of them — they are properties, not features) |

**FR-34 (the opt-in alert) is in v1**, having been deferred in the first draft. The
measurement moved it: on **271 of 503** turn ends the user left the finished session sitting
for more than two minutes. An ambient board only pays off while someone is looking at it,
and the data says that is often not happening. It stays off by default.

The hook layer is in v1 despite being formally optional — but **not** because inference is
helpless. Inference now covers subprocess tools (the process probe), in-process tools (the
outlier rule), always-interactive tools, and subagents
([ADR-0008](adr/0008-detect-blocking-by-whether-the-tool-started.md),
[ADR-0007](adr/0007-detect-subagents-from-their-own-file.md)). Hooks are in v1 because they
make the same transition *authoritative and immediate* instead of doubly-inferred, and
because the parts that make editing a user's settings defensible cannot be bolted on later:
consent, backup, additive merge, marker, the do-nothing helper, verification, and removal.

**FR-33 (click to raise the owning window) moves into v1.** It was deferred on the argument
that raising a window cannot select the session inside it, so the board should carry the
information instead. The second half of that is still true and stays a requirement — but it
does not follow that the jump is worthless. The board's whole purpose is to answer *which
session needs me*, and the action that answer leads to is going there; making the user find
the right window by hand afterwards throws away most of what the board just established.
The limits are real and are now written into FR-33 itself rather than used as a reason to
drop it. **This makes D-3 blocking** — a v1 `MUST` cannot rest on an unanswered question
about whether the right window can be reached at all.

**FR-52 (automatic fallback when hook events stop) moves into v1** as well. It was deferred
while the state model simultaneously promised it in §3, which was simply inconsistent, and
[ADR-0003](adr/0003-guided-hook-setup.md) treats fallback as part of the decision rather
than as polish. A mode that silently keeps claiming exactness it is no longer earning is
worse than no hook layer at all.

**Deferred past v1**

| Requirement | Why it can wait |
|---|---|
| FR-25 (UI localisation) | The UI has almost no text. Session titles already pass through in any language. |
| FR-31 (opacity, click-through) | Comfort features; nothing is blocked without them. |
| FR-32's *start-on-login* half | Writing to the user's `Run` key needs the same explain-consent-undo flow as the hooks (FR-54), which is a screen of its own. **The tray half is no longer deferred** — `Ctrl+Alt+B` made it the only way back to a hidden board. |
| FR-35 (hide titles) | Matters for screen sharing, not for the first user. |

Two rules keep this honest:

- **Deferring is not deleting.** A deferred requirement keeps its id and its wording; only
  its release changes.
- **Nothing in the deferred list may be designed out.** v1 must not make anything in the
  table above harder to add later — in particular, hook events and inferred events must
  already share one internal representation, even while only one of them is produced. The
  rule has now been paid three times over: FR-43's rendered diff arrived **early**, because
  the installer had to compute the change anyway and showing it cost almost nothing; FR-32's
  tray reuses the board's own menu and its own roll-up; and FR-55's diagnosis reuses the setup
  window's steps and its refusals. None of the three needed anything unpicked first.
