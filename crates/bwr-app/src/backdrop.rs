//! Windows 11 detection for the Mica-style material.
//!
//! The app paints a Mica-*style* material (a soft theme-tinted vertical
//! gradient, see `theme::paint_material`) on Windows 11. A real DWM Mica
//! backdrop was prototyped (transparent window + DWMWA_SYSTEMBACKDROP_TYPE)
//! but abandoned: per-pixel alpha compositing is not reliably available on
//! this machine's hybrid Intel/NVIDIA stack — the Vulkan drivers expose no
//! alpha surface modes at all, and the dx12 flip-model path deadlocks inside
//! the first frame. The painted material gives the same visual language with
//! zero risk. On older Windows everything stays solid as before.

#![cfg(windows)]

use std::sync::OnceLock;

/// True on Windows 11 (build 22000+).
pub fn supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| build_number().is_some_and(|b| b >= 22000))
}

/// CurrentBuildNumber under HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion.
fn build_number() -> Option<u32> {
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, REG_VALUE_TYPE,
    };
    let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value: Vec<u16> = "CurrentBuildNumber".encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = [0u8; 16];
    let mut len = buf.len() as u32;
    let mut kind = REG_VALUE_TYPE::default();
    unsafe {
        let r = RegGetValueW(
            HKEY_LOCAL_MACHINE,
            windows::core::PCWSTR(subkey.as_ptr()),
            windows::core::PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            Some(&mut kind),
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut len),
        );
        if r.is_err() || len < 2 {
            return None;
        }
        let text = String::from_utf16_lossy(
            &buf[..(len as usize)]
                .chunks_exact(2)
                .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        );
        text.trim().trim_matches('\0').parse().ok()
    }
}
