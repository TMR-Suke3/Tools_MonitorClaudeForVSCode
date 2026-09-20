# Guided hook setup

The product can detect session status without any configuration. It detects it *exactly*
if Claude Code is configured to report its own lifecycle events — a feature Claude Code
calls **hooks**.

Setting hooks up by hand means editing a JSON settings file. **The user is never asked to
do that.** This document specifies the assisted setup that does it for them, and the rules
that keep it safe.

Related: [ADR-0003](adr/0003-guided-hook-setup.md) (why it is built this way),
[ADR-0001](adr/0001-observe-local-state-files.md) (why hooks are optional at all),
[observation-sources.md](observation-sources.md) §5, [requirements.md](requirements.md) §6.7.

> This document describes the **complete** flow. Parts of it are deferred past the first
> release; which parts is stated in one place only — [requirements.md](requirements.md)
> §11. Where a deferral changes what the user actually sees, the v1 minimum is called out
> inline.

---

## 1. Design stance

The person this is built for does not want to learn what a hook is. They want the board to
be right.

So the setup flow obeys four rules:

1. **No jargon in the UI.** The offer is *"make 'waiting for you' exact"*, not *"install
   hooks"*. The word "hook" appears once, in the details, as an explanation — never as
   something the user must understand to proceed.
2. **Nothing happens without a yes.** It is never installed automatically, never at first
   run, never as part of an update.
3. **Every change is shown, backed up, and undoable in one click.**
4. **The smallest possible footprint.** The fewest events, none of them able to interfere
   with a session, and a helper that does as close to nothing as possible.

## 2. What changes when it is on

| | Inference mode (default) | Hook mode |
|---|---|---|
| *Waiting for you* | Inferred from a pending tool call plus evidence that the tool is not actually running — measured within a couple of seconds where that evidence exists, and silent where it does not (see [session-state-model.md](session-state-model.md) §4.1). | Known the moment it happens, from the session itself rather than from two indirect signals. |
| *Turn finished* | Already structural — the turn's stop reason says so. Hooks add little here. | Known immediately. |
| *Session ended normally vs. died* | Inferred from process disappearance plus context; a normal close in an unusual order can be misread. | Known — a clean end reports itself. |
| Latency | Up to a few seconds. | Sub-second. |
| Configuration | None. | Five entries added to a settings file, removable in one click. |

Everything else — discovery, grouping, titles, working/idle — is identical in both modes.

## 3. What gets installed

### 3.1 The events

Five events, chosen to be the minimum that answers the questions above:

| Event | What the board learns |
|---|---|
| Session start | A session exists, exactly when it starts. |
| Prompt submitted | Work has begun → `working`. |
| Notification | Claude is asking the user for something → `awaiting_user`. **The reason this feature exists.** |
| Turn stop | The turn is over → classify idle. |
| Session end | The session ended *cleanly* → not a crash. |

**Deliberately not installed: any event that runs before a tool call.** Those events fire
constantly and — more importantly — are able to block or rewrite the tool call they
precede. The product refuses to put itself in that path (FR-47). Nothing it installs is
capable of changing what a session does.

> Event names and payload shapes are Claude Code's, not ours, and are pinned against the
> current documentation at implementation time. If they do not match at runtime, the setup
> refuses to install rather than writing a guess (FR-53).

### 3.2 The shape of the change

One entry per event is added under the `hooks` key of the **user-level** settings file
(`~/.claude/settings.json`) — user level because the board watches every project, not one.

