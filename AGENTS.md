# AGENTS.md

Notes for AI coding agents working in this repository.

## Writing a BigWave Radio plugin

Read **[docs/PLUGIN_GUIDE.md](docs/PLUGIN_GUIDE.md)** first; it is the complete
plugin API (v1) and starts with a one-shot prompt. In short:

* A plugin is a folder `plugins/<id>/` with `plugin.toml` and `main.rhai`
  (Rhai script, not Rust). Installed copies live in
  `%LOCALAPPDATA%\BigWaveRadio\plugins\<id>\`.
* Declare permissions in `plugin.toml`; there is no network or file access.
* Draw by returning commands (`draw.fill/rect/circle/line/text/svg/rings`),
  never egui. `draw.rings` is the efficient way to do concentric-ring /
  moiré effects (one command per disc). For visualisers use
  `draw.visual(#{ scene, palette, zoom, rotate, decay, bands, ... })`: a
  native MilkDrop-style feedback renderer (scenes tunnel, kaleido, plasma,
  flow, sphere, terrain, stars, mandala) that is fast and non-strobing.
  `crates/bwr-plugins/src/visual.rs` holds the engine; see `plugins/visions/`.
* Validate before handing over:
  `cargo run -p bwr-app -- --check-plugin path/to/plugin` (or
  `BigWaveRadio.exe --check-plugin <folder>`). It must print `OK`.
* App-wide shortcuts: declare `[[hotkeys]] keys = "Ctrl+F,U" action = "start"`
  (needs the `hotkeys` permission) and handle `on_hotkey(ctx, action)`. The
  first step must use Ctrl or Alt; Esc is reserved for the host.
* Flashing content must respect `ctx.flash_cap`; the host enforces it anyway.

Working examples: `plugins/strobe/` (bundled) and `docs/plugin-examples/*`.

## Working on the app itself

* Workspace crates: `bwr-core` (API client, settings), `bwr-player`
  (stream/decode/output), `bwr-spectrum` (FFT bands, beat/break detection),
  `bwr-plugins` (plugin host: manifest, Rhai engine, permissions, hot reload,
  safety limiter, zip import, `check_plugin`), `bwr-app` (egui UI).
* Keep `crates/bwr-app/src/app.rs` changes small; plugin glue belongs in
  `crates/bwr-app/src/plugin_bridge.rs` and `pages/plugins.rs`, logic in
  `bwr-plugins`.
* Adding a script API: register it in `crates/bwr-plugins/src/api.rs`, gate it
  with a permission, document it in `docs/PLUGIN_GUIDE.md`, add a test in
  `crates/bwr-plugins/tests/loader.rs`. Breaking changes need `API_VERSION` + 1.
* `cargo test` must pass (it also checks every example plugin and that the
  guide inlines the example files verbatim).
* Target is Windows (`x86_64-pc-windows-gnullvm`); the crates also build and
  test on Linux, which is how CI-less agents can verify changes.
