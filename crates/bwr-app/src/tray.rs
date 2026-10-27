//! Notification-area (tray) icon with a right-click menu.
//!
//! Lives on its own hidden window. The icon stays up for the lifetime of the
//! app: Windows removes the icons of dead processes automatically, and
//! [`Drop`] sends an explicit removal on a clean exit. (An earlier heartbeat
//! design that self-removed the icon when UI frames stalled was the cause of
//! the "app vanished and I had to End Task" bug — repaints can legitimately
//! pause, so the icon must not depend on them.)

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::mpsc;
use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO,
    NIM_ADD, NIM_DELETE, NIM_MODIFY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetWindowLongPtrW, PostMessageW, RegisterClassW,
    SetForegroundWindow, SetMenuDefaultItem, SetWindowLongPtrW, TrackPopupMenu, CW_USEDEFAULT,
    GWLP_USERDATA, HICON, ICONINFO, MF_SEPARATOR, MF_STRING, MSG, TPM_LEFTALIGN, TPM_RETURNCMD,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_QUIT,
    WM_RBUTTONUP, WNDCLASSW,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCmd {
    Show,
    PlayPause,
    Next,
    Previous,
    Quit,
}

const TRAY_MSG: u32 = WM_APP;
/// Click on our balloon/toast notification (shellapi.h: WM_USER + 5).
const NIN_BALLOONUSERCLICK: u32 = 0x0405;
const MENU_SHOW: usize = 1001;
const MENU_PLAY_PAUSE: usize = 1002;
const MENU_NEXT: usize = 1003;
const MENU_PREVIOUS: usize = 1004;
const MENU_QUIT: usize = 1005;

pub struct Tray {
    rx: mpsc::Receiver<TrayCmd>,
    hwnd: HWND,
    /// Icon installed; without it nothing may hide the main window.
    icon_ok: bool,
}

fn to_wchar<const N: usize>(s: &str) -> [u16; N] {
    let mut buf = [0u16; N];
    let v: Vec<u16> = s.encode_utf16().collect();
    let n = v.len().min(N - 1);
    buf[..n].copy_from_slice(&v[..n]);
    buf
}

impl Tray {
    /// Install the tray icon. Returns None if the icon could not be created.
    pub fn new() -> Option<Self> {
        let (tx, rx) = mpsc::channel::<TrayCmd>();
        // HWND is a raw pointer and not Send; pass it across as an isize.
        let (started_tx, started_rx) = mpsc::channel::<isize>();

        std::thread::spawn(move || tray_thread(tx, started_tx));

        // Wait for the icon to come up — on a busy machine (e.g. Defender
        // scanning a freshly installed exe) window creation can take a few
        // seconds. Never wait for the thread itself, which keeps running.
        let hwnd = started_rx
            .recv_timeout(Duration::from_secs(5))
            .map(|p| HWND(p as *mut c_void))
            .unwrap_or(HWND(std::ptr::null_mut()));
        let icon_ok = hwnd.0 != std::ptr::null_mut();
        if !icon_ok {
            eprintln!("[tray] icon did not come up within 5s; tray unavailable");
            crate::diag::log("[tray] icon did not come up within 5s; tray unavailable");
        }
        Some(Self { rx, hwnd, icon_ok })
    }

    /// True only if the shell confirmed the icon was added. The app uses
    /// this as the fail-safe gate before hiding the main window.
    pub fn icon_ok(&self) -> bool {
        self.icon_ok
    }

    pub fn try_recv(&self) -> Option<TrayCmd> {
        self.rx.try_recv().ok()
    }

    /// Show a balloon/toast above the tray icon (Windows notification).
    pub fn notify(&self, title: &str, text: &str) {
        if !self.icon_ok {
            return;
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_INFO,
            szInfo: to_wchar::<256>(text),
            szInfoTitle: to_wchar::<64>(title),
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &mut nid);
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if self.icon_ok {
            unsafe {
                let mut nid = NOTIFYICONDATAW {
                    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: self.hwnd,
                    uID: 1,
                    ..Default::default()
                };
                let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
                let _ = PostMessageW(self.hwnd, WM_QUIT, None, None);
            }
        }
    }
}