```jsonc
{
  "hooks": {
    "<EventName>": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"<install-dir>/mcv-hook.exe\" <event> # monitor-claude-vscode v1",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

The trailing marker is what makes the entry **ours**: removal and upgrade match on it, and
nothing without that marker is ever touched (FR-45, FR-46).

### 3.3 The helper

`mcv-hook` is a single small executable shipped with the product. Per invocation it:

- reads the event from standard input,
- keeps **only** the event kind, the session id, a timestamp it stamps on arrival, and —
  for `Notification` — the notification kind (FR-49),
- appends one line to a file in the product's own data directory,
- exits `0`, silently, always.

What it must never do (FR-48, FR-49):

- **Never fail loudly.** It exits `0` even when the board is not running, the data directory
  is missing, or the payload is unrecognised. A hook that errors is a hook that annoys the
  user inside the session it was supposed to help.
- **Never print anything** Claude Code might interpret as instructions or as a block.
- **Never keep content.** Hook payloads can contain prompt text, file contents, and command
  lines. All of it is discarded in memory before anything is written (NFR-07).
- **Never use the network** (NFR-08).
- **Never be slow.** Budget: a few tens of milliseconds; a timeout is configured on the hook
  entry as a second line of defence.

Events are written for *every* Claude Code session on the machine, including terminal ones.
The board ignores the sessions it does not display, and old event files are cleaned up.

## 4. The flow

### Step 1 — the offer

Shown once, in the board, when hooks are not installed. Dismissible, and *stays* dismissed
(FR-54).

> **"Waiting for you" is currently a guess.** Make it exact? — [Set up] [Not now] [Don't ask again]

### Step 2 — the plain-language explanation

One screen, no JSON:

> **What you get** — the board knows the moment Claude asks you something, instead of
> guessing after a delay. It also knows when a session ended normally, so a normal close is
> never reported as a crash.
>
> **What it costs** — **five entries** are added to your Claude Code settings file, one for
> each moment this app needs to hear about: a session starting, you sending a message,
> Claude asking you something, a reply finishing, and the session ending. Each entry runs a
> small program that came with this app for a few milliseconds and writes a timestamp to a
> file on this computer. It never reads your conversation and never connects to the
> internet.
>
> **Undo** — one click, any time. A backup is made first.

[Show me the exact change] [Cancel]

*Amended 2026-08-27.* This row used to read **[Show me the exact change] [Set it up]
[Cancel]**, and it contradicted FR-43: a *Set it up* here is either a way to write without
having seen the change, or a second button that does exactly what the first one does. The
requirement wins, so the button that writes lives on the preview screen and nowhere else.

### Step 3 — the preview

Nothing is written until the user has seen what will change.

The path of the file being edited, and **the exact diff as it will be applied** — which
does the thing a description cannot: it *shows* that nothing is removed or reordered,
rather than asking the user to take that on trust (FR-43).

One caveat is stated on the same screen rather than discovered afterwards. The product
parses the settings and prints them back, so a file laid out any other way — a different
indentation, different line endings, no whitespace at all — comes back in the product's
layout throughout: every value and every position unchanged, but the whole file rewritten.
Claude Code's own two-space output round-trips untouched, so this is normally not the case;
when it is, the preview says so before the user agrees.

### Step 4 — backup and write

- A timestamped copy of the settings file is written first; **its path is shown, not hidden**
  (FR-44).
- Only the `hooks` key is touched; existing hooks — the user's own or another tool's — are
  left in place and ours are added alongside (FR-45).
- If the file is missing, it is created with only the `hooks` key.
- If the file cannot be parsed, **nothing is written**: the user is told, and offered a
  button to open the file (FR-53).

### Step 5 — proof that it works

The setup does not claim success from a successful write. It waits for a real event
(FR-50):

> Open any Claude Code session and send a message.
> ⏳ Waiting for the first event… → ✅ Received. Status detection is now exact.

If nothing arrives within a couple of minutes, the setup says so plainly and offers
[Undo the change] and [Keep it and check later]. It never leaves the user believing
something works when it does not.

### Step 6 — afterwards

The board's menu permanently shows the current mode — *exact* or *inferred* — and the way
back out. There is no state the user cannot see or reverse.

## 5. Diagnosis

One panel, reachable from the board's menu (*Check the reporting…*), that answers "is this
thing on?" without the user opening a single file. It is a step in this same window rather
than a window of its own, because every repair it offers is already a path through this one
([ADR-0026](adr/0026-the-diagnosis-lives-in-the-setup-window.md)).

The mode is stated above it, as it is on every step here. Below that, four preconditions **in
repair order** — the helper first, because nothing under it can be fixed while it is missing,
and the file before what is in it for the same reason:

| Precondition | What it says | The one action, when it fails |
|---|---|---|
| The helper program | found · **missing**, and where it should be | *Reinstalling this app puts it back.* No button: nothing here can do it |
| The settings file | read, and from where · not there yet · **cannot be read** | **Open the file** — FR-53's own action |
| Our entries in it | all present · **ours but not this set** · **none** | **Set it up… / Repair them…** |
| Reports arriving | last one 27 minutes ago · **nothing for 4 hours** · **never** | **Watch for the next one** |

Three rules, and each was a way to get this wrong:

- **A row that holds offers nothing.** A repair button beside a tick is how a panel stops
  being read.
- **A repair that would be refused is not offered.** With the helper missing, the entries row
  states what is true and shows no button — setting up would write entries naming a program
  that is not there, and the preview refuses it anyway.
- **Two rows never offer the same repair.** With nothing installed, the silence row is not
  where to act; the entries row is.

**A settings file that is not there yet is not a failure.** A first install writes the file
Claude Code would have written itself.

**The last row watches rather than guesses.** Entries installed and nothing arriving has two
causes that cannot be told apart from here — no session has started since they went in (the
commonest, see §7), or something is stopping the helper — so the action is to wait for a real
event, exactly as step 5 does, and say which it was. Proof, in the one corner of the product
where an inference would otherwise be presented as a fact.

Every failing row carries the single action that fixes it, never an explanation of what went
wrong internally: the user is not required to know what a hook is in order to repair one.

## 6. Removal

- One click removes **only** entries carrying our marker, and leaves the file otherwise
  exactly as it was (FR-51).
- Removal is offered in the same place as installation, always, not buried in settings.
- If the product is uninstalled, removal is offered as part of that.
- Removal never depends on the backup — the backup is a safety net, not the mechanism.

## 7. Failure modes

| Situation | Behaviour |
|---|---|
| Settings file malformed | Nothing written. Explained, with a button to open the file. |
| Another tool already hooks the same events | Ours are added alongside. Nothing of theirs is read, moved, or removed. |
| An older version of our own entries is present | Replaced — only ours, matched by marker (FR-46). |
| The user edited our entry by hand | Treated as theirs: not overwritten. Diagnosis reports "modified"; repair asks first. |
| Hooks installed but no events arriving | The board never keeps showing exact-mode confidence it is not getting: it falls back to inference on its own after 30 minutes' silence (FR-52), says so in the menu and the mode line, and §5's last row offers to watch for the next event rather than name a cause it cannot know. |
| Event names no longer match Claude Code | Setup refuses to install; an installed setup degrades to inference and reports it. |
| Board not running when an event fires | The helper writes the line anyway and exits 0. Nothing breaks. |

## 8. What this is not

- Not a way to run arbitrary commands on session events for the user. The product installs
  its own helper and nothing else; it is not a hook manager.
- Not a requirement. Every feature of the board works without it (ADR-0001).
- Not silent. There is no path through this product where the user's settings file changes
  without them having seen the change and said yes.
