//! TC-101 — raising a window must not rearrange it (FR-33).
//!
//! `SW_RESTORE` is how a *minimised* window is brought back, and it was being sent to every
//! window the board raised. On a maximised window it does something else entirely: it
//! un-maximises it. So clicking a row to glance at a session shrank the editor the user was
//! working in, back to whatever size it had before they maximised it — the board rearranging
//! the windows it exists to point at.
//!
//! The case needs a real window, because the bug is in what Windows does with a flag rather
//! than in anything this crate computes. It creates one from the built-in `STATIC` class —
//! no window class to register, no message loop to run — maximises it, raises it, and asks
//! whether it is still maximised.
//!
//! TC-106 is the other half of the same rule, added 2026-08-28. A window that is on screen is
//! left exactly as it is; a **minimised** one cannot be, because it has to come back before
//! it can be the foreground window. What it comes back as is a decision, and it is
//! *maximised*: a window the user put away and is now being sent to is one they are going to
//! read (FR-33, amended).
#![cfg(windows)]

use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IsIconic, IsZoomed, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
    SW_SHOWNORMAL, ShowWindow, WS_OVERLAPPEDWINDOW,
};

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A top-level window in the state named, or `None` where there is no desktop to put one on.
///
/// The `None` arm is not a way to make a case pass quietly: it prints, and it exists because
/// a headless agent has no window station. Where a desktop exists — which is everywhere this
/// product runs — the cases run for real.
fn window_shown_as(how: i32, case: &str) -> Option<HWND> {
    let class = utf16("STATIC");
    let title = utf16("mcv-board window case");
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            400,
            300,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut::<c_void>(),
            std::ptr::null(),
        )
    };
    if window.is_null() {
        println!("{case} skipped: this machine has no desktop to create a window on");
        return None;
    }
    unsafe { ShowWindow(window, how) };
    Some(window)
}

fn maximised_window() -> Option<HWND> {
    let window = window_shown_as(SW_SHOWMAXIMIZED, "TC-101")?;
    if unsafe { IsZoomed(window) } == 0 {
        unsafe { DestroyWindow(window) };
        println!("TC-101 skipped: the window would not maximise");
        return None;
    }
    Some(window)
}

/// TC-101 (FR-33) — a maximised window is still maximised after being raised.
#[test]
fn tc_101_raising_does_not_unmaximise() {
    let Some(window) = maximised_window() else {
        return;
    };

    let raised = mcv_board::win::raise(window as isize);

    let still_maximised = unsafe { IsZoomed(window) } != 0;
    unsafe { DestroyWindow(window) };

    assert!(
        still_maximised,
        "the window came back to its pre-maximised size. Raising a window must bring it \
         forward and change nothing else — the user clicked to look at a session, not to \
         have their editor resized (raised={raised})",
    );
}

/// TC-106 (FR-33, amended 2026-08-28) — a **minimised** window comes back maximised.
///
/// The one case where leaving the window alone is not on offer: it has to be un-minimised
/// before it can be the foreground window. `SW_RESTORE` would hand back whatever size it had
/// before it was put away, which for a window the user is now being *sent to* is a window
/// they then have to resize themselves.
#[test]
fn tc_106_a_minimised_window_comes_back_maximised() {
    let Some(window) = window_shown_as(SW_SHOWMINIMIZED, "TC-106") else {
        return;
    };
    if unsafe { IsIconic(window) } == 0 {
        unsafe { DestroyWindow(window) };
        println!("TC-106 skipped: the window would not minimise");
        return;
    }

    let raised = mcv_board::win::raise(window as isize);

    let iconic = unsafe { IsIconic(window) } != 0;
    let maximised = unsafe { IsZoomed(window) } != 0;
    unsafe { DestroyWindow(window) };

    assert!(
        !iconic,
        "a raised window must not still be minimised (raised={raised})"
    );
    assert!(
        maximised,
        "a minimised window is sent back maximised, not restored to the size it had before \
         it was put away (raised={raised})"
    );
}

/// TC-106b — and a window that is merely *normal* is left normal. The rule is that only a
/// minimised window is touched at all; this is the case that would break first if that ever
/// became "always maximise".
#[test]
fn tc_106b_a_normal_window_is_left_alone() {
    let Some(window) = window_shown_as(SW_SHOWNORMAL, "TC-106b") else {
        return;
    };

    let raised = mcv_board::win::raise(window as isize);

    let iconic = unsafe { IsIconic(window) } != 0;
    let maximised = unsafe { IsZoomed(window) } != 0;
    unsafe { DestroyWindow(window) };

    assert!(
        !iconic,
        "raising must not minimise anything (raised={raised})"
    );
    assert!(
        !maximised,
        "a window that was on screen at its own size must stay that size — the user clicked \
         to look at a session, not to have their editor resized (raised={raised})"
    );
}
