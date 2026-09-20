//! The board.
//!
//! A single always-on-top window showing every Claude Code session across every VS Code
//! window, so that "which session needs me?" is answerable at a glance
//! (`docs/ui-overlay.md`).
//!
//! It watches the sessions running now: the registry says which exist, the process table
//! says which are alive, each transcript is followed by byte offset, and the state machine
//! turns that into a status. `--demo` draws a fixed board from the recordings instead, which
//! is what the screenshots are of.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use mcv_board::view::Inspect;
use mcv_core::color::{contrast_ratio, lab, to_hex};
use mcv_core::palette::{Status, Theme, indicator, ink_index, rendering};

fn main() {
    let mut theme = Theme::Dark;
    let mut inspect = Inspect::default();
    // Live by default. The recordings are a development aid, and a board that showed them
    // to a user would be lying about what is running.
    let mut live = true;
    // Open the setup window as well, so it can be photographed and reviewed like the board.
    // Both of its screens are otherwise reached from the board's menu, which a screenshot
    // cannot open.
    let mut setup: Option<mcv_board::setup::Screen> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            // The palette, as text. It predates the window and stays because it is the
            // cheapest way to check §3.2 against what the code holds.
            "--palette" => return print_palette(),
            // The tray icon is drawn by hand into a buffer, so it is the one thing on this
            // product nobody can see by running it. Same argument as --palette.
            "--tray-icons" => return print_tray_icons(),
            "--check-raise" => return check_raise(),
            "--demo" => live = false,
            "--light" => theme = Theme::Light,
            // Freeze the animations: a screenshot of a pulsing indicator is a different
            // colour every time, and this is also the reduced-motion rendering (§3.1).
            "--still" => inspect.still = true,
            // The four screen flags share one mapping with the second-launch path, so a flag
            // cannot come to mean one thing at startup and another when a board is running.
            flag if mcv_board::setup::screen_for_flag(flag).is_some() => {
                setup = mcv_board::setup::screen_for_flag(flag);
            }
            "--zoom" => {
                inspect.zoom = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| usage("--zoom wants a whole number"));
            }
            other => usage(&format!("unknown option {other:?}")),
        }
    }

    mcv_board::app::run(theme, inspect, live, setup);
}

fn usage(problem: &str) -> ! {
    eprintln!("mcv-board: {problem}");
    eprintln!("  (no option)     the board, watching the sessions running now");
    eprintln!("  --demo          drawn from the recordings instead, for looking at");
    eprintln!("  --light         the light theme");
    eprintln!("  --still         no animation: a stable picture, and the reduced-motion one");
    eprintln!("  --zoom <n>      draw everything at n times size, to inspect a silhouette");
    eprintln!("  --setup         also open the hook setup window, for looking at");
    eprintln!("  --diagnose      ...open it on the diagnosis panel instead");
    eprintln!("  --reveal-setup  ...or on the session-reveal explanation");
    eprintln!("  --shortcuts     ...or on the screen that changes the two keys");
    eprintln!("  --palette       print the palette and its measurements");
    eprintln!("  --tray-icons    print every tray icon as pixels, for looking at");
    eprintln!("  --check-raise   walk the session-to-window chain and print each step");
    std::process::exit(2)
}

fn print_palette() {
    for theme in Theme::ALL {
        let board = theme.board();
        println!();
        println!("{} board {}", theme.name(), to_hex(board));
        println!(
            "  {:<14} {:<9} {:>5} {:>10} {:>7}  {:<22} motion",
            "status", "hex", "L*", "contrast", "ink", "silhouette"
        );
        for status in Status::ALL {
            let colour = indicator(status, theme);
            let r = rendering(status);
            let silhouette = format!("{:?}", r.silhouette);
            println!(
                "  {:<14} {:<9} {:>5.0} {:>9.2}:1 {:>7.0}  {silhouette:<22} {:?}",
                status.name(),
                to_hex(colour),
                lab(colour)[0],
                contrast_ratio(colour, board),
                ink_index(status, theme),
                r.motion,
            );
        }
    }
}

/// Every tray icon, as pixels, one JSON object per line.
///
/// The icon is composited into a buffer and handed straight to the shell, so unlike every
/// other surface this product draws there is no way to look at it except by running the
/// product and squinting at a taskbar — at 16 px, in whatever theme the machine happens to
/// be in. This prints all twelve so they can be put side by side and reviewed like the board
/// is (`.notes/tools/`), and it is the same argument that keeps `--palette` around: the
/// cheapest way to check a picture against what the code actually holds.
fn print_tray_icons() {
    use mcv_board::tray::{SIZE, icon_pixels};

    for theme in Theme::ALL {
        for status in Status::ALL {
            let hex: String = icon_pixels(status, theme)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            println!(
                r#"{{"theme":"{}","status":"{}","size":{SIZE},"rgba":"{hex}"}}"#,
                theme.name(),
                status.name(),
            );
        }
    }
}

