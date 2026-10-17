//! Update checker: version comparison, release JSON parsing (fixture), the
//! HTTP fetch against a local server (success + failure), and update.json state.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use bwr_core::update::{
    fetch_latest, is_newer, notes_to_plain, parse_release, parse_release_for, parse_version, strip_v,
    linux_package, LinuxPackage, Platform, UpdateState, STARTUP_INTERVAL_SECS,
};

const FIXTURE: &str = include_str!("fixtures/release_latest.json");

#[test]
fn version_comparison() {
    assert_eq!(strip_v("v0.9.5"), "0.9.5");
    assert_eq!(strip_v("0.9.5"), "0.9.5");
    assert!(is_newer("v0.9.6", "0.9.5"));
    assert!(
        is_newer("v0.10.0", "0.9.5"),
        "numeric, not string, comparison"
    );
    assert!(is_newer("v1.0.0", "0.9.5"));
    assert!(
        !is_newer("v0.9.5", "0.9.5"),
        "same version is not an update"
    );
    assert!(
        !is_newer("v0.9.4", "0.9.5"),
        "older release is not an update"
    );
    // Two-part tags from early releases (v0.9) parse as x.y.0.
    assert_eq!(parse_version("v0.9"), Some(semver::Version::new(0, 9, 0)));
    assert!(is_newer("v0.9.1", "0.9.0"));
    assert!(!is_newer("v0.9", "0.9.5"));
    // The 0.1.0 crate version the early builds carried is older than every release.
    assert!(is_newer("v0.9.5", "0.1.0"));
    // Pre-releases sort before the final version.
    assert!(is_newer("v1.0.0", "1.0.0-beta.1"));
    assert!(!is_newer("v1.0.0-beta.1", "1.0.0"));
    // Garbage never counts as newer.
    assert!(!is_newer("latest", "0.9.5"));
    assert!(!is_newer("v0.9.6", "not-a-version"));
}

#[test]
fn the_running_crate_version_is_a_release_version() {
    let v = parse_version(env!("CARGO_PKG_VERSION")).expect("crate version parses");
    assert!(
        v >= semver::Version::new(0, 9, 5),
        "workspace version bumped for the update checker"
    );
}

#[test]
fn parses_the_github_fixture() {
    let r = parse_release_for(FIXTURE, Platform::Windows).expect("fixture parses");
    assert_eq!(r.tag, "v0.9.6");
    assert_eq!(r.version, "0.9.6");
    assert_eq!(r.name, "BigWave Radio 0.9.6 (fixture)");
    assert_eq!(
        r.html_url,
        "https://github.com/postmanpat-303/BigWaveRadio/releases/tag/v0.9.6"
    );
    // Zips and the mac dmg are listed first; Windows must still pick the setup exe.
    assert_eq!(
        r.asset_url.as_deref(),
        Some("https://github.com/postmanpat-303/BigWaveRadio/releases/download/v0.9.6/BigWaveRadio-0.9.6-setup-x64.exe")
    );
    assert!(r
        .download_url()
        .ends_with("BigWaveRadio-0.9.6-setup-x64.exe"));
    assert!(r.notes.contains("Sleep timer"));
    assert!(is_newer(&r.tag, "0.9.5"));
}

#[test]
fn macos_downloads_the_universal_dmg() {
    let r = parse_release_for(FIXTURE, Platform::MacOs).unwrap();
    assert_eq!(
        r.download_url(),
        "https://github.com/postmanpat-303/BigWaveRadio/releases/download/v0.9.6/BigWaveRadio-0.9.6-macos-universal.dmg"
    );
    // A release with only the Windows build: the mac app goes to the release page.
    let win_only = r#"{"tag_name":"v0.9.5","html_url":"https://example.invalid/r","assets":[
        {"name":"BigWaveRadio-0.9.5-setup-x64.exe","browser_download_url":"https://example.invalid/s.exe"}]}"#;
    let r = parse_release_for(win_only, Platform::MacOs).unwrap();
    assert_eq!(r.asset_url, None);
    assert_eq!(r.download_url(), "https://example.invalid/r");
    // Platforms without a package (Linux) always get the release page.
    let r = parse_release_for(FIXTURE, Platform::Other).unwrap();
    assert_eq!(r.download_url(), r.html_url);
}

