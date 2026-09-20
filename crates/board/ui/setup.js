// The hook setup window: `docs/hook-setup.md` §4, step by step.
//
// Every decision worth getting wrong was made in Rust — what the change is, whether the
// file can be read, whether an event has arrived. This file moves between steps and paints
// what it is handed. In particular it never builds the JSON it is about to write: it shows
// the diff the installer produced, so what is previewed and what is applied cannot drift.

const { invoke } = window.__TAURI__.core;

const steps = new Map(
  [...document.querySelectorAll(".step")].map((el) => [el.dataset.step, el]),
);

/** Where the backup went, kept for the sentence that names it (FR-44). */
let backup = null;
/**
 * The poll for the first real event, so leaving the step stops it.
 *
 * Set back to `null` everywhere it is cleared. Clearing a stale id is harmless in itself, but
 * this is shared by step 5 and by the diagnosis's watch, and a variable that still holds a
 * dead timer cannot be read to answer "is one running".
 */
let waiting = null;
/** When the entries were written, so an old event log cannot pass for proof (FR-50). */
let installedAt = 0;

function show(step) {
  for (const [name, el] of steps) {
    el.hidden = name !== step;
  }
  // The first button of a step is where the eye goes, so it is where the keyboard goes.
  steps.get(step)?.querySelector("button")?.focus();
}

/** Paints the colours the board is using, so the two windows are the same product. */
function applyTheme(chrome, theme) {
  document.documentElement.dataset.theme = theme;
  const root = document.documentElement.style;
  root.setProperty("--board", chrome.board);
  root.setProperty("--hairline", chrome.hairline);
  root.setProperty("--text", chrome.text);
  root.setProperty("--secondary", chrome.secondary);
  root.setProperty("--edge-colour", chrome.edge);
}

/**
 * The change in words: what a reader who does not know diff conventions needs, and what
 * FR-45 is a promise about. Counted in Rust from the diff that is being shown, so the
 * sentence and the pane cannot disagree.
 */
function renderTally(target, change) {
  const lines = (n) => `${n} line${n === 1 ? "" : "s"}`;
  const entries = (n) => `${n} entr${n === 1 ? "y" : "ies"}`;
  target.replaceChildren();

  const added = document.createElement("strong");
  added.textContent = lines(change.added);
  const removed = document.createElement("strong");
  removed.className = "removed";
  removed.textContent = change.removed === 0 ? "nothing" : lines(change.removed);

  target.append(added, " added, ", removed, " removed");
  // In the unit the previous screen used. "Five entries" is what the user agreed to think
  // about; "53 lines" is what that turns out to be.
  const changed = change.entries_added || change.entries_removed;
  if (changed) {
    const verb = change.entries_added ? "added" : "taken out";
    target.append(` — ${entries(changed)} ${verb}, written out in full`);
  }
  target.append(".");
  if (change.removed === 0) {
    target.append(" Every setting you already have keeps its value and its place.");
  }
}

/**
 * Renders a diff.
 *
 * Line by line, coloured by its first character, and *not* interpreted any further: this
 * text contains the user's own settings, which must be shown exactly as they are.
 */
function renderDiff(target, text) {
  target.replaceChildren();
  for (const line of text.split("\n")) {
    if (line === "") continue;
    // One element per line, so a line too long for the pane hangs under itself instead of
    // wrapping back to the margin — where it would read as a line of its own, without the
    // + or - that says what it is.
    const row = document.createElement("div");
    row.className = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "same";
    row.textContent = line;
    target.append(row);
  }
}

// ------------------------------------------------------------------ the steps

async function toPreview() {
  const change = await invoke("hook_preview", { install: true }).catch((reason) => {
    refuse(reason);
    return null;
  });
  if (!change) return;

  document.getElementById("settings-path").textContent = change.path;
  renderTally(document.getElementById("diff-tally"), change);
  renderDiff(document.getElementById("diff"), change.diff);
  document.getElementById("reformat-warning").hidden = !change.reformats;
  show("preview");
}

async function toRemove() {
  const change = await invoke("hook_preview", { install: false }).catch((reason) => {
    refuse(reason);
    return null;
  });
  if (!change) return;

  renderTally(document.getElementById("remove-tally"), change);
  renderDiff(document.getElementById("remove-diff"), change.diff);
  show("remove");
}

