// Rendering the board.
//
// Everything that required a decision was decided in Rust and arrives in the view model:
// which status a folded group stands for, how a title is cut, whether a workspace is one
// merged row or a header plus rows. This file lays those out and does nothing else — there
// is no palette here, no fitting, and no precedence.

const board = document.getElementById("board");

/** Draws a whole board. */
function render(view) {
  document.documentElement.dataset.theme = view.theme;

  const html = document.documentElement;
  html.dataset.motion = view.inspect.still ? "still" : "live";
  // Two multipliers, and the stylesheet has one number: the development zoom, and the size
  // the user chose (§2.9). Multiplied here rather than in CSS so that the one place they meet
  // is the same place the window size is computed from — `BoardView::drawn`.
  html.style.setProperty(
    "--zoom",
    String((view.inspect.zoom || 1) * ((view.scale || 100) / 100)),
  );

  const root = document.documentElement.style;
  root.setProperty("--board", view.chrome.board);
  root.setProperty("--hairline", view.chrome.hairline);
  root.setProperty("--text", view.chrome.text);
  root.setProperty("--secondary", view.chrome.secondary);
  root.setProperty("--edge-colour", view.chrome.edge);
  root.setProperty("--titlebar", view.chrome.titlebar);
  root.setProperty("--titlebar-hairline", view.chrome.titlebar_hairline);

  board.title = modeTooltip(view.mode);

  lastMeasuredWorkspaces = view.fully_folded
    ? view.groups.map((group) => group.label).join("\0")
    : null;

  // What the board should now be, in order, each row keyed by the thing it is *about* rather
  // than by its position — a session keeps its row when the group above it gains one.
  const wanted = [];
  for (const group of view.groups) {
    if (group.merged) {
      wanted.push({
        key: "m:" + group.key,
        draw: (row) => mergedRow(row, group, view.fully_folded),
      });
      continue;
    }
    wanted.push({ key: "g:" + group.key, draw: (row) => groupRow(row, group) });
    if (!group.folded) {
      for (const session of group.sessions) {
        wanted.push({ key: "s:" + session.id, draw: (row) => sessionRow(row, session) });
      }
    }
  }
  reconcile(wanted);
}

/**
 * Brings the board to the wanted set of rows **without rebuilding it**.
 *
 * This is not an optimisation. `replaceChildren` every time was cheap enough — but a CSS
 * animation belongs to an element, and replacing the element restarts it from zero. The board
 * repaints whenever anything in the view changes, and something always does: the time column
 * ticks. So the halo on `awaiting_user` was being restarted roughly once a second, part way
 * through its 0.9 s cycle, for as long as a session waited. That is what "the animation is
 * janky" was — not the machine, and not the easing.
 *
 * Rows are matched by key and updated in place, so an element survives everything except the
 * row it stands for going away.
 */
function reconcile(wanted) {
  const existing = new Map();
  for (const row of board.children) existing.set(row.dataset.rowKey, row);

  let previous = null;
  for (const { key, draw } of wanted) {
    let row = existing.get(key);
    if (row) {
      existing.delete(key);
    } else {
      row = document.createElement("div");
      row.dataset.rowKey = key;
    }
    draw(row);
    // Put it after the row that precedes it. Already there is the common case, and moving a
    // node that is already in place is not free — it detaches and re-attaches it, restarting
    // any animation on it, which is the very thing this function exists to avoid.
    //
    // Elements rather than nodes on both sides: the map above is built from `children`, and
    // comparing that against `nextSibling` would go wrong the day anything puts a text node
    // between two rows.
    const shouldFollow = previous ? previous.nextElementSibling : board.firstElementChild;
    if (row !== shouldFollow) board.insertBefore(row, shouldFollow);
    previous = row;
  }

  // Whatever is left over stands for a row that is gone.
  for (const row of existing.values()) row.remove();
}

/**
 * One pulse of the panel's edge, to say the board has arrived.
 *
 * The class is removed first so that a second press restarts it: an animation that is already
 * running ignores the class being set again, and pressing the key twice because you did not
 * spot it the first time is exactly when it has to fire.
 */
