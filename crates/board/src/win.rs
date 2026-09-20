//! The Win32 half of raising a window (FR-33).
//!
//! Everything in this file talks to the operating system, and it is deliberately the only
//! file that does — [ADR-0004](../../../docs/adr/0004-use-tauri-and-rust.md) keeps the
//! platform in a thin module so the rest of the board, and all of the core, stays portable.
//!
//! Three things happen here, none of them clever:
//!
//! 1. Find which TCP port a process is listening on. That port names the window, because the
//!    extension host is per window and it is the process holding the port.
//! 2. Find a top-level window whose title carries the workspace's folder name.
//! 3. Bring it forward, and then **look** to see whether it came.
//!
//! Step 3's second half is the one worth insisting on. The return values here are worthless:
//! `FlashWindowEx` reported success on all three measured runs while raising nothing. Reading
//! `GetForegroundWindow` back after a moment is what lets the board tell the user the truth
//! (TC-75, TC-76).

#![cfg(windows)]

use std::ffi::c_void;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SW_MAXIMIZE, SetForegroundWindow, ShowWindow,
};

/// How long to keep waiting for the window to come forward before giving up on it.
///
/// 250 ms is [ADR-0009](../../../docs/adr/0009-map-a-session-to-its-window.md)'s measured
/// figure: long enough for the switch to have happened, short enough that a click still feels
/// answered. It is the **deadline**, not a pause — see [`GLANCE`].
const SETTLE: Duration = Duration::from_millis(250);

/// How often to look while waiting.
///
/// This used to be one `sleep(SETTLE)` followed by a single look, which answered the wrong
/// question — *was it foreground a quarter of a second later* rather than *did it come
/// forward* — and charged every raise the full 250 ms whether or not it had already arrived.
/// That cost is now visible: with the opt-in reveal on, nothing can be sent until the raise
/// is confirmed, so the pause sat between the window appearing and its session's tab coming
/// up ([ADR-0029](../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)).
///
/// Looking often instead answers sooner in the ordinary case — a window that is already
/// forward passes on the first glance — and the worst case is unchanged.
const GLANCE: Duration = Duration::from_millis(10);

/// Start a child process without giving it a console window.
///
/// The board runs two things through `cmd`: the editor's `bin/*.cmd` launcher, which carries
/// a reveal ([ADR-0029](../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)),
/// and the shell's file association, which opens the settings file. Both were spawned plainly
/// until 2026-08-29, and a console stood in front of the user for as long as they took — for
/// the reveal, the several seconds `cli.js` needs. An ambient board must not put a window in
/// the user's way (FR-30), and least of all one that is not even its own.
///
/// `cmd` hands its console down to what it runs, so setting this on `cmd` covers the launcher
/// and the `node` behind it too. `DETACHED_PROCESS` would also hide it, and changes more than
/// that — this one only declines the console.
pub const NO_CONSOLE: u32 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

/// Every IPv4 port `pid` is listening on, in the order the table gives them.
///
/// **All of them, because the extension host has several.** This returned only the first
/// until 2026-08-28, on a stated assumption — "the host listens on one" — that a live machine
/// disproved: one host was listening on 54851, 58915 and 59126, and only the last had a lock
/// file. The first was returned, no lock claimed it, and every click on that window's
/// sessions did nothing at all
/// ([ADR-0027](../../../docs/adr/0027-pick-the-editors-port-by-its-lock-file.md)).
///
/// Which of them is Claude Code's is not decidable here, and is not guessed at: the caller
/// asks the lock files, which are the authority on it.
#[must_use]
pub fn listening_ports_of(pid: u32) -> Vec<u16> {
    let mut size: u32 = 0;
    // First call sizes the buffer. It is expected to fail.
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &raw mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }
    if size == 0 {
        return Vec::new();
    }

    // A `Vec<u32>`, not a `Vec<u8>`. The table's rows are all DWORDs and therefore want
    // four-byte alignment, while a byte vector guarantees only one — casting a byte buffer
    // to the table type and dereferencing it is undefined behaviour, not merely untidy.
    let words = (size as usize).div_ceil(std::mem::size_of::<u32>());
    let mut buffer = vec![0u32; words];
    let result = unsafe {
        GetExtendedTcpTable(
            buffer.as_mut_ptr().cast::<c_void>(),
            &raw mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if result != 0 {
        return Vec::new();
    }

    // The table is a count followed by that many rows, laid out in the buffer.
    let table = buffer.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
    let count = unsafe { (*table).dwNumEntries } as usize;
    let rows = unsafe {
        std::ptr::addr_of!((*table).table)
            .cast::<windows_sys::Win32::NetworkManagement::IpHelper::MIB_TCPROW_OWNER_PID>()
    };

    let mut ports = Vec::new();
    for index in 0..count {
        let row = unsafe { &*rows.add(index) };
        if row.dwOwningPid == pid {
            // The port is stored in network byte order in the low half of the field.
            ports.push(u16::from_be((row.dwLocalPort & 0xFFFF) as u16));
        }
    }
    ports
}

/// Every visible top-level window's handle and title.
#[must_use]
pub fn visible_windows() -> Vec<(isize, String)> {
    let mut found: Vec<(isize, String)> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), std::ptr::from_mut(&mut found) as LPARAM);
    }
    found
}

