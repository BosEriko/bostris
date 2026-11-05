# bostris

A fast, polished falling-block puzzle game for the desktop, built with Rust and
[Tauri 2](https://v2.tauri.app), with a graphical board, mouse-friendly menus, and
a dedicated settings panel. Gameplay and persistence run in Rust.

- Standard 10×20 field with all seven tetrominoes
- Super Rotation System (SRS) with full wall and floor kicks, plus 180° spins (SRS+ kicks)
- Seven-bag randomizer, next queue (up to six pieces) and hold
- Ghost piece, soft drop and hard drop
- Guideline scoring with T-spins, back-to-back bonuses, combos and perfect clears
- Versus mode against an AI opponent, with seven difficulty levels from D to X
- Increasing gravity, lock delay with move/rotate reset limits
- In-game settings screen for keys, DAS, ARR and more
- Finesse tracking with an optional retry-on-fault training mode
- Persistent top-ten high scores
- Colored graphical pieces, hold and next previews, and live match statistics
- Layout that scales with the window and pauses when it becomes too small
- Rendering stops while idle; precise key press and release handling
- Looping 8-bit background music with a saved mute preference
- Learn library with 14 openers and 10 spin, kick, and tuck lessons

## Installation

Requires a recent stable Rust toolchain (edition 2024), Node.js with npm, and the
[Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).
On macOS, install Xcode Command Line Tools. Linux needs WebKitGTK 4.1 and the other
packages listed by Tauri; Windows needs Microsoft C++ Build Tools and WebView2.

```sh
npm install
npm run dev
```

Build a native application and installer for your current platform:

```sh
npm run build
```

Bundles are written to `target/release/bundle/` (including `bostris.app` and a DMG
on macOS). Desktop assets are bundled locally and need no web server or network
connection at runtime. This is a desktop application controlled with a keyboard.

Music begins after your first click or key press. Use the music-note button in the
header to mute or enable it. Music pauses when the window loses focus.

## Controls

| Key            | Action                      |
| -------------- | --------------------------- |
| ← / →          | Move left / right           |
| ↓              | Soft drop                   |
| Space          | Hard drop                   |
| ↑ or X         | Rotate clockwise            |
| Z              | Rotate counterclockwise     |
| A              | Rotate 180°                 |
| C              | Hold piece                  |
| P or Esc       | Pause / resume              |
| R              | Restart                     |
| S              | Settings                    |
| Q (or Ctrl+C)  | Quit                        |

On the title screen, use ↑/↓ to choose **Marathon** or **Versus AI**, ←/→ to pick the
bot's level, and Enter (or Space) to start. The game pauses automatically when the
window loses focus or becomes too small. From the pause screen, Backspace returns to
the title; from a game-over screen, P/Esc does.

## Gameplay

Pieces fall into the 10×20 well. Complete a horizontal row to clear it; clear several
rows at once for more points. The game ends when a new piece cannot spawn or a piece
locks entirely above the visible field.

- **Hold** stores the current piece for later. It can be used once per placed piece.
- **Ghost** shows where the current piece will land.
- **Lock delay**: a piece that touches the stack waits 500 ms before locking. Moving or
  rotating resets the timer up to 15 times; reaching a new lowest row restores the resets.
- **Levels** increase every 10 lines, and gravity follows the guideline curve
  (1 row per second at level 1, far faster later on).

### Scoring

| Action                    | Points                    |
| ------------------------- | ------------------------- |
| Single / Double / Triple  | 100 / 300 / 500 × level   |
| Tetris                    | 800 × level               |
| T-spin (no lines)         | 400 × level               |
| T-spin Single / Double / Triple | 800 / 1200 / 1600 × level |
| T-spin Mini (0 / 1 / 2 lines) | 100 / 200 / 400 × level |
| Back-to-back Tetris or T-spin | ×1.5                  |
| Combo                     | +50 × combo × level       |
| Perfect clear             | +800 to +2000 × level     |
| Soft drop / hard drop     | 1 / 2 per row             |

T-spins use the three-corner rule: the last move must be a rotation, and three of the
four cells diagonal to the T's centre must be blocked. A T-spin is "mini" unless both
cells on the side the T points to are blocked, or the rotation used the last SRS kick.

## Versus AI

Choose **Versus AI** on the title screen to battle a bot. Both players get the same
piece sequence. Clearing lines sends garbage to your opponent:

| Clear                         | Lines sent |
| ----------------------------- | ---------- |
| Double / Triple / Tetris      | 1 / 2 / 4  |
| T-spin Single / Double / Triple | 2 / 4 / 6 |
| T-spin Mini Double            | 1          |
| Back-to-back bonus            | +1         |
| Combo                         | +1 to +5   |
| Perfect clear                 | +10        |

Incoming garbage appears in the red meter beside your board. It rises 0.5 s after it
arrives, at most 8 rows per piece, the next time you place a piece without clearing a
line. Clearing lines first cancels incoming garbage. The first player to top out loses.
Gravity stays at your start level for the whole match.

The bot plays through the same rules and inputs as you: it searches every reachable
placement (including tucks, T-spins and 180° spins at the highest levels), looks ahead
using hold and the next queue, and moves the piece one input per frame.

| Level | Speed (pieces/s) | Measured attack/min |
| ----- | ---------------- | ------------------- |
| D     | 0.75             | ~12                 |
| C     | 1.1              | ~20                 |
| B     | 1.25             | ~32                 |
| A     | 1.55             | ~47                 |
| A+    | 2.0              | ~65                 |
| S     | 2.4              | ~85                 |
| X     | 3.4              | ~120                |

Attack per minute was measured in solo play without garbage. The levels are named
after TETR.IO ranks, and the targets are approximate figures for players of those
ranks, so treat them as a rough guide rather than an exact match. Lower levels also
search less, use fewer T-spins and make deliberate mistakes. The chosen level is saved
as `bot_level` in the configuration file.

To re-measure the levels:

```sh
cargo test --release benchmark_levels -- --ignored --nocapture
```

## Learn mode

Choose **Openers & spin lab** on the title screen to browse the lesson library.
Search or filter by openers and spins, then select a lesson for its setup, steps,
tips, and technique source.

- **Build first bag** guides the TKI Castle Top, Albatross, and Stickspin opening
  stages. Follow the fixed queue and place each piece in its outline. Completion
  checks this first-bag build, rather than the entire opener and its continuations.
- **Start challenge** loads a preset spin, kick, or tuck. Follow the hint, rotate
  into the target, and hard drop. Success uses the real rotation and scoring rules.
- **Free practice** opens an ungraded board for other openers. The T-Spin Triple
  lesson is a reference lesson without a preset challenge.

Practice has no passive gravity or automatic locking. Hold Down to lower a piece;
Space locks it. Hold is disabled in guided builds and preset challenges. Retry
restores the exercise. Practice scores are kept separate from Marathon high scores.
Non-T tucks and kicks are placement techniques, not additional spin scoring bonuses.

## Settings

Press **S** on the title, pause or game-over screen (or during play, which pauses) to
open the settings screen.

| Key            | In settings                                   |
| -------------- | --------------------------------------------- |
| ↑ / ↓          | Select an option                              |
| ← / →          | Adjust a value or toggle                      |
| Enter          | Toggle, or rebind the selected control        |
| A              | Add another key to the selected control       |
| Delete / Bksp  | Restore the selected control's default keys   |
| Esc            | Save and go back                              |

When rebinding, the next key you press is bound, including Esc and Enter. Taking a key
that is another control's only key swaps the two controls' keys, so no control is ever
left unbound. Changes take effect immediately and are written to the configuration file.

## Finesse

Finesse is placing each piece with the fewest inputs. Every tap, DAS hold and rotation
counts as one input. Each placement is compared with the minimum needed to reach the
same spot from spawn, and the stats panel shows your finesse accuracy and fault count.
Tucks and spins that cannot be reached by dropping straight down are not judged.

Enable **180 in finesse** if you use 180° spins, so a single 180° spin counts as one
input instead of two rotations.

Turn on **Retry on fault** in settings to practise: a placement with extra inputs does
not lock; the same piece respawns at the top so you can place it again.

Finesse counting uses native key press and release events, so separate taps and held
keys are distinguished.

## Configuration

A commented default configuration file is created on first launch, and the settings
screen rewrites it when you change something. Existing configuration and high-score
files from the terminal version are reused:

| Platform | Config file                                              |
| -------- | -------------------------------------------------------- |
| Linux    | `~/.config/bostris/config.toml`                          |
| macOS    | `~/Library/Application Support/bostris/config.toml`      |
| Windows  | `%APPDATA%\bostris\config.toml`                          |

```toml
[gameplay]
start_level = 1        # 1-30
das_ms = 167           # Delayed Auto Shift, 0-1000
arr_ms = 33            # Auto Repeat Rate, 0-500 (0 = instant)
soft_drop_factor = 20  # multiple of gravity, 1-100
lock_delay_ms = 500    # 100-5000
max_lock_resets = 15   # 0-100
line_clear_ms = 250    # 0-1000
preview_count = 5      # 0-6
ghost = true
finesse_retry = false  # retry pieces placed with extra inputs
finesse_180 = false    # count 180 degree spins when judging finesse

[display]
color_mode = "auto"    # auto (truecolor), truecolor, 256, 16

[versus]
bot_level = "A+"       # D, C, B, A, A+, S or X

[keys]
left = ["Left"]
right = ["Right"]
soft_drop = ["Down"]
hard_drop = ["Space"]
rotate_cw = ["Up", "x"]
rotate_ccw = ["z"]
rotate_180 = ["a"]
hold = ["c"]
pause = ["p", "Esc"]
restart = ["r"]
settings = ["s"]
quit = ["q"]
```

Key names: `Left`, `Right`, `Up`, `Down`, `Space`, `Esc`, `Enter`, `Tab`, `Backspace`,
`Home`, `End`, `PageUp`, `PageDown`, `Insert`, `Delete`, `F1`–`F12`, or any single
character. Letters are case-insensitive.

Missing settings use their defaults. Out-of-range values are clamped and unknown keys
are ignored. A file that cannot be parsed falls back to the defaults entirely.
Configuration warnings and save errors are displayed below the game.

### DAS, ARR and key release events

The desktop window reports key presses and releases directly. DAS/ARR and held soft
drop work independently of system keyboard repeat settings. Switching away from the
window pauses the game and releases held inputs; press P or Esc to resume.

## High scores

The top ten scores are saved to `highscores.toml` in the platform data directory
(`~/.local/share/bostris` on Linux, `~/Library/Application Support/bostris` on macOS,
`%APPDATA%\bostris` on Windows). A missing or corrupted file is treated as empty, and
writes are atomic.

## Development

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
npm run build
```

| Module          | Responsibility                                   |
| --------------- | ------------------------------------------------ |
| `piece`         | Tetromino shapes, rotation states, SRS kick data |
| `board`         | Fixed-size grid, collision, line clearing        |
| `bag`           | Seven-bag randomizer and PRNG                    |
| `scoring`       | Points, levels, gravity curve                    |
| `game`          | Game state, gravity, lock delay, hold            |
| `input`         | Keymap and DAS/ARR auto-shift                    |
| `finesse`       | Optimal input search and finesse statistics      |
| `bot`           | AI opponent: placement search and evaluation     |
| `settings`      | Settings menu model and key rebinding            |
| `config`        | Configuration parsing and validation             |
| `highscores`    | High-score persistence                           |
| `ui`            | Graphical game snapshots and settings models     |
| `app`           | Fixed-timestep simulation and event handling     |
| `desktop`       | Tauri commands, window events and game updates   |
| `training`      | Lesson catalog, preset boards and completion     |
| `frontend/`     | Graphical interface, mouse and keyboard input    |

Game logic has no dependency on the window and is covered by unit tests.
The simulation runs at a fixed 60 Hz timestep on a monotonic clock with bounded
catch-up. The frontend requests frames while playing and on input or resize;
unchanged snapshots are not sent again. No frontend framework or bundler is required.


## Music credit

[Title Theme (8-Bit Style)](https://opengameart.org/content/title-theme-8-bit-style)
by Kat, licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
The original WAV is bundled unchanged; playback uses reduced volume and looping.
