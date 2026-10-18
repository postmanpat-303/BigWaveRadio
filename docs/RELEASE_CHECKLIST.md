# Release checklist

Work through this before publishing a release.

- [ ] Bump `[workspace.package] version` in the root `Cargo.toml` (all crates share it). It must equal the
      release tag without the `v`: the About page, the exe resource and the update checker all read it
- [ ] `cargo test --workspace` passes
- [ ] Windows: the **Windows** workflow (`gh workflow run windows.yml -f version=<ver>`, also runs on the tag push)
      builds with MSVC and checks DLLs, manifest, icon, version info, `--check-plugin` and a silent install/uninstall.
      It only produces an artifact: download it (`gh run download <run id>`) and upload the zip + setup exe by hand
- [ ] Open the new Windows build once on a real PC (CI runners have no OpenGL 2.0, so the window can't be tested there)
- [ ] PKGBUILD `pkgver` in `packaging/arch/*/PKGBUILD` matches (the Linux workflow fails otherwise)
- [ ] Release notes written (the in-app update dialog shows them, so keep them short and readable);
      `gh release create vX --target <sha> --latest=false` (mark it Latest once every asset is attached)
- [ ] Creating the release pushes the tag, which runs the **macOS** and **Linux** workflows; they attach
      `-macos-universal.dmg/.zip` and `-linux-{x86_64,aarch64}.{tar.gz,deb,rpm,AppImage}` + `-linux-x86_64.pkg.tar.zst`
      and `bwr-plugin-check-linux-x64`. Re-run with
      `gh workflow run macos.yml -f release_tag=vX` / `gh workflow run linux.yml -f release_tag=vX`
- [ ] Both workflows green (each package is installed in Ubuntu / Fedora / Arch and runs `--check-plugin`)
- [ ] `SHA256SUMS.txt` covers every asset (`sha256sum * > SHA256SUMS.txt` over all downloaded assets), then
      `gh release edit vX --latest`

See [PACKAGING.md](PACKAGING.md).

## Done

- [x] Wolf icon everywhere: exe resource, window/taskbar, tray, shortcuts, Apps & features, setup/uninstaller
- [x] Installer wizard bitmaps and the About-page text logo redone in the wolf palette (0.9.4)
- [x] Inno Setup replaced by NSIS, same as ChipWave Tracker (0.9.4); Wine no longer needed to build