async function install() {
  const written = await invoke("hook_install", { install: true }).catch((reason) => {
    refuse(reason);
    return null;
  });
  if (!written) return;

  backup = written.backup;
  installedAt = written.installed_at;
  document.getElementById("backup-line").textContent = backup
    ? `A backup of your settings is at ${backup}`
    : "You had no settings file yet, so there was nothing to back up.";
  show("verify");
  awaitFirstEvent();
}

async function remove() {
  const written = await invoke("hook_install", { install: false }).catch((reason) => {
    refuse(reason);
    return null;
  });
  if (!written) return;
  await invoke("close_setup");
}

/**
 * Step 5. **A successful write is not success** (FR-50).
 *
 * Nothing here claims the feature works until an event written by the helper, in a real
 * session, has arrived. If none does, the step says so and offers the way back out — which
 * is the difference between a setup that verifies and one that hopes.
 */
function awaitFirstEvent() {
  const line = document.getElementById("waiting-line");
  const actions = document.getElementById("verify-actions");
  const started = Date.now();

  /** Two minutes, as §4 step 5 says. Long enough to switch windows and type something. */
  const GIVE_UP_AFTER = 120_000;

  clearInterval(waiting);
  waiting = setInterval(async () => {
    const seen = await invoke("hook_events_seen", { since: installedAt }).catch(() => false);
    if (seen) {
      clearInterval(waiting);
      waiting = null;
      document.getElementById("done-backup-line").textContent =
        document.getElementById("backup-line").textContent;
      show("done");
      return;
    }
    if (Date.now() - started > GIVE_UP_AFTER) {
      clearInterval(waiting);
      waiting = null;
      line.dataset.state = "late";
      line.textContent =
        "No event has arrived. The entries are in place, but nothing has used them yet — " +
        "which usually means the session you sent a message in was already running. " +
        "A session started from now on would settle it.";
      actions.hidden = false;
    }
  }, 1000);
}

// ------------------------------------------------------------- the diagnosis

/**
 * The diagnosis (FR-55, `docs/hook-setup.md` §5).
 *
 * Four preconditions, each with the single action that repairs it. Every one of those
 * decisions is Rust's — which precondition is broken, what to say about it, and which action
 * belongs to it — and this builds the rows. The same rule as the rest of the window: a front
 * end that decided any of it would be a second, quieter copy of the rules that do the
 * writing.
 */
async function toDiagnose() {
  // A poll left running from step 5, or from a previous watch on this panel, would go on
  // firing behind the rows it was about to redraw.
  clearInterval(waiting);
  waiting = null;
  const diagnosis = await invoke("hook_diagnosis").catch(() => null);
  if (!diagnosis) return;

  renderMode(diagnosis.mode);
  const list = document.getElementById("checks");
  list.replaceChildren();
  for (const check of diagnosis.checks) {
    list.append(renderCheck(check));
  }
  show("diagnose");
}

function renderCheck(check) {
  const row = document.createElement("li");
  row.className = "check";
  row.dataset.ok = String(check.ok);
  row.dataset.check = check.id;

  const mark = document.createElement("span");
  mark.className = "mark";
  mark.textContent = check.ok ? "✓" : "!";
  // The mark is decoration; the state is said in words for anything that cannot see it.
  mark.setAttribute("aria-hidden", "true");
  const state = document.createElement("span");
  state.className = "sr-only";
  state.textContent = check.ok ? "in place: " : "not working: ";

  const what = document.createElement("p");
  what.className = "what";
  what.append(state, check.what);

  const detail = document.createElement("p");
  detail.className = "detail";
  detail.textContent = check.detail;

  row.append(mark, what, detail);
  if (check.action) row.append(renderFix(check.action));
  return row;
}

function renderFix(action) {
  const fix = document.createElement("p");
  fix.className = "fix";
  // `tell` is the case the product cannot repair. It gets the sentence and no control,
  // because a button that could only report its own failure is worse than saying so.
  if (action.kind === "tell") {
    fix.classList.add("tell");
    fix.textContent = action.label;
    return fix;
  }
  const button = document.createElement("button");
  button.dataset.action = `fix:${action.kind}`;
  button.textContent = action.label;
  fix.append(button);
  return fix;
}