#[test]
fn linux_picks_the_asset_for_its_package_type() {
    let dl = |package, arch| {
        parse_release_for(FIXTURE, Platform::Linux { package, arch })
            .unwrap()
            .download_url()
            .rsplit('/')
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(dl(LinuxPackage::Deb, "x86_64"), "BigWaveRadio-0.9.6-linux-x86_64.deb");
    assert_eq!(dl(LinuxPackage::Rpm, "x86_64"), "BigWaveRadio-0.9.6-linux-x86_64.rpm");
    assert_eq!(dl(LinuxPackage::Arch, "x86_64"), "BigWaveRadio-0.9.6-linux-x86_64.pkg.tar.zst");
    assert_eq!(dl(LinuxPackage::AppImage, "x86_64"), "BigWaveRadio-0.9.6-linux-x86_64.AppImage");
    assert_eq!(dl(LinuxPackage::TarGz, "x86_64"), "BigWaveRadio-0.9.6-linux-x86_64.tar.gz");
    assert_eq!(dl(LinuxPackage::Deb, "aarch64"), "BigWaveRadio-0.9.6-linux-aarch64.deb");
    // No Arch package for aarch64 in the fixture: the aarch64 tarball.
    assert_eq!(dl(LinuxPackage::Arch, "aarch64"), "BigWaveRadio-0.9.6-linux-aarch64.tar.gz");
    // Nothing at all for this arch: the release page.
    let r = parse_release_for(FIXTURE, Platform::Linux { package: LinuxPackage::Deb, arch: "riscv64" }).unwrap();
    assert_eq!(r.download_url(), r.html_url);
}

#[test]
fn linux_package_detection() {
    use std::path::Path;
    let ubuntu = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n";
    let fedora = "NAME=\"Fedora Linux\"\nID=fedora\n";
    let cachy = "NAME=\"CachyOS Linux\"\nID=cachyos\nID_LIKE=arch\n";
    let usr = Some(Path::new("/usr/bin/bigwave-radio"));
    assert_eq!(linux_package(true, usr, ubuntu), LinuxPackage::AppImage);
    assert_eq!(linux_package(false, usr, ubuntu), LinuxPackage::Deb);
    assert_eq!(linux_package(false, usr, fedora), LinuxPackage::Rpm);
    assert_eq!(linux_package(false, usr, cachy), LinuxPackage::Arch);
    assert_eq!(linux_package(false, usr, "ID=gentoo\n"), LinuxPackage::TarGz);
    // Unpacked tarball in the home folder, even on Ubuntu.
    let home = Some(Path::new("/home/del/BigWaveRadio/bigwave-radio"));
    assert_eq!(linux_package(false, home, ubuntu), LinuxPackage::TarGz);
    assert_eq!(linux_package(false, None, ubuntu), LinuxPackage::TarGz);
}

#[test]
fn the_build_picks_its_own_platform() {
    let p = Platform::current();
    if cfg!(windows) {
        assert_eq!(p, Platform::Windows);
    } else if cfg!(target_os = "macos") {
        assert_eq!(p, Platform::MacOs);
    } else if cfg!(target_os = "linux") {
        assert!(matches!(p, Platform::Linux { arch, .. } if arch == std::env::consts::ARCH), "{p:?}");
    }
    assert_eq!(parse_release(FIXTURE).unwrap(), parse_release_for(FIXTURE, p).unwrap());
}

#[test]
fn release_without_setup_downloads_the_release_page() {
    let json =
        r#"{"tag_name":"v0.9.7","html_url":"https://example.invalid/r","body":null,"assets":[]}"#;
    let r = parse_release_for(json, Platform::Windows).unwrap();
    assert_eq!(r.asset_url, None);
    assert_eq!(r.download_url(), "https://example.invalid/r");
    assert_eq!(r.name, "v0.9.7");
    assert_eq!(r.notes, "");
}

#[test]
fn bad_release_json_is_an_error() {
    assert!(parse_release("not json")
        .unwrap_err()
        .contains("unexpected reply"));
    // What GitHub sends for a repo without releases.
    assert!(parse_release(r#"{"message":"Not Found","status":"404"}"#).is_err());
    assert!(parse_release(r#"{"tag_name":"nightly","assets":[]}"#)
        .unwrap_err()
        .contains("not a version"));
    assert!(parse_release(r#"{"tag_name":"v2.0.0","prerelease":true}"#).is_err());
}

#[test]
fn notes_become_readable_plain_text() {
    let r = parse_release(FIXTURE).unwrap();
    let t = notes_to_plain(&r.notes);
    assert!(
        t.contains("Download: BigWaveRadio-0.9.6-setup-x64.exe (installer)"),
        "{t}"
    );
    assert!(t.contains("• Sleep timer"), "{t}");
    assert!(
        !t.contains("**") && !t.contains('`') && !t.contains('\r'),
        "{t}"
    );
    assert_eq!(notes_to_plain("## New\n- a"), "New\n• a");
}

/// One-shot local HTTP server; returns its URL and a handle yielding the raw request.
fn serve_once(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/repos/x/y/releases/latest",
        l.local_addr().unwrap()
    );
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut buf = [0u8; 8192];
        let mut req = Vec::new();
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = s.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            req.extend_from_slice(&buf[..n]);
        }
        s.write_all(resp.as_bytes()).unwrap();
        String::from_utf8_lossy(&req).into_owned()
    });
    (url, h)
}

#[test]
fn fetch_sends_a_user_agent_and_parses_the_reply() {
    let (url, h) = serve_once("200 OK", FIXTURE);
    let r = fetch_latest(&url, "0.9.5", Duration::from_secs(10)).expect("fetch ok");
    assert_eq!(r.version, "0.9.6");
    let req = h.join().unwrap().to_ascii_lowercase();
    assert!(req.contains("user-agent: bigwaveradio/0.9.5"), "{req}");
    assert!(req.contains("accept: application/vnd.github+json"), "{req}");
}

#[test]
fn fetch_failures_give_a_reason() {
    let (url, h) = serve_once("403 Forbidden", r#"{"message":"API rate limit exceeded"}"#);
    let e = fetch_latest(&url, "0.9.5", Duration::from_secs(10)).unwrap_err();
    assert!(e.contains("HTTP 403") && e.contains("rate limit"), "{e}");
    h.join().unwrap();

    let (url, h) = serve_once("200 OK", "<html>captive portal</html>");
    let e = fetch_latest(&url, "0.9.5", Duration::from_secs(10)).unwrap_err();
    assert!(e.contains("unexpected reply"), "{e}");
    h.join().unwrap();

    // Nothing listening: connection refused, well inside the timeout.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let t = std::time::Instant::now();
    let e = fetch_latest(
        &format!("http://127.0.0.1:{port}/"),
        "0.9.5",
        Duration::from_secs(10),
    )
    .unwrap_err();
    assert!(
        e.contains("couldn't reach GitHub") || e.contains("network error"),
        "{e}"
    );
    assert!(t.elapsed() < Duration::from_secs(10));
}

#[test]
fn fetch_times_out() {
    // Accepts but never answers.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", l.local_addr().unwrap());
    let _keep = std::thread::spawn(move || {
        let (_s, _) = l.accept().unwrap();
        std::thread::sleep(Duration::from_secs(5));
    });
    let t = std::time::Instant::now();
    let e = fetch_latest(&url, "0.9.5", Duration::from_millis(600)).unwrap_err();
    assert!(e.contains("in time"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(4));
}

#[test]
fn update_state_round_trip_and_daily_startup_check() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("update.json");
    let mut s = UpdateState::load_from(path.clone());
    assert!(s.check_at_startup, "on by default");
    let now = 1_800_000_000;
    assert!(s.startup_due(now), "never checked");
    s.last_check = now;
    assert!(!s.startup_due(now + 60), "not twice a day");
    assert!(s.startup_due(now + STARTUP_INTERVAL_SECS));
    assert!(s.startup_due(now - 10), "clock went backwards");
    s.check_at_startup = false;
    assert!(
        !s.startup_due(now + 10 * STARTUP_INTERVAL_SECS),
        "toggle off"
    );
    s.skipped_version = Some("0.9.6".into());
    s.save();

    let back = UpdateState::load_from(path);
    assert!(!back.check_at_startup);
    assert_eq!(back.last_check, now);
    let r = parse_release(FIXTURE).unwrap();
    assert!(
        !back.wants_banner(&r, "0.9.5"),
        "skipped version: no banner"
    );
    let fresh = UpdateState::load_from(dir.path().join("missing.json"));
    assert!(fresh.wants_banner(&r, "0.9.5"));
    assert!(!fresh.wants_banner(&r, "0.9.6"));
}

/// Live check against GitHub (network): `cargo test -p bwr-core --test update -- --ignored`
#[test]
#[ignore]
fn live_github_latest_release() {
    let r = fetch_latest(
        bwr_core::update::LATEST_URL,
        env!("CARGO_PKG_VERSION"),
        Duration::from_secs(10),
    )
    .expect("GitHub reachable");
    assert!(parse_version(&r.tag).is_some());
    if Platform::current() == Platform::Windows {
        assert!(r.download_url().ends_with("-setup-x64.exe"), "{r:?}");
    }
    println!("latest: {} -> {}", r.tag, r.download_url());
}