/// Every top-level window's handle and title, **hidden ones included**.
///
/// The board hides itself rather than closing, so a board that is running and out of sight has
/// a window like any other — it simply fails `IsWindowVisible`. Anything asking *is there a
/// board on this machine* has to look here; anything asking *what can the user see* wants
/// [`visible_windows`].
#[must_use]
pub fn top_level_windows() -> Vec<(isize, String)> {
    let mut found: Vec<(isize, String)> = Vec::new();
    unsafe {
        EnumWindows(Some(collect_all), std::ptr::from_mut(&mut found) as LPARAM);
    }
    found
}

unsafe extern "system" fn collect(window: HWND, out: LPARAM) -> i32 {
    if unsafe { IsWindowVisible(window) } == 0 {
        return TRUE;
    }
    unsafe { collect_all(window, out) }
}

unsafe extern "system" fn collect_all(window: HWND, out: LPARAM) -> i32 {
    let mut text = [0u16; 512];
    let length = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) };
    if length > 0 {
        let title = String::from_utf16_lossy(&text[..length as usize]);
        let found = unsafe { &mut *(out as *mut Vec<(isize, String)>) };
        found.push((window as isize, title));
    }
    TRUE
}

/// Whether a window is the foreground one *now*.
///
/// Asked again immediately before the session URL is sent, and it is the whole safety of that
/// feature. The editor decides for itself which of its windows handles a URL, and it hands it
/// to the active one — so the board's only lever is to send it while the window it just
/// raised is still the active one. A URL that lands in the wrong window does not fail
/// quietly: it opens that session there
/// ([ADR-0029](../../../docs/adr/0029-reveal-the-session-through-the-editors-url-handler.md)).
#[must_use]
pub fn foreground_is(window: isize) -> bool {
    std::ptr::eq(unsafe { GetForegroundWindow() }, window as HWND)
}

/// The process that owns a window.
#[must_use]
pub fn owner_of(window: isize) -> u32 {
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(window as HWND, &raw mut pid) };
    pid
}

/// Brings a window forward, and reports whether it actually came.
///
/// Windows refuses `SetForegroundWindow` from a process that does not own the foreground,
/// which is most of the time for a board that never takes focus itself (FR-30). Attaching to
/// the foreground thread's input queue for the duration is the documented way around that —
/// and it is detached again immediately, because leaving two threads sharing an input queue
/// affects far more than one call.
#[must_use]
pub fn raise(window: isize) -> bool {
    let target = window as HWND;
    let foreground = unsafe { GetForegroundWindow() };

    let mut foreground_thread = 0;
    if !foreground.is_null() {
        foreground_thread = unsafe { GetWindowThreadProcessId(foreground, std::ptr::null_mut()) };
    }
    let this_thread = unsafe { GetCurrentThreadId() };

    let attached = foreground_thread != 0
        && foreground_thread != this_thread
        && unsafe { AttachThreadInput(this_thread, foreground_thread, TRUE) } != 0;

    unsafe {
        // **Only a minimised window is touched at all.** `SW_RESTORE` on a maximised window
        // un-maximises it: the user clicks a row to look at that session and the editor they
        // were working in shrinks to whatever size it last had. A window that is already on
        // screen — normal or maximised — is left exactly as it is, which is what FR-33 means
        // by not rearranging it.
        //
        // A minimised one cannot be left alone: it has to come back before it can be the
        // foreground window, so the only question is what it comes back *as*. It is
        // **maximised**, not restored to whatever it was before it was minimised. A window
        // the user put away and is now being sent to is a window they are going to read, and
        // `SW_RESTORE` would hand back a small one they then have to resize. This is the one
        // place the board changes a window's shape, and it is the case where leaving it alone
        // is not on offer.
        if IsIconic(target) != 0 {
            ShowWindow(target, SW_MAXIMIZE);
        }
        SetForegroundWindow(target);
    }

    if attached {
        unsafe { AttachThreadInput(this_thread, foreground_thread, 0) };
    }

    // Look, rather than trust. The return values above lie.
    let deadline = std::time::Instant::now() + SETTLE;
    loop {
        if foreground_is(window) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(GLANCE);
    }
}
