# BigWave Radio (Rust)

A native Windows 11 internet radio player, written in Rust. Streams stations from
the community-powered [radio-browser.info](https://www.radio-browser.info) directory.

Created by Derek Heasley. Inspired by ShortWave on Linux — I wanted a nice clean
version for Windows. If you find this app useful, you can donate to
https://paypal.me/derekh100. Thank you. :)

This is a ground-up Rust rewrite of the original C#/WinUI 3 BigWave Radio app,
undertaken to shrink the download from an ~84 MB self-contained installer to a
single ~8 MB executable.

## Features

- **Discover page** — trending, popular and recently-added stations
- **Live search** — top-bar search bar, results update as you type
- **Search page** — tag/genre exploration
- **My Library** — star any station for quick reference (stored in
  `%LOCALAPPDATA%\BigWaveRadio\library.json`)
- **Tags & Countries** browsing
- **Bitrate filter** — show only streams at or above a chosen bitrate
- **Now-playing transport** — previous / play / pause / stop / next
- **32-band spectrum analyser** animated inside the playing station card
- **Themes** — dark / light / system, plus 8 accent highlight colours
- **Settings compatible with the C# app** — reads and writes the same
  `%LOCALAPPDATA%\BigWaveRadio\settings.json`, so your existing library and
  theme preferences carry over

## Architecture

| Crate | Responsibility |
| --- | --- |
| `bwr-core` | radio-browser.info API client (IPv4-only DNS resolver, mirror failover, Schannel TLS via `native-tls`), station/tag/country models, library + settings persistence |
| `bwr-player` | stream download, decode (symphonia), audio output (cpal/WASAPI) on a worker thread |
| `bwr-spectrum` | FFT band analysis (`rustfft`) fed by the decoded PCM |
| `bwr-app` | egui user interface, worker-thread result pump |

## Building

Requires the `x86_64-pc-windows-gnullvm` host toolchain (a self-contained
MinGW-flavoured Rust toolchain that links with the bundled rust-lld, so no
external MSVC or MinGW linker is needed) plus [llvm-mingw](https://github.com/mstorsjo/llvm-mingw)
on `PATH` for the C dependencies and the icon resource:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnullvm
rustup override set stable-x86_64-pc-windows-gnullvm   # in this directory
$env:PATH = "C:\path\to\llvm-mingw-20261006-ucrt-x86_64\bin;$env:PATH"
cargo build --release
```

The executable is `target\release\bwr-app.exe` (~8 MB). The toolchain ships
`libunwind.dll`, which the exe links against — copy that DLL next to the exe
when distributing, or run with the toolchain's `bin` directory on `PATH`.

## Roadmap (vs. the C# app)

- Glitch-effect word-cloud search page
- Mica / acrylic backdrop
- Right-side station information panel
- Sleep timer
- Signed installer

## License

MIT
