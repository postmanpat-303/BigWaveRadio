//! Global media-key hotkeys (works even when the app window is unfocused).
//!
//! Registers the hardware media keys on a dedicated thread that owns a Windows
//! message loop (WM_HOTKEY is delivered to the thread that called
//! RegisterHotKey). Key presses are forwarded to the UI over an mpsc channel.

#![cfg(windows)]

use std::sync::mpsc;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_B,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    /// Bring the window back from the tray (Ctrl+Alt+B).
    Show,
}

const ID_PLAY_PAUSE: i32 = 1;
const ID_NEXT: i32 = 2;
const ID_PREVIOUS: i32 = 3;
const ID_SHOW: i32 = 4;

/// Spawn the hotkey thread. Returns a receiver that yields one `MediaKey`
/// per key press; if registration fails (e.g. another app grabbed the keys)
/// the channel simply never receives.
pub fn spawn() -> mpsc::Receiver<MediaKey> {
    let (tx, rx) = mpsc::channel::<MediaKey>();
    std::thread::spawn(move || unsafe {
        // A thread that calls RegisterHotKey must pump messages.
        let ok_pp = RegisterHotKey(
            None,
            ID_PLAY_PAUSE,
            MOD_NOREPEAT,
            VK_MEDIA_PLAY_PAUSE.0 as u32,
        )
        .is_ok();
        let ok_next =
            RegisterHotKey(None, ID_NEXT, MOD_NOREPEAT, VK_MEDIA_NEXT_TRACK.0 as u32).is_ok();
        let ok_prev =
            RegisterHotKey(None, ID_PREVIOUS, MOD_NOREPEAT, VK_MEDIA_PREV_TRACK.0 as u32).is_ok();
        // Ctrl+Alt+B always registers (independent of media keys): it is the
        // guaranteed way back when the window hides to the tray.
        let ok_show = RegisterHotKey(None, ID_SHOW, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_B.0 as u32).is_ok();
        if !ok_pp && !ok_next && !ok_prev && !ok_show {
            return; // no keys available: channel stays silent
        }

        let mut msg = MSG::default();
        loop {
            let got = GetMessageW(&mut msg, None, 0, 0);
            if got.as_bool() {
                if msg.message == WM_HOTKEY {
                    let key = match msg.wParam.0 as i32 {
                        ID_PLAY_PAUSE => MediaKey::PlayPause,
                        ID_NEXT => MediaKey::Next,
                        ID_SHOW => MediaKey::Show,
                        _ => MediaKey::Previous,
                    };
                    if tx.send(key).is_err() {
                        break; // app gone
                    }
                }
            } else {
                break; // error, or WM_QUIT
            }
        }

        let _ = UnregisterHotKey(None, ID_PLAY_PAUSE);
        let _ = UnregisterHotKey(None, ID_NEXT);
        let _ = UnregisterHotKey(None, ID_PREVIOUS);
        if ok_show {
            let _ = UnregisterHotKey(None, ID_SHOW);
        }
    });
    rx
}
