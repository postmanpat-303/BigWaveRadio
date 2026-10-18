# Translating BigWave Radio

BigWave Radio's interface is available in:

| Language | Code | Catalog |
|---|---|---|
| English (source and fallback) | `en` | [`crates/bwr-app/locales/en.toml`](../crates/bwr-app/locales/en.toml) |
| Português (Brasil) | `pt-BR` | [`crates/bwr-app/locales/pt-BR.toml`](../crates/bwr-app/locales/pt-BR.toml) |
| Italiano | `it` | [`crates/bwr-app/locales/it.toml`](../crates/bwr-app/locales/it.toml) |

Users pick a language under **Settings → Language**. The default, **System**,
follows the computer's language (`pt-*` → Português (Brasil), `it-*` →
Italiano, anything else → English). The choice is saved in `ui.json` in the
app's data folder (never in `settings.json`, which stays compatible with the
old C# app), and switching applies immediately, without a restart.

Improvements to an existing translation are just as welcome as new
languages: a native speaker's eye is the best test.

## How the catalogs work

Each catalog is a small TOML file of `key = "text"` lines grouped into
sections:

```toml
[settings]
title = "Settings"
language = "Language"

[settings.minutes]       # plural: .one and .other
one = "{n} minute"
other = "{n} minutes"
```

* **Keep the keys** (left side) exactly as in `en.toml`; translate only the
  text in quotes.
* **Keep `{placeholders}`** such as `{name}`, `{count}`, `{version}` or `{n}`.
  The app fills them in, and you can move them anywhere in the sentence.
* **Plurals** have `.one` (one item) and `.other` (any other number).
  Portuguese uses `.one` for 0 and 1, English and Italian only for 1. A new
  language with different rules needs a line in `Lang::is_one` in
  `crates/bwr-app/src/i18n.rs`.
* **Don't translate** "BigWave Radio", station names, plugin names, tag names
  (`#trance`), file names (`moods.json`, `PLUGIN_GUIDE.md`) or keyboard keys
  (Esc, Ctrl+Alt+B). Translate "Enter" only if your keyboards label it
  differently (Italian: "Invio").
* **Keep it short** where the English is short: transport captions (`player.*`)
  sit under small buttons and mood names (`mood.*`) on narrow chips.
* `format.thousands_sep` is the digit-group separator for big numbers
  (`,` in English, `.` in Portuguese and Italian).
* A key missing from a translation simply shows the English text, so a
  partial translation never breaks the app; the tests below make sure
  nothing is left out before a release.

## Improving an existing language

1. Edit `crates/bwr-app/locales/<code>.toml`.
2. Run `cargo test -p bwr-app` (see *Checking your work*).
3. Open a pull request. Screenshots are a bonus.

No Rust needed: you can also edit the file straight on GitHub (pencil icon)
and open the pull request from there; CI runs the checks.

## Adding a new language

1. Copy `crates/bwr-app/locales/en.toml` to `crates/bwr-app/locales/<code>.toml`
   (a BCP 47 code: `de`, `es`, `fr`, `pt-PT`, ...) and translate every text.
2. Register it in `crates/bwr-app/src/i18n.rs`:
   * add a variant to `enum Lang` and to `Lang::ALL`;
   * add its `code()`, its `native_name()` (the language's name in that
     language, e.g. "Deutsch"), its catalog in `source()`
     (`include_str!("../locales/<code>.toml")`) and its plural rule in
     `is_one()`;
   * map the OS locale to it in `from_locale()` (e.g. `"de" => Lang::De`);
   * update `index()`/`current()` if needed (they follow the enum order) and
     the catalog array size in `catalogs()`.
3. Add the language to the loop in `translations_cover_english_exactly` in
   `crates/bwr-app/src/i18n/tests.rs`.
4. Optional: translate the bundled plugins' texts (`plugins/strobe` and
   `plugins/visions`: `[descriptions]`, `titles`, `labels` in `plugin.toml`
   and `texts()` in `main.rhai`).
5. Add the language to the table at the top of this file and to the README.

If you would rather not touch Rust, just send the translated `.toml` file in
an issue or pull request and we'll wire it up.

## Checking your work

```sh
cargo test -p bwr-app i18n -- --nocapture
```

This fails if any language is missing a key from English, has a key English
doesn't, changes a `{placeholder}`, leaves a text empty, or if `en.toml`
contains a key the code no longer uses. It also prints each language's
coverage, e.g. `it: 237/237 keys (100.0%)`.

To see the app in a language without changing your system settings, set
`BWR_LANG` for a run while Settings → Language is on **System**:

```sh
BWR_LANG=it cargo run -p bwr-app
```

## For developers: adding UI text

Never hard-code user-facing text in the app. Add a key to `en.toml` (and
the other catalogs) and use the macros from `crates/bwr-app/src/i18n.rs`:

```rust
ui.heading(t!("settings.title"));                        // &'static str
ui.label(t!("update.banner", version = rel.version));    // fills {version}
ui.label(tn!("moods.talk_hidden", hidden));              // plural, fills {n}
```

If you can't translate a new text yourself, put the English text in every
catalog and mention it in the pull request so a translator can pick it up.

Plugins get the UI language as `ctx.lang`; see *Translations* in
[PLUGIN_GUIDE.md](PLUGIN_GUIDE.md).
