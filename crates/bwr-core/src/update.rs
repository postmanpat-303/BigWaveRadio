//! Update checker: fetch the latest GitHub release, compare it with the
//! running version, and remember when we last checked (`update.json`, next to
//! `discovery.json`; never `settings.json`, which the old C# app shares).
//!
//! Everything here is UI-free and blocking; the app runs [`fetch_latest`] on a
//! background thread.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// GitHub "latest release" endpoint for BigWave Radio.
pub const LATEST_URL: &str =
    "https://api.github.com/repos/postmanpat-303/BigWaveRadio/releases/latest";
/// Releases page, used when a release has no setup asset.
pub const RELEASES_PAGE: &str = "https://github.com/postmanpat-303/BigWaveRadio/releases/latest";
/// A startup check runs at most this often.
pub const STARTUP_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// What we need from a GitHub release.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Tag as published, e.g. `v0.9.5`.
    pub tag: String,
    /// Tag without the leading `v`, e.g. `0.9.5`.
    pub version: String,
    pub name: String,
    /// Release notes (GitHub markdown).
    pub notes: String,
    /// The release page.
    pub html_url: String,
    /// Direct download for this platform, if the release has one: the setup
    /// exe on Windows, the universal .dmg on macOS.
    pub asset_url: Option<String>,
}

impl Release {
    /// Where "Download" goes: this platform's asset, else the release page.
    pub fn download_url(&self) -> &str {
        self.asset_url.as_deref().unwrap_or(&self.html_url)
    }
}

/// How this Linux build was installed (decides which asset "Download" fetches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxPackage {
    AppImage,
    Deb,
    Rpm,
    /// Arch / CachyOS / Manjaro (`.pkg.tar.zst`)
    Arch,
    /// Unpacked tar.gz (or anything else)
    TarGz,
}

/// Which release asset "Download" should fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// `BigWaveRadio-<ver>-setup-x64.exe`
    Windows,
    /// `BigWaveRadio-<ver>-macos-universal.dmg` (or the .app zip)
    MacOs,
    /// `BigWaveRadio-<ver>-linux-<arch>.{AppImage,deb,rpm,pkg.tar.zst,tar.gz}`
    Linux { package: LinuxPackage, arch: &'static str },
    /// No packaged build: the release page.
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "linux") {
            let exe = std::env::current_exe().ok();
            let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
            Platform::Linux {
                package: linux_package(
                    std::env::var_os("APPIMAGE").is_some(),
                    exe.as_deref(),
                    &os_release,
                ),
                arch: std::env::consts::ARCH,
            }
        } else {
            Platform::Other
        }
    }

    /// Asset names in order of preference (lower-case `ends_with` patterns).
    fn asset_suffixes(self) -> Vec<String> {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        match self {
            Platform::Windows => v(&["-setup-x64.exe", "setup.exe"]),
            Platform::MacOs => v(&["-macos-universal.dmg", ".dmg", "-macos-universal.zip"]),
            Platform::Linux { package, arch } => {
                let ext = match package {
                    LinuxPackage::AppImage => "appimage",
                    LinuxPackage::Deb => "deb",
                    LinuxPackage::Rpm => "rpm",
                    LinuxPackage::Arch => "pkg.tar.zst",
                    LinuxPackage::TarGz => "tar.gz",
                };
                let mut out = vec![format!("-linux-{arch}.{ext}")];
                if package != LinuxPackage::TarGz {
                    // no package of that kind for this arch: the portable tarball
                    out.push(format!("-linux-{arch}.tar.gz"));
                }
                out
            }
            Platform::Other => Vec::new(),
        }
    }
}

/// Work out how a Linux build was installed. Pure, for tests:
/// `appimage` = the `APPIMAGE` variable is set (the AppImage runtime sets it);
/// `exe` = this program's path; `os_release` = `/etc/os-release`.
/// A binary under `/usr` came from the distro's package format; anything else
/// is an unpacked tarball.
pub fn linux_package(appimage: bool, exe: Option<&std::path::Path>, os_release: &str) -> LinuxPackage {
    if appimage {
        return LinuxPackage::AppImage;
    }
    if !exe.is_some_and(|p| p.starts_with("/usr")) {
        return LinuxPackage::TarGz;
    }
    let mut ids = String::new();
    for line in os_release.lines() {
        if let Some(v) = line.strip_prefix("ID=").or_else(|| line.strip_prefix("ID_LIKE=")) {
            ids.push(' ');
            ids.push_str(&v.trim_matches('"').to_ascii_lowercase());
        }
    }
    let has = |w: &str| ids.split_whitespace().any(|id| id == w);
    if has("arch") || has("cachyos") || has("manjaro") || has("endeavouros") {
        LinuxPackage::Arch
    } else if has("debian") || has("ubuntu") {
        LinuxPackage::Deb
    } else if has("fedora") || has("rhel") || has("centos") || has("suse") || has("opensuse") {
        LinuxPackage::Rpm
    } else {
        LinuxPackage::TarGz
    }
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// Strip a leading `v`/`V` from a tag.
pub fn strip_v(tag: &str) -> &str {
    let t = tag.trim();
    t.strip_prefix('v')
        .or_else(|| t.strip_prefix('V'))
        .unwrap_or(t)
}

/// Parse a version leniently into semver: `v0.9` → 0.9.0, `0.9.5` → 0.9.5,
/// `1.0.0-beta.1` stays a pre-release. Only major.minor.patch counts; a
/// fourth number (`0.2.3.1`) is ignored.
pub fn parse_version(s: &str) -> Option<semver::Version> {
    let s = strip_v(s);
    if let Ok(v) = semver::Version::parse(s) {
        return Some(v);
    }
    let (core, pre) = match s.find('-') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let core = core.split('+').next().unwrap_or(core);
    let mut nums = core.split('.').map(|p| p.parse::<u64>());
    let major = nums.next()?.ok()?;
    let minor = nums.next().unwrap_or(Ok(0)).ok()?;
    let patch = nums.next().unwrap_or(Ok(0)).ok()?;
    let mut v = semver::Version::new(major, minor, patch);
    if let Some(p) = pre {
        v.pre = semver::Prerelease::new(p).ok()?;
    }
    Some(v)
}

/// True when `latest` (a tag like `v0.9.6`) is newer than `current`
/// (normally `CARGO_PKG_VERSION`). Unparseable versions never count as newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Parse the JSON of `GET /repos/{owner}/{repo}/releases/latest` for the
/// platform this build runs on.
pub fn parse_release(json: &str) -> Result<Release, String> {
    parse_release_for(json, Platform::current())
}

/// [`parse_release`] for an explicit platform (tests use this).
pub fn parse_release_for(json: &str, platform: Platform) -> Result<Release, String> {
    let gh: GhRelease =
        serde_json::from_str(json).map_err(|e| format!("unexpected reply from GitHub ({e})"))?;
    if gh.draft || gh.prerelease {
        return Err("the latest release is not a final release".into());
    }
    let version = strip_v(&gh.tag_name).to_string();
    if parse_version(&version).is_none() {
        return Err(format!("release tag '{}' is not a version", gh.tag_name));
    }
    let asset_url = platform
        .asset_suffixes()
        .iter()
        .find_map(|suffix| {
            gh.assets
                .iter()
                .find(|a| a.name.to_ascii_lowercase().ends_with(suffix.as_str()))
        })
        .map(|a| a.browser_download_url.clone());
    Ok(Release {
        name: gh
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| gh.tag_name.clone()),
        tag: gh.tag_name,
        version,
        notes: gh.body.unwrap_or_default(),
        html_url: gh.html_url.unwrap_or_else(|| RELEASES_PAGE.to_string()),
        asset_url,
    })
}

