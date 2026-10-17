# Release checklist

Work through this before publishing a release.

- [ ] Bump `MyAppVersion` and `OutputBaseFilename` in `installer/bigwave.iss`
- [ ] `cargo test --workspace` passes
- [ ] Windows build + installer + zip built from the release commit (no build files committed)
- [ ] Smoke-test the installer: shortcuts, desktop icon, uninstall
- [ ] Release notes written; `gh release create vX --target <sha> --latest` with the installer, zip and `bwr-plugin-check-linux-x64`

## Pending

- [x] Update setup/installer icon (SetupIconFile) to new bwc icon
  - Superseded 2026-10-17: the official icon is now the **engraved howling wolf on green**
    (`/workspace/loud/pack/BigWave_Radio`). `installer/bigwave.ico` (SetupIconFile), `assets/bigwave.ico`
    (exe + shortcuts), `assets/bigwave-icon.png` (window/taskbar, About icon) and
    `assets/bigwave-icon-64.png` (tray) all carry the wolf icon.
- [ ] Decide whether the installer wizard images (`installer/wizard_image.bmp`, `installer/wizard_small.bmp`)
      and the About-page text logo (`assets/bigwave-logo.png`, still the wave + "BigWave Radio" artwork)
      should be redone to match the wolf icon