/**
 * The repair for silence: wait, rather than assert a cause.
 *
 * Entries in place and nothing arriving has two causes that look identical from here — no
 * session has started since they went in, or something is stopping the helper — and one
 * action separates them. So the row stops claiming to know and watches instead. This is the
 * same proof step 5 uses (FR-50), moved into the row it is about.
 */
function watchForEvent(row) {
  const since = Date.now();
  const started = since;
  /** Two minutes, as §4 step 5 uses. Long enough to switch windows and type something. */
  const GIVE_UP_AFTER = 120_000;

  const fix = row.querySelector(".fix");
  fix.replaceChildren();
  const line = document.createElement("span");
  line.className = "waiting";
  const spinner = document.createElement("span");
  spinner.className = "spinner";
  spinner.setAttribute("aria-hidden", "true");
  line.append(spinner, "Open a session and send a message. Watching…");
  fix.append(line);

  clearInterval(waiting);
  waiting = setInterval(async () => {
    const seen = await invoke("hook_events_seen", { since }).catch(() => false);
    if (seen) {
      clearInterval(waiting);
      waiting = null;
      // Re-read rather than mark this row green: an event arriving changes the mode line at
      // the top too, and a panel where one row disagreed with the heading would be worse
      // than one that took a moment to redraw.
      await toDiagnose();
      return;
    }
    if (Date.now() - started > GIVE_UP_AFTER) {
      clearInterval(waiting);
      waiting = null;
      line.dataset.state = "late";
      line.replaceChildren(
        "Nothing arrived in two minutes. A session started from now on would settle it — " +
          "one that was already running when the entries went in does not have them.",
      );
    }
  }, 1000);
}

/**
 * The two faces of the reveal screen (ADR-0029).
 *
 * Off, it explains before anything happens. On, it offers the way out and nothing else —
 * stopping something the user already agreed to owes them no second argument.
 */
async function toReveal() {
  const on = await invoke("reveal_state").catch(() => false);
  show(on ? "reveal-on" : "reveal");
}

async function setReveal(on) {
  await invoke("reveal_set", { on }).catch(() => {});
  await toReveal();
}

async function undo() {
  clearInterval(waiting);
  waiting = null;
  await invoke("hook_install", { install: false }).catch(() => {});
  await invoke("close_setup");
}

/** The refusal (FR-53): the file is as it was, and the useful action is to open it. */
function refuse(reason) {
  clearInterval(waiting);
  waiting = null;
  document.getElementById("refusal-reason").textContent = String(reason);
  invoke("hook_settings_path")
    .then((path) => {
      document.getElementById("refusal-path").textContent = path;
    })
    .catch(() => {});
  show("refused");
}

// ---------------------------------------------------------------- shortcuts

/**
 * The keys as they are being edited, before *Use these keys* is pressed.
 *
 * Held apart from what is registered, because pressing a combination is not the same as
 * asking for it: someone trying keys to see what a chord looks like has not yet decided
 * anything, and nothing should be taken from another program until they say so.
 */
let editing = { summon: "", toggle: "" };
/** Which key is listening for a press, or `null`. */
let capturing = null;

/** What the operating system answered, in the words the menu uses for the same states. */
const VERDICT = {
  live: "Working.",
  off: "Not set.",
  taken: "Another application on this computer already has this one.",
  unreadable: "Not a combination this can be asked for.",
  duplicate: "The other shortcut above already has this one.",
};

/**
 * A key press as an accelerator string, or `null` if it is not one.
 *
 * **At least one modifier, always.** A global shortcut of a bare letter is that letter taken
 * away from every other program on the machine, which is not something anybody means to ask
 * for from a settings screen.
 *
 * Read from `event.code`, the physical key, rather than `event.key`, which is what the layout
 * makes of it — the accelerator the operating system is given is about the key, and on a
 * layout where Alt+M types something else `event.key` would name that instead.
 */