function flashEdge() {
  const panel = document.getElementById("panel");
  if (!panel) return;
  panel.classList.remove("flash");
  // Reading a layout property is what makes the removal take effect before the class is set
  // again; without it the browser collapses the two into no change at all.
  void panel.offsetWidth;
  panel.classList.add("flash");
}

/**
 * What the board says about itself on hover (FR-55).
 *
 * The board has no menu yet — right-click opens the hook setup directly — so the mode lives
 * where the rest of the board's detail lives, in a tooltip. Discreet on purpose:
 * `session-state-model.md` §4.1 asks the product to say when hooks are off "once, discreetly,
 * not as a nag", and a badge on a 20 px row would be neither.
 */
function modeTooltip(mode) {
  switch (mode) {
    case "exact":
      return "Waiting for you is exact — sessions report it themselves.";
    case "went-quiet":
      return (
        "Waiting for you is being worked out from a pause: the reporting was set up but " +
        "has gone quiet. Right-click to check it."
      );
    default:
      return "Waiting for you is worked out from a pause. Right-click to make it exact.";
  }
}

/** A workspace header: roll-up indicator, name, live count, chevron. */
function groupRow(row, group) {
  shape(row, group.folded ? "row group folded" : "row group", { key: group.key });
  cells(row, [
    ["indicator", group.rollup],
    ["name", group.label, group.label],
    ["column", String(group.sessions.length)],
    ["chevron", group.folded ? "›" : "˅"],
  ]);
  return row;
}

/** A session inside an open group. */
function sessionRow(row, session) {
  shape(row, "row session", { id: session.id });
  cells(row, [
    ["indicator", session.status],
    ["title", session.title, session.full_title],
    ["column", session.time],
    ["chevron", ""],
  ]);
  return row;
}

/**
 * A workspace with exactly one session: no header, no count, no chevron (§2.3).
 *
 * Its title is hidden only while the whole board is folded, which is what lets the board
 * narrow to its workspace names — the full title stays reachable on hover in both states.
 */
function mergedRow(row, group, fullyFolded) {
  const session = group.sessions[0];
  shape(row, "row merged", { key: group.key, id: session.id });
  const wanted = [
    ["indicator", session.status],
    ["name", group.label, session.full_title],
  ];
  if (!fullyFolded) {
    wanted.push(["title", session.title, session.full_title]);
    wanted.push(["column", session.time]);
  }
  wanted.push(["chevron", ""]);
  cells(row, wanted);
  return row;
}

/**
 * The row's own class, and the ids a click reads back off it.
 *
 * Anything the row is *not* given is removed rather than left behind. A row is only ever
 * reused for the same kind of thing — the keys see to that — so a stale `data-id` cannot
 * happen today; it would be a click on a session that no longer exists if it ever did, and
 * that is not a failure worth leaving one refactor away.
 */
const ROW_DATA = ["key", "id"];

function shape(row, className, data) {
  if (row.className !== className) row.className = className;
  for (const name of ROW_DATA) {
    const value = data[name];
    if (value === undefined) {
      delete row.dataset[name];
    } else if (row.dataset[name] !== value) {
      row.dataset[name] = value;
    }
  }
}

/**
 * Fills a row's cells, reusing the elements already in it.
 *
 * The cells of a row never change kind — a session row is always indicator, title, time,
 * chevron — so position is enough to match them. The one row whose shape moves is a merged
 * row when the board folds, which drops two cells from the middle; that one is rebuilt.
 */
function cells(row, wanted) {
  // Same number of cells is not the same *kinds* of cell. It is today, because the only row
  // whose shape moves is a merged one folding, and that changes the count — but a row rebuilt
  // on a count alone would carry a cell's old text into a cell that now means something else,
  // and the check is one comparison.
  const sameShape =
    row.childElementCount === wanted.length &&
    wanted.every(([kind], i) => row.children[i].dataset.cell === kind);
  if (!sameShape) row.replaceChildren();

  wanted.forEach(([kind, value, tooltip], i) => {
    let cell = row.children[i];
    if (!cell) {
      cell = document.createElement("div");
      cell.dataset.cell = kind;
      row.append(cell);
    }
    if (kind === "indicator") {
      paintIndicator(cell, value);
      return;
    }
    if (cell.className !== kind) cell.className = kind;
    if (cell.textContent !== value) cell.textContent = value;
    // The full title is always reachable (FR-22), including on a row whose title is not drawn.
    const title = tooltip || "";
    if (cell.title !== title) cell.title = title;
  });
}