/// Walks the chain that FR-33 rests on, for every session running now, and prints what each
/// step produced.
///
/// The failure this diagnoses is the one that looks like nothing happening: a click that
/// raises no window could have failed at any of four places, and from the outside they are
/// identical. Printing the steps is how someone finds out which — including whether the
/// answer is simply "you renamed your window title", which is a supported thing to have done
/// ([ADR-0009](../../../docs/adr/0009-map-a-session-to-its-window.md)).
fn check_raise() {
    use mcv_board::raise::{
        only_unclaimed_window, read_locks, title_matches, title_needle, title_needle_of,
    };
    use mcv_board::watch::live::{claude_dir, read_registry};
    use mcv_board::watch::process::ProcessTable;

    let Some(claude) = claude_dir() else {
        eprintln!("mcv-board: cannot find ~/.claude");
        return;
    };

    let entries = read_registry(&claude.join("sessions"));
    let mut table = ProcessTable::new();
    let live = table.live_among(&entries.iter().map(|e| e.pid).collect::<Vec<_>>());
    let locks = read_locks(&claude.join("ide"));
    println!(
        "{} registry entries, {} running, {} lock files",
        entries.len(),
        live.len(),
        locks.len()
    );

    for entry in entries.iter().filter(|e| e.is_editor_session()) {
        if !live.iter().any(|p| p.matches(entry)) {
            continue;
        }
        println!();
        println!("session {} in {}", entry.session_id, entry.cwd);
        println!("  pid            {}", entry.pid);

        let Some(parent) = table.parent_of(entry.pid) else {
            println!("  parent         (none) — the chain stops here");
            continue;
        };
        println!("  parent         {parent}  (the extension host)");

        #[cfg(windows)]
        {
            let ports = mcv_board::win::listening_ports_of(parent);
            if ports.is_empty() {
                println!("  listening port (none) — the parent is not the extension host");
                continue;
            }
            // Every port, and what each one's lock says. The host listens on several and only
            // one of them is Claude Code's; printing only the chosen one is how this
            // diagnostic reported a real failure as "no lock claims that port" while the lock
            // sat there under a different number (ADR-0027).
            let mut needle = None;
            for port in &ports {
                let lock = locks.iter().find(|lock| lock.port == *port);
                let verdict = match lock {
                    None => "no lock file".to_owned(),
                    Some(lock) if lock.workspace_folders.is_empty() => {
                        "lock, but it names no folder".to_owned()
                    }
                    Some(lock) => format!("lock -> {}", lock.workspace_folders.join(", ")),
                };
                let chosen = if needle.is_none() {
                    needle = title_needle(&locks, *port);
                    if needle.is_some() {
                        "  <- this one"
                    } else {
                        ""
                    }
                } else {
                    ""
                };
                println!("  port {port:<6}    {verdict}{chosen}");
            }

            let editor = table.parent_of(parent);
            let Some(needle) = needle else {
                // No folder open, so nothing to recognise. The board names this window by what
                // it cannot be instead (ADR-0028), and this walks the same arithmetic.
                println!("  looking for    (nothing — this window has no folder open)");
                let windows: Vec<(isize, String)> = mcv_board::win::visible_windows()
                    .into_iter()
                    .filter(|(window, _)| {
                        let owner = mcv_board::win::owner_of(*window);
                        editor.is_some_and(|editor| table.is_descendant_of(owner, editor))
                    })
                    .collect();
                // Step by step rather than one chain: each adapter would want its own mutable
                // borrow of the process table.
                let mut siblings: Vec<u32> = Vec::new();
                for other in &entries {
                    let Some(host) = table.parent_of(other.pid) else {
                        continue;
                    };
                    if host != parent && table.parent_of(host) == editor {
                        siblings.push(host);
                    }
                }
                siblings.sort_unstable();
                siblings.dedup();
                let claimed: Vec<String> = siblings
                    .iter()
                    .filter_map(|host| {
                        title_needle_of(&locks, &mcv_board::win::listening_ports_of(*host))
                    })
                    .collect();
                println!("  this editor    {} window(s)", windows.len());
                for (_, title) in &windows {
                    let taken = claimed.iter().any(|needle| title_matches(title, needle));
                    println!(
                        "    {} {title:?}",
                        if taken {
                            "[another session's]"
                        } else {
                            "[unclaimed]      "
                        }
                    );
                }
                match only_unclaimed_window(&windows, &claimed) {
                    Some(_) => println!("  by elimination one window is left — that is this one"),
                    None => println!(
                        "  by elimination (no single window left — cannot name it, so nothing                          is raised)"
                    ),
                }
                continue;
            };
            println!("  looking for    {needle:?} in a window title");

            for (window, title) in mcv_board::win::visible_windows() {
                if !title_matches(&title, &needle) {
                    continue;
                }
                let owner = mcv_board::win::owner_of(window);
                let ours = editor.is_some_and(|editor| table.is_descendant_of(owner, editor));
                // Both are printed, because the ones that are *rejected* are the interesting
                // half: a browser showing a pull request for a repository of the same name
                // matches the title and must not be raised.
                println!(
                    "  window         {} {title:?}",
                    if ours {
                        "[the editor]"
                    } else {
                        "[not the editor]"
                    }
                );
            }
        }
    }
}
