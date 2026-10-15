//! Notification-area (tray) icon with a right-click menu.
//!
//! Lives on its own hidden window. The UI thread pings it every frame; if the
//! pings stop (app exited or crashed) the thread removes the icon, so no
//! ghost icon is ever left behind in the tray.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetWindowLongPtrW, PeekMessageW, PostMessageW, RegisterClassW,
    SetForegroundWindow, SetMenuDefaultItem, SetWindowLongPtrW, TrackPopupMenu, CW_USEDEFAULT,
    GWLP_USERDATA, HICON, ICONINFO, MF_SEPARATOR, MF_STRING, MSG, PM_REMOVE, TPM_LEFTALIGN,
    TPM_RETURNCMD, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROY, WM_LBUTTONUP, WM_NULL,
    WM_QUIT, WM_RBUTTONUP, WNDCLASSW,
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
const MENU_SHOW: usize = 1001;
const MENU_PLAY_PAUSE: usize = 1002;
const MENU_NEXT: usize = 1003;
const MENU_PREVIOUS: usize = 1004;
const MENU_QUIT: usize = 1005;

pub struct Tray {
    rx: mpsc::Receiver<TrayCmd>,
    last_ping: Arc<Mutex<Instant>>,
}

impl Tray {
    /// Install the tray icon. Returns None if the icon could not be created.
    pub fn new() -> Option<Self> {
        let (tx, rx) = mpsc::channel::<TrayCmd>();
        let (started_tx, started_rx) = mpsc::channel::<bool>();
        let last_ping = Arc::new(Mutex::new(Instant::now()));

        std::thread::spawn({
            let last_ping = last_ping.clone();
            move || tray_thread(tx, started_tx, last_ping)
        });

        // Wait (briefly) only for the icon to come up — never for the thread
        // itself, which runs its message loop for the lifetime of the app.
        let started = started_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(false);

        if started {
            Some(Self { rx, last_ping })
        } else {
            None
        }
    }

    /// Call once per UI frame so the tray thread knows the app is alive.
    pub fn ping(&self) {
        if let Ok(mut t) = self.last_ping.lock() {
            *t = Instant::now();
        }
    }

    pub fn try_recv(&self) -> Option<TrayCmd> {
        self.rx.try_recv().ok()
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
            WM_LBUTTONUP => {
                let _ = tx.send(TrayCmd::Show);
            }
            WM_RBUTTONUP => {
                let menu = match CreatePopupMenu() {
                    Ok(m) => m,
                    Err(_) => return LRESULT(0),
                };
                let _ = AppendMenuW(menu, MF_STRING, MENU_SHOW, w!("Show BigWave Radio"));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, MENU_PLAY_PAUSE, w!("Play / Pause"));
                let _ = AppendMenuW(menu, MF_STRING, MENU_NEXT, w!("Next station"));
                let _ = AppendMenuW(menu, MF_STRING, MENU_PREVIOUS, w!("Previous station"));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit"));
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
    let png = include_bytes!("../../../assets/bigwave-logo.png");
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

/// Runs the tray window's message loop. `started_tx` receives true once the
/// icon is installed, false on any setup failure.
fn tray_thread(
    tx: mpsc::Sender<TrayCmd>,
    started_tx: mpsc::Sender<bool>,
    last_ping: Arc<Mutex<Instant>>,
) {
    unsafe {
        macro_rules! fail {
            ($what:expr) => {{
                eprintln!("[tray] failed: {}", $what);
                let _ = started_tx.send(false);
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
            Err(_) => fail!("GetModuleHandleW"),
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
            szTip: {
                let mut tip = [0u16; 128];
                let s: Vec<u16> = "BigWave Radio".encode_utf16().collect();
                let n = s.len().min(tip.len() - 1);
                tip[..n].copy_from_slice(&s[..n]);
                tip
            },
            ..Default::default()
        };
        if !Shell_NotifyIconW(NIM_ADD, &mut nid).as_bool() {
            fail!("NIM_ADD");
        }
        eprintln!("[tray] icon added");
        let _ = started_tx.send(true);

        let mut msg = MSG::default();
        'outer: loop {
            // Heartbeat: app gone -> remove icon and leave.
            let alive = last_ping
                .lock()
                .map(|t| t.elapsed() < Duration::from_secs(4))
                .unwrap_or(false);
            if !alive {
                break 'outer;
            }

            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'outer;
                }
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(150));
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
    }
}