/**
 * Tells the Rust side how wide a folded board needs to be (§2.5).
 *
 * Measured here because only the renderer knows: the font is proportional, so "the longest
 * workspace name" is a number of pixels nobody else can produce. The *rules* — two discrete
 * widths, clamped to [140, 300] — stay on the Rust side.
 *
 * Reported only when the set of workspaces changes. Re-measuring on every status or title
 * change would make the board twitch in the corner of the eye all day, which is exactly what
 * ADR-0015 chose two widths to avoid.
 */
let lastMeasuredWorkspaces = null;
let lastReported = null;

function measureFoldedWidth() {
  if (lastMeasuredWorkspaces === null || lastMeasuredWorkspaces === lastReported) return;

  // Measured off-screen, one label at a time. Measuring the laid-out rows does not work:
  // the name cell is `flex: 1 1 auto`, so it has already been stretched to fill the row and
  // reports the width the board currently has rather than the width its text needs.
  const ruler = document.createElement("span");
  ruler.style.cssText =
    "position:absolute;visibility:hidden;white-space:nowrap;font:inherit;font-weight:600";
  board.append(ruler);

  let widestName = 0;
  let widestColumn = 0;
  for (const row of board.querySelectorAll(".row")) {
    const name = row.querySelector(".name");
    if (name) {
      ruler.textContent = name.textContent;
      widestName = Math.max(widestName, ruler.getBoundingClientRect().width);
    }
    const column = row.querySelector(".column");
    if (column) {
      ruler.textContent = column.textContent;
      widestColumn = Math.max(widestColumn, ruler.getBoundingClientRect().width);
    }
  }
  ruler.remove();
  if (widestName <= 0) return;

  // §2.5: the longest workspace name, plus the indicator, the count and the chevron. The
  // chrome comes from the same custom properties the layout uses, so the two cannot drift.
  const px = (name) =>
    parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  const chrome =
    px("--pad-horizontal") * 2 +
    px("--indicator") +
    px("--gap-indicator-text") +
    px("--gap-text-column") +
    px("--chevron-clearance") +
    px("--chevron-column");

  // The title line is content too. A folded board narrows to the longest workspace name
  // (§2.5) — but it may not narrow past its own title, or the one piece of text that is
  // always there would be the one piece that gets cut.
  const handle = document.querySelector("#titlebar .handle");
  const titleFloor = handle
    ? handle.getBoundingClientRect().width + px("--pad-horizontal") * 2
    : 0;

  lastReported = lastMeasuredWorkspaces;
  invoke("report_folded_width", {
    width: Math.ceil(Math.max(widestName + widestColumn + chrome, titleFloor)),
  });
}

/**
 * Paints an indicator, touching only what differs.
 *
 * Writing the same class back would restart the animation attached to it, which is the whole
 * reason this file updates rather than rebuilds — see `reconcile`.
 */
function paintIndicator(mark, status) {
  const className = "indicator " + status.silhouette + " " + status.motion;
  if (mark.className !== className) mark.className = className;
  if (mark.style.getPropertyValue("--status") !== status.colour) {
    mark.style.setProperty("--status", status.colour);
  }
  if (mark.title !== status.name) mark.title = status.name;
}

window.renderBoard = render;

// After a repaint, once the browser has laid the rows out.
const observer = new ResizeObserver(() => measureFoldedWidth());
observer.observe(board);

// The Rust side hands the first view over on startup and every change after it.
if (window.__TAURI__) {
  const { event, core } = window.__TAURI__;
  event.listen("board", (message) => render(message.payload));
  event.listen("raised", (message) => acknowledge(message.payload));
  // Summoned by its shortcut: the board has just arrived on this display, and the user is
  // looking somewhere else on it (hotkeys.rs).
  event.listen("flash", () => flashEdge());
  core.invoke("current_board").then(render);
}

