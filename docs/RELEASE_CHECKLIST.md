# Release checklist

Work through this before publishing a release.

- [ ] Bump `MyAppVersion` and `OutputBaseFilename` in `installer/bigwave.iss`
- [ ] `cargo test --workspace` passes
- [ ] Windows build + installer + zip built from the release commit (no build files committed)
- [ ] Smoke-test the installer: shortcuts, desktop icon, uninstall
- [ ] Release notes written; `gh release create vX --target <sha> --latest` with the installer, zip and `bwr-plugin-check-linux-x64`

## Pending

- [ ] Update setup/installer icon (SetupIconFile) to new bwc icon
  - Note: `installer/bigwave.ico` (the SetupIconFile) was already swapped to the bwc icon in fdf69d6.
    Confirm that is wanted, and decide whether `wizard_image.bmp` / `wizard_small.bmp` should change too.
    App shortcuts no longer depend on this file; they use `assets/bigwave.ico`.
