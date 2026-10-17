# Release checklist

Work through this before publishing a release.

- [ ] Bump `[workspace.package] version` in the root `Cargo.toml` (all crates share it). It must equal the
      release tag without the `v`: the About page, the exe resource and the update checker all read it
- [ ] `cargo test --workspace` passes
- [ ] `scripts/package-windows.sh <ver>` from the release commit (no build files committed; `dist/` is ignored)
- [ ] `cargo build --release -p bwr-plugins --bin bwr-plugin-check` → upload as `bwr-plugin-check-linux-x64`
- [ ] Icon check: `wrestool -x -t 14` on the setup exe and `dist/pkg/BigWaveRadio.exe`; frames match the wolf PNGs
- [ ] Smoke-test the setup (Wine is fine): install over the previous version, shortcuts, uninstall
- [ ] Release notes written (the in-app update dialog shows them, so keep them short and readable); `gh release create vX --target <sha> --latest` with
      `BigWaveRadio-<ver>-setup-x64.exe`, `BigWaveRadio-<ver>-windows-x64.zip`, `bwr-plugin-check-linux-x64`

See [PACKAGING.md](PACKAGING.md).

## Done

- [x] Wolf icon everywhere: exe resource, window/taskbar, tray, shortcuts, Apps & features, setup/uninstaller
- [x] Installer wizard bitmaps and the About-page text logo redone in the wolf palette (0.9.4)
- [x] Inno Setup replaced by NSIS, same as ChipWave Tracker (0.9.4); Wine no longer needed to build