function accelerator(event) {
  const modifiers = [];
  if (event.ctrlKey) modifiers.push("Ctrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Super");

  const code = event.code;
  let key = null;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) key = code;
  else if (code === "Space") key = "Space";
  else if (code === "Backquote") key = "`";
  else if (/^Arrow(Up|Down|Left|Right)$/.test(code)) key = code.slice(5);
  else if (["Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(code)) key = code;

  if (!key || modifiers.length === 0) return null;
  return [...modifiers, key].join("+");
}

function paintKeys(state) {
  editing = { summon: state.summon, toggle: state.toggle };
  for (const which of ["summon", "toggle"]) {
    document.getElementById(`key-${which}`).textContent = state[which] || "not set";
    document.getElementById(`verdict-${which}`).textContent =
      VERDICT[state[`${which}_bound`]] ?? "";
  }
}

/** Draws what is being edited, with the verdicts cleared: they describe the keys as saved. */
function paintEditing() {
  for (const which of ["summon", "toggle"]) {
    const button = document.getElementById(`key-${which}`);
    if (capturing === which) {
      button.textContent = "press a combination…";
      continue;
    }
    button.textContent = editing[which] || "not set";
  }
}

async function toShortcuts() {
  capturing = null;
  paintKeys(await invoke("shortcut_state"));
  show("shortcuts");
}

/**
 * Applies both keys, and closes the window when there is nothing left to say.
 *
 * **Pressing this used to look like nothing happening.** The keys were saved and registered,
 * and then the screen repainted itself with the same two lines it already showed — so the one
 * primary button on the screen had no observable result, which is the thing this product
 * refuses to do anywhere else.
 *
 * Closing is right only when both keys came back *working* or *not set*: there is no answer
 * to read, and staying open would leave the user to find the Close button themselves. Any
 * other outcome — taken, unreadable, or held by the other row — is exactly what the screen
 * exists to show, so it stays open with the answer under the key it belongs to.
 */
async function saveKeys() {
  capturing = null;
  const state = await invoke("shortcut_set", {
    summon: editing.summon,
    toggle: editing.toggle,
  }).catch((why) => String(why));

  if (typeof state === "string") {
    // The command does not fail for a key it cannot use — that is a verdict, not an error —
    // so anything arriving here is the window itself being unable to ask. Saying so beats a
    // button that stays silent.
    for (const which of ["summon", "toggle"]) {
      document.getElementById(`verdict-${which}`).textContent = state;
    }
    return;
  }

  paintKeys(state);
  const settled = ["summon", "toggle"].every((which) =>
    ["live", "off"].includes(state[`${which}_bound`]),
  );
  if (settled) await invoke("close_setup").catch(() => {});
}

// ------------------------------------------------------------------ wiring

document.addEventListener("click", async (event) => {
  const button = event.target.closest("button");
  const action = button?.dataset.action;
  if (!action) return;

  // The diagnosis's repairs. Each is the one action for the row it sits in, and each hands
  // over to a path this window already has — which is the argument for the panel living
  // here at all (ADR-0026).
  if (action.startsWith("fix:")) {
    switch (action.slice(4)) {
      case "install":
        await toPreview();
        break;
      case "open-settings":
        await invoke("open_settings_file").catch(() => {});
        break;
      case "watch":
        watchForEvent(button.closest(".check"));
        break;
      default:
        break;
    }
    return;
  }

  switch (action) {
    case "recheck":
      clearInterval(waiting);
      waiting = null;
      await toDiagnose();
      break;
    case "capture":
      capturing = button.dataset.key;
      paintEditing();
      break;
    case "clear":
      if (capturing === button.dataset.key) capturing = null;
      editing[button.dataset.key] = "";
      paintEditing();
      break;
    case "save-keys":
      await saveKeys();
      break;
    case "reveal-on":
      await setReveal(true);
      break;
    case "reveal-off":
      await setReveal(false);
      break;
    case "preview":
      await toPreview();
      break;
    case "back":
      show("explain");
      break;
    case "install":
      await install();
      break;
    case "do-remove":
      await remove();
      break;
    case "undo":
      await undo();
      break;
    case "retry":
      await toPreview();
      break;
    case "open-settings":
      await invoke("open_settings_file").catch(() => {});
      break;
    case "keep":
    case "close":
    case "cancel":
      clearInterval(waiting);
      waiting = null;
      await invoke("close_setup");
      break;
    default:
      break;
  }
});

// Escape leaves, from every step. Nothing here is a commitment until a button says so.
//
// **Except while a key is being captured**, where Escape means "never mind, leave this key
// alone" — the nearer of the two meanings, and the one a user pressing Escape over a
// half-recorded shortcut intends. Escape is not offered as a shortcut key either way: it is
// not among the codes `accelerator` accepts, and on Windows the combination anyone would
// reach for first, Ctrl+Escape, is the Start menu.
document.addEventListener("keydown", (event) => {
  if (capturing !== null) {
    event.preventDefault();
    if (event.key === "Escape") {
      capturing = null;
      paintEditing();
      return;
    }
    const key = accelerator(event);
    if (!key) return;
    editing[capturing] = key;
    capturing = null;
    paintEditing();
    return;
  }
  if (event.key === "Escape") {
    clearInterval(waiting);
    waiting = null;
    invoke("close_setup");
  }
});

/**
 * The standing line at the top: which mode the board is in right now (FR-55).
 *
 * Three states, and the middle one is the one this exists for. "Installed" is not the same
 * as "working": entries that are in place but silent buy the user nothing, and a window that
 * showed only whether they were installed would report that situation as success.
 */
function renderMode(mode) {
  const line = document.getElementById("mode-line");
  const say = {
    exact: [
      "exact",
      "the board is told the moment Claude asks you something.",
    ],
    "went-quiet": [
      "worked out from a pause",
      "the entries are in place, but nothing has reported in for a while, so the board has " +
        "gone back to working it out.",
    ],
    inferred: [
      "worked out from a pause",
      // "goes quiet", not "has stopped": `idle` is a status of this product and it means
      // stopped, so the same word for the *evidence* reads as an outcome — and one that
      // sounds like something went wrong. The state model uses "goes quiet" for exactly this.
      "the board notices when a session goes quiet and infers the rest.",
    ],
  }[mode];
  if (!say) return;

  const [state, explanation] = say;
  line.replaceChildren();
  line.append("Right now, ");
  const em = document.createElement("em");
  em.textContent = "waiting for you";
  const strong = document.createElement("strong");
  strong.textContent = state;
  line.append(em, " is ", strong, " — ", explanation);
  line.dataset.mode = mode;
  line.hidden = false;
}

/** Which step to open on depends on what is already installed (FR-54). */
async function start() {
  const state = await invoke("hook_state");
  applyTheme(state.chrome, state.theme);
  renderMode(state.mode);
  document.getElementById("settings-path").textContent = state.settings_path;
  document.getElementById("refusal-path").textContent = state.settings_path;

  // Asked for by name, from the board's menu. It comes first because the diagnosis is the
  // one screen that is worth showing when something *is* broken — including the missing
  // helper below, which it reports as one row among four rather than as a dead end.
  const asked = new URLSearchParams(location.search).get("step");
  if (asked === "diagnose") {
    await toDiagnose();
    return;
  }
  if (asked === "reveal") {
    await toReveal();
    return;
  }
  if (asked === "shortcuts") {
    await toShortcuts();
    return;
  }

  // The one problem setup cannot write its way out of: the program the entries would name
  // is not there. Saying so up front beats writing five entries that point at nothing.
  if (!state.helper_found) {
    refuse(
      "The helper program that came with this app is not where it should be, so there is " +
        "nothing to point the settings at. Reinstalling the app puts it back.",
    );
    return;
  }

  if (state.installed === "current") {
    await toRemove();
  } else {
    show("explain");
  }
}

// Asked for while the window is already open. Reloading it at the diagnosis URL would throw
// away whatever the user is part-way through, so the Rust side names the screen it wants and
// this switches to it. Nothing is lost by switching: the only step that writes is the one
// behind the button that writes, and the diagnosis leads back to it.
window.__TAURI__?.event
  ?.listen("step", (event) => {
    if (event.payload === "diagnose") toDiagnose();
    if (event.payload === "reveal") toReveal();
    if (event.payload === "shortcuts") toShortcuts();
  })
  .catch(() => {});

start();
