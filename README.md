# bostris

A fast, polished falling-block puzzle game for the terminal, written in Rust with
[Ratatui](https://ratatui.rs) and [Crossterm](https://github.com/crossterm-rs/crossterm).

- Standard 10×20 field with all seven tetrominoes
- Super Rotation System (SRS) with full wall and floor kicks
- Seven-bag randomizer, next queue (up to six pieces) and hold
- Ghost piece, soft drop and hard drop
- Guideline scoring with back-to-back Tetris bonus and combos
- Increasing gravity, lock delay with move/rotate reset limits
- Configurable keys, DAS and ARR
- Persistent top-ten high scores
- True color, 256-color and 16-color palettes
- Layout that scales up on large terminals and degrades gracefully on small ones
- Near-zero CPU use when idle, a few megabytes of memory

## Installation

Requires a recent stable Rust toolchain (edition 2024).

```sh
git clone <repository-url> bostris
cd bostris
cargo install --path .
```

Or build a standalone executable without installing:

```sh
cargo build --release
./target/release/bostris
```

The binary has no runtime dependencies and runs on macOS, Linux and Windows.

## Controls

| Key            | Action                      |
| -------------- | --------------------------- |
| ← / →          | Move left / right           |
| ↓              | Soft drop                   |
| Space          | Hard drop                   |
| ↑ or X         | Rotate clockwise            |
| Z              | Rotate counterclockwise     |
| C              | Hold piece                  |
| P or Esc       | Pause / resume              |
| R              | Restart                     |
| Q (or Ctrl+C)  | Quit                        |

Press any key on the title screen to begin. The game pauses automatically when the
terminal loses focus or becomes too small.

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

| Action           | Points             |
| ---------------- | ------------------ |
| Single           | 100 × level        |
| Double           | 300 × level        |
| Triple           | 500 × level        |
| Tetris           | 800 × level        |
| Back-to-back Tetris | ×1.5            |
| Combo            | +50 × combo × level |
| Soft drop        | 1 per row          |
| Hard drop        | 2 per row          |

## Configuration

A commented default configuration file is created on first launch. Run
`bostris --help` to print its exact path:

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

[display]
color_mode = "auto"    # auto, truecolor, 256, 16

[keys]
left = ["Left"]
right = ["Right"]
soft_drop = ["Down"]
hard_drop = ["Space"]
rotate_cw = ["Up", "x"]
rotate_ccw = ["z"]
hold = ["c"]
pause = ["p", "Esc"]
restart = ["r"]
quit = ["q"]
```

Key names: `Left`, `Right`, `Up`, `Down`, `Space`, `Esc`, `Enter`, `Tab`, `Backspace`,
`Home`, `End`, `PageUp`, `PageDown`, `Insert`, `Delete`, `F1`–`F12`, or any single
character. Letters are case-insensitive.

Missing settings use their defaults. Out-of-range values are clamped and unknown keys
are ignored, and any problems are reported on stderr after the game exits. A file that
cannot be parsed falls back to the defaults entirely.

### DAS, ARR and key release events

Precise DAS/ARR and held soft drop need the terminal to report key releases. bostris
enables the kitty keyboard protocol when the terminal supports it (kitty, WezTerm,
Ghostty, foot, Alacritty, recent iTerm2) and always has release events on Windows.
In other terminals, holding a key relies on the terminal's own key repeat, so
horizontal auto-repeat follows your system keyboard repeat settings.

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
cargo build --release
```

| Module          | Responsibility                                   |
| --------------- | ------------------------------------------------ |
| `piece`         | Tetromino shapes, rotation states, SRS kick data |
| `board`         | Fixed-size grid, collision, line clearing        |
| `bag`           | Seven-bag randomizer and PRNG                    |
| `scoring`       | Points, levels, gravity curve                    |
| `game`          | Game state, gravity, lock delay, hold            |
| `input`         | Keymap and DAS/ARR auto-shift                    |
| `config`        | Configuration parsing and validation             |
| `highscores`    | High-score persistence                           |
| `ui`            | Ratatui rendering and responsive layout          |
| `app`           | Fixed-timestep loop and event handling           |

Game logic has no dependency on the terminal and is covered by unit tests.
The simulation runs at a fixed 60 Hz timestep on a monotonic clock, rendering
happens only when the game state changes (capped at 60 FPS), and the loop blocks
on input whenever the game is paused, over or waiting to start.