unsafe extern "system" fn tray_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == TRAY_MSG {
        let tx = {
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if p == 0 {
                return LRESULT(0);
            }
            &*(p as *const mpsc::Sender<TrayCmd>)
        };
        match lparam.0 as u32 {
            // Left click on the icon, or a click on our toast notification.
            WM_LBUTTONUP | NIN_BALLOONUSERCLICK => {
                let _ = tx.send(TrayCmd::Show);
            }
            WM_RBUTTONUP => {
                let menu = match CreatePopupMenu() {
                    Ok(m) => m,
                    Err(_) => return LRESULT(0),
                };
                // Built per right-click, so the menu follows the UI language.
                let show = windows::core::HSTRING::from(t_raw!("tray.show"));
                let play = windows::core::HSTRING::from(t_raw!("tray.play_pause"));
                let next = windows::core::HSTRING::from(t_raw!("tray.next"));
                let prev = windows::core::HSTRING::from(t_raw!("tray.previous"));
                let quit = windows::core::HSTRING::from(t_raw!("tray.quit"));
                let _ = AppendMenuW(menu, MF_STRING, MENU_SHOW, PCWSTR(show.as_ptr()));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, MENU_PLAY_PAUSE, PCWSTR(play.as_ptr()));
                let _ = AppendMenuW(menu, MF_STRING, MENU_NEXT, PCWSTR(next.as_ptr()));
                let _ = AppendMenuW(menu, MF_STRING, MENU_PREVIOUS, PCWSTR(prev.as_ptr()));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, PCWSTR(quit.as_ptr()));
                let _ = SetMenuDefaultItem(menu, MENU_SHOW as u32, 0);

                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = SetForegroundWindow(hwnd);
                let cmd = TrackPopupMenu(
                    menu,
                    TPM_LEFTALIGN | TPM_RETURNCMD,
                    pt.x,
                    pt.y,
                    0,
                    hwnd,
                    None,
                );
                let _ = PostMessageW(hwnd, WM_NULL, None, None);
                let _ = DestroyMenu(menu);

                let tray_cmd = match cmd.0 as usize {
                    MENU_SHOW => Some(TrayCmd::Show),
                    MENU_PLAY_PAUSE => Some(TrayCmd::PlayPause),
                    MENU_NEXT => Some(TrayCmd::Next),
                    MENU_PREVIOUS => Some(TrayCmd::Previous),
                    MENU_QUIT => Some(TrayCmd::Quit),
                    _ => None,
                };
                if let Some(c) = tray_cmd {
                    let _ = tx.send(c);
                }
            }
            _ => {}
        }
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn load_tray_icon() -> Option<HICON> {
    let png = include_bytes!("../../../assets/bigwave-icon-64.png");
    let img = image::load_from_memory(png).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    let mut px = img.into_raw();
    // RGBA -> BGRA (Windows 32-bit DIB byte order).
    for i in (0..px.len()).step_by(4) {
        px.swap(i, i + 2);
    }
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let hdc = GetDC(None);
        let mut bits: *mut c_void = std::ptr::null_mut();
        let hbm_color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok();
        ReleaseDC(None, hdc);
        let hbm_color = hbm_color?;
        std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u8, px.len());
        let hbm_mask = CreateBitmap(w as i32, h as i32, 1, 1, None);
        if hbm_mask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(hbm_color.0));
            return None;
        }

        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
        };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(HGDIOBJ(hbm_color.0));
        let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
        icon
    }
}

/// Runs the tray window's message loop. `started_tx` receives the window
/// handle once the icon is installed, or 0 on any setup failure.
fn tray_thread(tx: mpsc::Sender<TrayCmd>, started_tx: mpsc::Sender<isize>) {
    unsafe {
        macro_rules! fail {
            ($what:expr) => {{
                let why = $what;
                eprintln!("[tray] failed: {}", why);
                crate::diag::log(&format!("[tray] failed: {why}"));
                let _ = started_tx.send(0);
                return;
            }};
        }

        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(_) => fail!("GetModuleHandleW"),
        };

        let class_name = w!("BigWaveRadioTrayWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_wndproc),
            hInstance: windows::Win32::Foundation::HINSTANCE(hinstance.0),
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            fail!("RegisterClassW");
        }

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("BigWave Radio tray"),
            WINDOW_STYLE(0),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None, // no parent: a 0-style window is already hidden
            None,
            hinstance,
            None,
        ) {
            Ok(h) => h,
            Err(_) => fail!("CreateWindowExW"),
        };

        // Hand the sender to the window proc.
        let boxed: Box<mpsc::Sender<TrayCmd>> = Box::new(tx);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(boxed) as isize);

        let icon = match load_tray_icon() {
            Some(i) => i,
            None => fail!("load_tray_icon"),
        };

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: TRAY_MSG,
            hIcon: icon,
            szTip: to_wchar::<128>("BigWave Radio"),
            ..Default::default()
        };
        if !Shell_NotifyIconW(NIM_ADD, &mut nid).as_bool() {
            fail!("NIM_ADD");
        }
        eprintln!("[tray] icon added");
        crate::diag::log("[tray] icon added");
        let _ = started_tx.send(hwnd.0 as isize);

        let mut msg = MSG::default();
        loop {
            // Block until a message arrives; exit only on WM_QUIT (posted by
            // Tray::drop) or WM_QUIT from the system.
            let got = windows::Win32::UI::WindowsAndMessaging::GetMessageW(
                &mut msg,
                None,
                0,
                0,
            );
            if !got.as_bool() || msg.message == WM_QUIT {
                break;
            }
            DispatchMessageW(&msg);
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
    }
}