/// `BigWaveRadio/<ver> (+repo url)`. GitHub rejects API calls without a User-Agent.
pub fn user_agent(current: &str) -> String {
    format!("BigWaveRadio/{current} (+https://github.com/postmanpat-303/BigWaveRadio)")
}

/// Blocking fetch of the latest release from `url` (normally [`LATEST_URL`]).
/// Errors are short, human-readable reasons for the "Couldn't check" message.
pub fn fetch_latest(url: &str, current: &str, timeout: Duration) -> Result<Release, String> {
    let http = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout)
        .user_agent(user_agent(current))
        .dns_resolver(std::sync::Arc::new(crate::Ipv4OnlyResolver))
        .build()
        .map_err(|e| format!("couldn't start the HTTP client ({e})"))?;
    let resp = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                "GitHub didn't answer in time (no internet connection?)".to_string()
            } else if e.is_connect() {
                "couldn't reach GitHub (no internet connection?)".to_string()
            } else {
                format!("network error ({e})")
            }
        })?;
    let status = resp.status();
    if !status.is_success() {
        let why = match status.as_u16() {
            403 | 429 => " (GitHub rate limit; try again later)",
            404 => " (no published release found)",
            _ => "",
        };
        return Err(format!("GitHub returned HTTP {}{why}", status.as_u16()));
    }
    let text = resp
        .text()
        .map_err(|e| format!("couldn't read GitHub's reply ({e})"))?;
    parse_release(&text)
}

/// Make GitHub markdown readable as plain text: drop `**`/backticks, turn
/// `- ` bullets into `•`, headings into plain lines.
pub fn notes_to_plain(md: &str) -> String {
    let mut out = Vec::new();
    for line in md.replace("\r\n", "\n").lines() {
        let t = line.trim_end();
        if t.trim_start().starts_with("```") {
            continue;
        }
        let t = if t.trim_start().starts_with('#') {
            t.trim_start().trim_start_matches('#').trim_start()
        } else {
            t
        };
        let t = t.replace("**", "").replace('`', "");
        let lead = t.len() - t.trim_start().len();
        let rest = t.trim_start();
        let t = if let Some(r) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix("* ")) {
            format!("{}• {r}", " ".repeat(lead))
        } else {
            t
        };
        out.push(t);
    }
    out.join("\n").trim().to_string()
}

// ---------------------------------------------------------------------------
// Persistent state (update.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    /// Settings toggle "Check for updates at startup".
    pub check_at_startup: bool,
    /// Unix seconds of the last successful check (manual or startup).
    pub last_check: u64,
    /// "Skip this version": no startup banner for this version.
    pub skipped_version: Option<String>,
    #[serde(skip)]
    path: PathBuf,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            check_at_startup: true,
            last_check: 0,
            skipped_version: None,
            path: PathBuf::new(),
        }
    }
}

impl UpdateState {
    pub fn load() -> Self {
        Self::load_from(crate::discovery::data_dir().join("update.json"))
    }

    pub fn load_from(path: PathBuf) -> Self {
        let mut s: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.path = path;
        s
    }

    pub fn save(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    /// Should the quiet startup check run now? (`now` = unix seconds.)
    pub fn startup_due(&self, now: u64) -> bool {
        self.check_at_startup
            && (self.last_check == 0
                || now < self.last_check // clock went backwards
                || now - self.last_check >= STARTUP_INTERVAL_SECS)
    }

    /// Should a startup check that found `latest` show the banner?
    pub fn wants_banner(&self, latest: &Release, current: &str) -> bool {
        is_newer(&latest.tag, current)
            && self.skipped_version.as_deref() != Some(latest.version.as_str())
    }
}

/// Current unix time in seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
