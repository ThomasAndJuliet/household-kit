# household-kit

Shared crates for the household's libcosmic apps (Terminal, Cerea, Lollypop, Discord, ScreenPush, the AI-usage applet). Before this, each app kept a copy of household-ui's reference code. Each concern is now one crate in this workspace, used as a private git dependency, as cosmic-term already uses `household-mount`.

| Crate | What it is for |
|---|---|
| `household-settings` | The shared settings (rule 33). There is one `shared.toml` and one file per app, looked up app file → shared → default. Writes take a lock, are atomic and keep comments. It also has the folder watch, moving a file, migration with seeding, and Open making a missing shared file. |
| `household-tones` | The tone maths: OKLab/OKLCH, the 0.94 surface cap, coloured text, the muted accent and what goes on it, and a held surface's hover/selected/pressed. It has Juliet's seven tones and arrangements A/B (rule 25d), APCA-W3 0.0.98G-4g contrast, and APCA's font lookup table (`required_lc`, `passes`). |
| `household-highlight` | Code colours by token class (rule 25b): scope → class, and class → colour role. The parser stays in the app. |
| `household-wait` | Waiting without freezing (rules 40–43). It covers tiers, deadlines, stalls, cancellation, keyed waits, debounce, coalescing, ordered queues, time slices and timings. |

## No crate depends on libcosmic or iced

The apps pin different libcosmic versions, and a crate that named a libcosmic type would tie every app to one version. Every crate here is plain Rust. Its types are its own: `Srgb`/`Srgba` (convertible to and from `[f32; 3]`/`[f32; 4]`), a `Colour` role, and an `Events<T>` stream. `household-wait` uses tokio, which libcosmic's executor already runs. Keep it that way: a change that needs a widget or a theme type belongs in the app's glue.

## Adding a crate to an app

```toml
[dependencies]
household-settings = { git = "ssh://git@github.com/ThomasAndJuliet/household-kit.git" }
household-tones = { git = "ssh://git@github.com/ThomasAndJuliet/household-kit.git" }
```

The commit is pinned by the app's `Cargo.lock`. To pick up a change, push here first, then run `cargo update -p household-tones` (or the crate concerned) in the app and commit the lock. An app's CI needs read access to this private repo, as it has for household-mount.

## The glue each app writes

Each app keeps a thin layer in its `ui` module that turns crate results into its own widgets and theme types:

- **Colours.** `fn c(s: Srgb) -> Color { Color::from_rgb(s.r, s.g, s.b) }` and the reverse.
- **Tones.** `with_surfaces(theme)` stays in the app. It walks the theme's saved JSON as the copies do now, and for each container component calls `household_tones::held_component(component, washes, !is_dark)`, writing back what it returns. The muted accent is `muted_accent(accent, dark)` and `on_muted(muted)`. Surfaces and text come from `Arrangement::tone(Surface::Card, mode)` and `Role::tone`.
- **Contrast in CI.** Each app's theme tests judge every text style (its colour, the colour behind it, its size in px and its weight) with `household_tones::passes_for(fg, bg, size, weight, TextUse::…)`. There is no flat threshold: the level comes from APCA's table by size and weight.
- **Highlight.** The app keeps syntect (or its own parser). For each token it passes the scope names, outermost first, to `household_highlight::class_of`. It then turns `colour(class)` into a colour: `Fixed { light, dark }` by `is_dark(plate)`, and `Muted`, `LinkStrong`, `Added` and `Removed` from its own typography. `italic(class)` sets comments in italic.
- **Settings.** The app calls the `Store` functions from a background task, never from `update` or `view` (they block). It turns `Store::watch`'s callback into a message.
- **Waits.** `let (id, events) = waits.start(spec, work);` then `Task::stream(events.map(Message::Wait))`, and in `update`, `waits.apply(event)`. The view reads `waits.get(id)` or `waits.showing()`. For `Tier::Spinner` it draws a 16 px spinner and the label. For `Tier::Bar` it draws the label, Cancel (the app's own Secondary button, if `cancellable`), a determinate bar from `fraction()` or a moving one, and `caption()` beneath. For `Tier::Person` it draws the label and Cancel. `Debounce::poke` and `Coalesce::push` give futures to run as `Task::future`.

## The crates in more detail

### household-settings

This is household-ui's `reference/household_settings.rs` at 514be8f (the 15 Oct follow-up: seeding and Open makes the shared file), with all its tests. Only its opening comment changed.

**Drift between the copies (checked 16 Oct).** cosmic-term `origin/main` (`src/household_settings.rs`), Cerea `origin/main` and household-ui `main` hold byte-identical files (blob 21c4243). No copy has drifted. Older branches (cosmic-term `shared-settings`, `ui-batch-3`, `fix-tiny-composer`, `perf-conversation-worker`, `shared-settings-watch-test`; Cerea `shared-settings`) hold the two earlier versions, from before the follow-up.

### household-tones

- `color`: `Srgb`, `Srgba`, `Oklab`, `Oklch`, hex parsing, and Ottosson's OKLab matrices.
- `tones`: `SURFACE_LIGHTNESS`, `surface`, `over`, `text_tone`, `grey`, `dark_tone`, `MUTED_*`, `muted_accent`, `on_muted`, WCAG `luminance`/`contrast_ratio` (used only by `on_muted`), and `held_component`.
- `roles`: `Role` (darkest … lightest) with `tone(mode)` and `mirrors()`, `hover(surface, wash)` for darkish, `Arrangement::{A, B}` with `role(surface)`, and `text_role(size, body)`.
- `apca`: `apca(text, bg)`, `apca_rgb8`, and `apca_over` for translucent text. It is tested against apca-w3's own test values: #888 on #fff is Lc 63.056469930209424, and #fff on #888 is −68.54146436644962.
- `fonts`: `required_lc(size_px, weight)` and `level` read APCA's table sorted by font size. This is `byFontSize` in apca-w3's `data/LUT-GseriesMay28-2022.js`, Public Beta 0.1.7 (G), the table apcacontrast.com and ARC publish. It is interpolated linearly between size rows; a weight between columns takes the lighter one's. The result is monotonic in size, and in weight up to 700 (the table holds 800 and 900 back at small sizes). `font_lookup(lc)` ports `fontLookupAPCA`, the Lc-sorted table, for reference; it asks more at 14 and 15 px, so it is not used to judge. `required_lc_for(…, TextUse)` applies the README's notes for body, sub-fluent and spot text; a spot-only cell (777) asks Lc 30 of spot text. `max_lc` is Lc 90 above 24 px at weight 300 or more. `passes`/`passes_for` judge a style.

**Drift between the tones copies.** household-ui's `tones.rs` has no muted accent. cosmic-term's and Cerea's add the muted accent, identically. cosmic-term's `with_surfaces` is the newer: it also fixes an uncapped light container whose hover is not darker than its base, and it composites with full alpha. Cerea's only redoes the states of a container it capped. The crate follows cosmic-term's. Cerea's `TEXT_DARKEST` (L 0.25 → #222222) and `TEXT_DARK` (L 0.45 → #555555) are rule 25a's tones. Rule 25d replaced them with `Role::Darkest` #222222 and `Role::Dark` #525252, so they are not in the crate; `grey` and `dark_tone` are.

### household-highlight

The table is the same in household-ui, Cerea's prose-render and cosmic-term's thread view. The crate takes cosmic-term's one extra scope (`punctuation.definition.comment` → Comment). The language aliases, `#!` clues and file-name rules differ between the copies (cosmic-term's are the richest). They need syntect, so they stay in the apps for now.

