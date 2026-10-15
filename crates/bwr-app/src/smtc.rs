//! Windows System Media Transport Controls.
//!
//! This is what puts play/pause/next/previous buttons on the taskbar
//! thumbnail and in the Windows 11 media flyout, and lets hardware media
//! keys drive the app even when it is in the background.

#![cfg(windows)]

use std::sync::mpsc;

use windows::core::{HSTRING, Result};
use windows::Foundation::TypedEventHandler;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::{
    ISystemMediaTransportControlsInterop, RoGetActivationFactory, RoInitialize,
    RO_INIT_SINGLETHREADED,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtcEvent {
    Play,
    Pause,
    Next,
    Previous,
}

pub struct Smtc {
    smtc: SystemMediaTransportControls,
    // Must outlive the registration; Windows keeps a reference but keeping
    // our copy alive is what guarantees the delegate object stays valid.
    #[allow(dead_code)]
    handler: TypedEventHandler<SystemMediaTransportControls, SystemMediaTransportControlsButtonPressedEventArgs>,
    rx: mpsc::Receiver<SmtcEvent>,
    status: Option<MediaPlaybackStatus>,
    nav: Option<(bool, bool)>,
    meta: Option<(String, String)>,
}

impl Smtc {
    pub fn new(hwnd: HWND) -> Result<Self> {
        // winit already puts the UI thread in an STA; RoInitialize is a no-op
        // then, but call it anyway so this is safe if that ever changes.
        let _ = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) };

        let interop: ISystemMediaTransportControlsInterop = unsafe {
            RoGetActivationFactory(&HSTRING::from(
                "Windows.Media.SystemMediaTransportControls",
            ))?
        };
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };

        smtc.SetIsEnabled(true)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;

        let (tx, rx) = mpsc::channel::<SmtcEvent>();
        let handler = TypedEventHandler::new(
            move |_: &Option<SystemMediaTransportControls>,
                  args: &Option<SystemMediaTransportControlsButtonPressedEventArgs>| {
                let Some(args) = args else { return Ok(()) };
                let ev = match args.Button()? {
                    SystemMediaTransportControlsButton::Play => SmtcEvent::Play,
                    SystemMediaTransportControlsButton::Pause => SmtcEvent::Pause,
                    SystemMediaTransportControlsButton::Next => SmtcEvent::Next,
                    SystemMediaTransportControlsButton::Previous => SmtcEvent::Previous,
                    _ => return Ok(()),
                };
                crate::diag::log(&format!("[smtc] button event: {ev:?}"));
                let _ = tx.send(ev);
                Ok(())
            },
        );
        smtc.ButtonPressed(&handler)?;

        Ok(Self {
            smtc,
            handler,
            rx,
            status: None,
            nav: None,
            meta: None,
        })
    }

    pub fn try_recv(&self) -> Option<SmtcEvent> {
        self.rx.try_recv().ok()
    }

    pub fn set_status(&mut self, status: MediaPlaybackStatus) {
        if self.status == Some(status) {
            return;
        }
        self.status = Some(status);
        let _ = self.smtc.SetPlaybackStatus(status);
    }

    pub fn set_navigation(&mut self, prev: bool, next: bool) {
        if self.nav == Some((prev, next)) {
            return;
        }
        self.nav = Some((prev, next));
        let _ = self.smtc.SetIsPreviousEnabled(prev);
        let _ = self.smtc.SetIsNextEnabled(next);
    }

    pub fn set_metadata(&mut self, title: &str, artist: &str) {
        if let Some((t, a)) = &self.meta {
            if t == title && a == artist {
                return;
            }
        }
        self.meta = Some((title.to_string(), artist.to_string()));
        if let Ok(updater) = self.smtc.DisplayUpdater() {
            let _ = updater.SetType(MediaPlaybackType::Music);
            if let Ok(mp) = updater.MusicProperties() {
                let _ = mp.SetTitle(&HSTRING::from(title));
                let _ = mp.SetArtist(&HSTRING::from(artist));
            }
            let _ = updater.Update();
        }
    }
}