/**
 * Says what a click did (FR-33, TC-75, TC-76).
 *
 * A raise that worked gets a brief ring; one that did not gets the hollow outline and a
 * tooltip explaining. **Both are required.** A board that showed nothing on failure would
 * leave the user staring at an editor that never came forward, wondering whether they had
 * missed the click — and the whole reason the board can tell them is that the outcome was
 * measured rather than assumed.
 */
function acknowledge({ id, outcome }) {
  const row = board.querySelector(`[data-id="${CSS.escape(id)}"]`);
  if (!row) return;
  const mark = row.querySelector(".indicator");
  if (!mark) return;

  const failed = outcome !== "raised";
  mark.classList.add(failed ? "raise-failed" : "raise-worked");

  // The status tooltip is put back afterwards. Overwriting it and walking away left the
  // indicator explaining a failure that had long since scrolled out of the user's mind.
  const wasTitled = mark.title;
  if (failed) {
    mark.title =
      outcome === "not-found"
        ? "no VS Code window could be matched to this session"
        : "the window did not come forward";
  }

  // 150 ms for the acknowledgement, 2 s for the failure: long enough to be read, short
  // enough that the board is not still talking about it when the user looks back.
  setTimeout(() => {
    mark.classList.remove("raise-failed", "raise-worked");
    mark.title = wasTitled;
  }, failed ? 2000 : 150);
}

// ------------------------------------------------------------------ interaction

/**
 * Press-and-move drags the board; press-and-release acts on the row (FR-27, TC-77).
 *
 * The threshold is 4 px, from ui-overlay.md §6: below it the press was a click, above it the
 * user meant to move the window. Tauri's own `data-tauri-drag-region` starts dragging on
 * mousedown, which would make every row unclickable — the board has no title bar to put a
 * drag handle in, so the whole body has to be both.
 */
const DRAG_THRESHOLD_PX = 4;

function wireInteraction() {
  let origin = null;
  let dragging = false;

  document.addEventListener("mousedown", (event) => {
    if (event.button !== 0) return;
    origin = { x: event.screenX, y: event.screenY, row: event.target.closest(".row") };
    dragging = false;
  });

  document.addEventListener("mousemove", (event) => {
    if (!origin || dragging) return;
    const moved = Math.hypot(event.screenX - origin.x, event.screenY - origin.y);
    if (moved <= DRAG_THRESHOLD_PX) return;
    dragging = true;
    // From here the window follows the pointer. The webview loses the mouse with it, so the
    // matching mouseup may never arrive — the press is finished as far as this code is
    // concerned, and leaving it half-recorded would let the *next* release be read as a
    // click on a row the user never pressed.
    origin = null;
    window.__TAURI__?.window?.getCurrentWindow?.().startDragging?.();
  });

  // Right-click opens the board's menu (ui-overlay.md §6). It is a native menu, built in
  // Rust: the board draws everything else itself, but a menu wants the platform's keyboard
  // handling and its habits — and the tray will carry the same one (FR-32).
  document.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    invoke("open_menu");
  });

  document.addEventListener("mouseup", () => {
    if (origin && !dragging && origin.row) act(origin.row);
    origin = null;
    dragging = false;
  });

  // The same reasoning for every other way a press can end somewhere this code cannot see.
  for (const ending of ["mouseleave", "blur", "dragstart"]) {
    window.addEventListener(ending, () => {
      origin = null;
      dragging = false;
    });
  }
}

/**
 * What a click on a row does — and, just as importantly, what it does not.
 *
 * §2.4: **headers fold, rows raise.** A row that did both would have to guess which the user
 * meant, and guessing wrong either loses the user's place or takes them somewhere they were
 * not going (TC-56d).
 */
function act(row) {
  if (row.classList.contains("group")) {
    invoke("toggle_fold", { key: row.dataset.key });
    return;
  }
  // A session row or a merged row raises that session's window (FR-33). The merged row is a
  // workspace *and* a session, and it raises: it is a row, not a header.
  invoke("raise_session", { id: row.dataset.id });
}

function invoke(command, args) {
  window.__TAURI__?.core?.invoke(command, args);
}

wireInteraction();