### household-wait

This is the `ui_wait` draft's core, without its widgets and without libcosmic's `Task`: `start`/`start_blocking` return an `Events<T>` stream. The kinds follow where the work happens, with the defaults Tom and Juliet agreed on 16 Oct:

| Kind | Deadline |
|---|---|
| `Local` (this machine or the LAN) | 2 s |
| `Remote` (ssh, the internet) | 10 s |
| `Long` | none; stalled after 15 s without progress, and cancellable by default |
| `Person` | none |

Tiers: `Instant` (≤ 100 ms, nothing shown), `Spinner` from 100 ms, `Bar` from 1 s. The messages are `TOO_LONG` ("This took too long and was stopped. Try again.") and `STILL_WAITING` ("Still waiting. Nothing has changed for 15 seconds."). Cancellation is tokio-util's `CancellationToken`. Keyed waits replace the old one (latest wins); `.join()` is opt-in. The tests run on tokio's paused clock, apart from the one that kills a real child process.

## Checks

```sh
~/.local/bin/neo-cargo ~/Code/household-kit household-kit -- test --workspace
~/.local/bin/neo-cargo ~/Code/household-kit household-kit -- clippy --workspace --all-targets
```

CI is to be set up by Tom. There are no workflows yet.

## Licence

Not chosen yet. There is no LICENSE file and no `license` field. The household-ui reference files the code comes from are GPL-3.0-only, and the AI-usage applet is MPL-2.0.

## Open questions

- **APCA's table and the household's text.** Black on white reaches Lc 106.
  - 12 px at weights 400–700 is for spot text only.
  - 14 px regular asks Lc 100 as content and Lc 85 as sub-fluent text. Darkest on light (Lc 91.0) passes as sub-fluent only.
  - 16 px regular prose asks Lc 90: darkest passes on light (91.0) but not on lightish (79.5).
  - Paper's 20 px body text asks Lc 86.7 (71.7 + 15), and dark gives 75.3 on light.
  - A heading above 24 px may not exceed Lc 90, and darkest on light gives 91.0.
  - The body rule (+15 below Lc 75) jumps: a cell of Lc 70 asks 85, one of 75 asks 75. That is in apca-w3's README and ARC's Gold level as written.
  - The table's reference font is Barlow (x-height 0.506); Noto Sans's is 0.536 and Libertinus's 0.429. See `~/.cache/ui-audit/apca/required-lc.md`.
  - Rule 25c's flat levels (75/60/45/30) are gone, as Juliet asked, so these pairs need a decision before the apps' CI uses `passes`.
- **Rule 25d's own claim.** It says reading text reaches Lc 75 in either arrangement. Dark (#525252) on lightish (#d7d7d7), arrangement B's page, gives Lc 63.8. In dark mode, #d0d0d0 on #272727 gives Lc −74.8.
- **The code palette.** On lightish the light colours give Lc 56–60. On dark plates Keyword, Function, Number and Tag give Lc 44–56.
- **Long work.** It has no total deadline and is flagged at 15 s without progress, not stopped. Should a stall also stop it after a while?
- **The arrangement setting.** "Page: Light | Lightish" is a shared setting in rule 25d, but it is not yet a key in `household-settings`.
- **Non-text levels.** `on_muted` still chooses by the WCAG 2 ratio. APCA's table gives only Lc 15 as the floor for non-text; rule 25c's Lc 45 for icons is the household's own level.
