use std::fmt::Write;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Widget};

use crate::board::{VISIBLE_HEIGHT, WIDTH};
use crate::bot::Level;
use crate::config::ColorMode;
use crate::game::{Game, ToastEvent};
use crate::highscores::HighScores;
use crate::input::{Action, Keymap, key_label};
use crate::piece::{Kind, Piece, Rotation};
use crate::scoring::Spin;
use crate::settings::{Capture, Field, ITEMS, Item, SettingsMenu};

const SIDE_WIDTH: [u16; 2] = [18, 20];
const VERSUS_SIDE_WIDTH: [u16; 2] = [12, 20];
const HELP_WIDTH: u16 = 24;
const GAP: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Ready,
    Playing,
    Paused,
    GameOver { rank: Option<usize> },
    VersusOver { won: bool },
    Settings,
}

#[derive(Clone, Copy)]
pub struct Versus<'a> {
    pub opponent: &'a Game,
    pub level: Level,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Title {
    pub selected: usize,
    pub level: Level,
}

pub struct View<'a> {
    pub game: &'a Game,
    pub screen: Screen,
    pub scores: &'a HighScores,
    pub ghost: bool,
    pub preview_count: usize,
    pub menu: Option<&'a SettingsMenu>,
    pub key_releases: bool,
    pub versus: Option<Versus<'a>>,
    pub title: Title,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Palette {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl Palette {
    fn detect(mode: ColorMode) -> Self {
        match mode {
            ColorMode::TrueColor => Palette::TrueColor,
            ColorMode::Ansi256 => Palette::Ansi256,
            ColorMode::Ansi16 => Palette::Ansi16,
            ColorMode::Auto => {
                let var = |name| std::env::var(name).unwrap_or_default().to_ascii_lowercase();
                let colorterm = var("COLORTERM");
                let term = var("TERM");
                if colorterm.contains("truecolor")
                    || colorterm.contains("24bit")
                    || std::env::var_os("WT_SESSION").is_some()
                    || cfg!(windows)
                {
                    Palette::TrueColor
                } else if term.contains("256") {
                    Palette::Ansi256
                } else {
                    Palette::Ansi16
                }
            }
        }
    }
}

struct Theme {
    palette: Palette,
    board_bg: Color,
    grid: Color,
    border: Color,
    accent: Color,
    label: Color,
    value: Color,
    flash: Color,
    muted: Color,
    highlight: Color,
    warn: Color,
}

impl Theme {
    fn new(palette: Palette) -> Self {
        match palette {
            Palette::TrueColor => Self {
                palette,
                board_bg: Color::Rgb(17, 19, 26),
                grid: Color::Rgb(44, 48, 60),
                border: Color::Rgb(86, 94, 120),
                accent: Color::Rgb(122, 162, 247),
                label: Color::Rgb(140, 148, 170),
                value: Color::Rgb(230, 233, 240),
                flash: Color::Rgb(245, 245, 255),
                muted: Color::Rgb(70, 75, 90),
                highlight: Color::Rgb(36, 41, 58),
                warn: Color::Rgb(238, 110, 110),
            },
            Palette::Ansi256 => Self {
                palette,
                board_bg: Color::Indexed(234),
                grid: Color::Indexed(238),
                border: Color::Indexed(60),
                accent: Color::Indexed(111),
                label: Color::Indexed(246),
                value: Color::Indexed(255),
                flash: Color::Indexed(231),
                muted: Color::Indexed(240),
                highlight: Color::Indexed(236),
                warn: Color::Indexed(203),
            },
            Palette::Ansi16 => Self {
                palette,
                board_bg: Color::Reset,
                grid: Color::DarkGray,
                border: Color::Gray,
                accent: Color::LightBlue,
                label: Color::Gray,
                value: Color::White,
                flash: Color::White,
                muted: Color::DarkGray,
                highlight: Color::DarkGray,
                warn: Color::LightRed,
            },
        }
    }

    fn piece(&self, kind: Kind) -> Color {
        match self.palette {
            Palette::TrueColor => match kind {
                Kind::I => Color::Rgb(64, 210, 230),
                Kind::O => Color::Rgb(245, 210, 70),
                Kind::T => Color::Rgb(178, 108, 232),
                Kind::S => Color::Rgb(96, 208, 104),
                Kind::Z => Color::Rgb(238, 90, 90),
                Kind::J => Color::Rgb(74, 124, 240),
                Kind::L => Color::Rgb(245, 155, 60),
                Kind::Garbage => Color::Rgb(92, 96, 110),
            },
            Palette::Ansi256 => Color::Indexed(match kind {
                Kind::I => 44,
                Kind::O => 220,
                Kind::T => 135,
                Kind::S => 77,
                Kind::Z => 203,
                Kind::J => 69,
                Kind::L => 208,
                Kind::Garbage => 242,
            }),
            Palette::Ansi16 => match kind {
                Kind::I => Color::Cyan,
                Kind::O => Color::Yellow,
                Kind::T => Color::Magenta,
                Kind::S => Color::Green,
                Kind::Z => Color::Red,
                Kind::J => Color::Blue,
                Kind::L => Color::LightRed,
                Kind::Garbage => Color::DarkGray,
            },
        }
    }

    fn block(&self, title: &'static str) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(self.border))
            .title(Line::from(title).centered())
            .title_style(Style::new().fg(self.accent).add_modifier(Modifier::BOLD))
    }
}

struct Panel {
    scale: u16,
    board: Rect,
    left: Rect,
    right: Rect,
    meter: Option<Rect>,
    label: Option<Rect>,
}

struct Layout {
    player: Panel,
    opponent: Option<Panel>,
    help: Option<Rect>,
    title: Option<Rect>,
}

fn board_size(scale: u16) -> (u16, u16) {
    (
        WIDTH as u16 * 2 * scale + 2,
        VISIBLE_HEIGHT as u16 * scale + 2,
    )
}

pub fn min_size(versus: bool) -> (u16, u16) {
    let (board_w, board_h) = board_size(1);
    if versus {
        let side = VERSUS_SIDE_WIDTH[0];
        (side + 2 * board_w + 6 + side, board_h + 1)
    } else {
        (board_w + 2 * (SIDE_WIDTH[0] + GAP), board_h)
    }
}

fn single_layout(area: Rect) -> Option<Layout> {
    for scale in [2u16, 1] {
        for with_help in [true, false] {
            let side = SIDE_WIDTH[scale as usize - 1];
            let (board_w, board_h) = board_size(scale);
            let help_w = if with_help { HELP_WIDTH + GAP } else { 0 };
            let total_w = board_w + 2 * (side + GAP) + help_w;
            if total_w > area.width || board_h > area.height {
                continue;
            }
            let with_title = area.height > board_h;
            let total_h = board_h + u16::from(with_title);
            let x = area.x + (area.width - total_w) / 2;
            let y = area.y + (area.height - total_h) / 2 + u16::from(with_title);
            let left = Rect::new(x, y, side, board_h);
            let board = Rect::new(x + side + GAP, y, board_w, board_h);
            let right = Rect::new(board.right() + GAP, y, side, board_h);
            let help = with_help.then(|| Rect::new(right.right() + GAP, y, HELP_WIDTH, board_h));
            let title = with_title.then(|| Rect::new(board.x, y - 1, board_w, 1));
            return Some(Layout {
                player: Panel {
                    scale,
                    board,
                    left,
                    right,
                    meter: None,
                    label: None,
                },
                opponent: None,
                help,
                title,
            });
        }
    }
    None
}

fn versus_panel(x: u16, y: u16, scale: u16, side: u16, compact: bool) -> Panel {
    let (board_w, board_h) = board_size(scale);
    let side = if compact { 0 } else { side };
    let left = Rect::new(x, y, side, board_h);
    let meter_x = if compact { x } else { left.right() + 1 };
    let meter = Rect::new(meter_x, y, 1, board_h);
    let board = Rect::new(meter.right(), y, board_w, board_h);
    let right = Rect::new(board.right() + u16::from(!compact), y, side, board_h);
    Panel {
        scale,
        board,
        left,
        right,
        meter: Some(meter),
        label: Some(Rect::new(board.x, y - 1, board_w, 1)),
    }
}

fn versus_layout(area: Rect) -> Option<Layout> {
    for scale in [2u16, 1] {
        for compact in [false, true] {
            let side = VERSUS_SIDE_WIDTH[scale as usize - 1];
            let (board_w, board_h) = board_size(scale);
            let player_w = side + 1 + 1 + board_w + 1 + side;
            let opponent_w = if compact { 1 + board_w } else { player_w };
            let total_w = player_w + GAP + opponent_w;
            let total_h = board_h + 1;
            if total_w > area.width || total_h > area.height {
                continue;
            }
            let x = area.x + (area.width - total_w) / 2;
            let y = area.y + (area.height - total_h) / 2 + 1;
            let player = versus_panel(x, y, scale, side, false);
            let opponent = versus_panel(x + player_w + GAP, y, scale, side, compact);
            return Some(Layout {
                player,
                opponent: Some(opponent),
                help: None,
                title: None,
            });
        }
    }
    None
}

fn compute_layout(area: Rect, versus: bool) -> Option<Layout> {
    if versus {
        versus_layout(area)
    } else {
        single_layout(area)
    }
}

pub fn fits(width: u16, height: u16, versus: bool) -> bool {
    compute_layout(Rect::new(0, 0, width, height), versus).is_some()
}

pub struct Renderer {
    theme: Theme,
    keys: [String; Action::ALL.len()],
    scratch: String,
    in_tmux: bool,
}

impl Renderer {
    pub fn new(mode: ColorMode, keymap: &Keymap) -> Self {
        let keys = Action::ALL.map(|action| {
            let mut labels: Vec<String> = keymap.keys_for(action).map(key_label).collect();
            labels.sort_by_key(|l| l.chars().count());
            labels.join("/")
        });
        Self {
            theme: Theme::new(Palette::detect(mode)),
            keys,
            scratch: String::with_capacity(64),
            in_tmux: std::env::var_os("TMUX").is_some(),
        }
    }

    fn key(&self, action: Action) -> &str {
        key_of(&self.keys, action)
    }

    pub fn render(&mut self, frame: &mut Frame, view: &View) {
        let area = frame.area();
        let buf = frame.buffer_mut();
        if view.screen == Screen::Settings
            && let Some(menu) = view.menu
        {
            self.render_settings(buf, area, menu, view.key_releases);
            return;
        }
        let versus = view.versus.filter(|_| view.screen != Screen::Ready);
        let Some(layout) = compute_layout(area, versus.is_some()) else {
            self.render_too_small(buf, area, versus.is_some());
            return;
        };
        if let Some(title) = layout.title {
            let style = Style::new()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD);
            center(buf, title, title.y, "B O S T R I S", style);
        }
        let player = &layout.player;
        self.render_board(buf, player, view, view.game, view.ghost);
        self.render_left(buf, player, view, view.game);
        self.render_right(buf, player, view, view.game);
        if let Some(versus) = versus {
            self.render_meter(buf, player, view.game);
            self.render_label(buf, player, "YOU", None);
            if let Some(panel) = &layout.opponent {
                let opponent = versus.opponent;
                self.render_board(buf, panel, view, opponent, false);
                self.render_meter(buf, panel, opponent);
                if panel.left.width > 0 {
                    self.render_left(buf, panel, view, opponent);
                    self.render_right(buf, panel, view, opponent);
                    self.render_label(buf, panel, "BOT", Some(versus.level));
                } else {
                    self.render_label(buf, panel, "BOT", Some(versus.level));
                }
            }
        }
        if let Some(help) = layout.help {
            self.render_help(buf, help);
        }
        match view.screen {
            Screen::Playing | Screen::Settings => {}
            Screen::Ready => self.render_ready(buf, player, view),
            Screen::Paused => self.render_paused(buf, player),
            Screen::GameOver { rank } => self.render_game_over(buf, player, view, rank),
            Screen::VersusOver { won } => {
                if let Some(versus) = versus {
                    self.render_versus_over(buf, player, view.game, versus, won);
                }
            }
        }
    }

    fn render_label(&mut self, buf: &mut Buffer, panel: &Panel, name: &str, level: Option<Level>) {
        let Some(rect) = panel.label else {
            return;
        };
        self.scratch.clear();
        self.scratch.push_str(name);
        if let Some(level) = level {
            self.scratch.push_str(" · ");
            self.scratch.push_str(level.name());
        }
        let style = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        center(buf, rect, rect.y, &self.scratch, style);
    }

    fn render_meter(&self, buf: &mut Buffer, panel: &Panel, game: &Game) {
        let Some(rect) = panel.meter else {
            return;
        };
        let inner_top = rect.y + 1;
        let inner_height = rect.height.saturating_sub(2);
        let (ready, total) = game.pending_garbage();
        let s = panel.scale;
        let ready_cells = (ready as u16 * s).min(inner_height);
        let total_cells = (total as u16 * s).min(inner_height);
        for i in 0..inner_height {
            let y = inner_top + inner_height - 1 - i;
            let (symbol, style) = if i < ready_cells {
                ("█", Style::new().fg(self.theme.warn))
            } else if i < total_cells {
                ("▓", Style::new().fg(self.theme.label))
            } else {
                ("│", Style::new().fg(self.theme.grid))
            };
            if let Some(cell) = buf.cell_mut((rect.x, y)) {
                cell.set_symbol(symbol).set_style(style);
            }
        }
    }

    fn render_too_small(&mut self, buf: &mut Buffer, area: Rect, versus: bool) {
        let (min_w, min_h) = min_size(versus);
        let y = area.y + area.height / 2;
        let title = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let text = Style::new().fg(self.theme.label);
        center(buf, area, y.saturating_sub(1), "Terminal too small", title);
        self.scratch.clear();
        let _ = write!(
            self.scratch,
            "Need {min_w}×{min_h}, have {}×{}",
            area.width, area.height
        );
        center(buf, area, y, &self.scratch, text);
        if area.height > 2 {
            self.scratch.clear();
            let _ = write!(
                self.scratch,
                "Resize, or press {} to quit",
                key_of(&self.keys, Action::Quit)
            );
            center(buf, area, y + 1, &self.scratch, text);
        }
    }

    fn fill_cell(
        &self,
        buf: &mut Buffer,
        layout: &Panel,
        x: u16,
        y: u16,
        symbol: &str,
        style: Style,
    ) {
        let s = layout.scale;
        for dy in 0..s {
            for dx in 0..2 * s {
                if let Some(cell) = buf.cell_mut((x + dx, y + dy)) {
                    cell.set_symbol(symbol).set_style(style);
                }
            }
        }
    }

    fn board_cell_origin(layout: &Panel, x: i8, y: i8) -> Option<(u16, u16)> {
        if x < 0 || y < 0 || x as usize >= WIDTH || y as usize >= VISIBLE_HEIGHT {
            return None;
        }
        let s = layout.scale;
        let inner_x = layout.board.x + 1;
        let inner_y = layout.board.y + 1;
        Some((
            inner_x + x as u16 * 2 * s,
            inner_y + (VISIBLE_HEIGHT as u16 - 1 - y as u16) * s,
        ))
    }

    fn render_board(
        &self,
        buf: &mut Buffer,
        layout: &Panel,
        view: &View,
        game: &Game,
        ghost: bool,
    ) {
        let theme = &self.theme;
        theme.block("").render(layout.board, buf);
        let hidden = matches!(view.screen, Screen::Paused);
        let empty = Style::new().bg(theme.board_bg).fg(theme.grid);
        let clearing = game.line_clear_progress();
        let s = layout.scale;
        for y in 0..VISIBLE_HEIGHT as i8 {
            let row = game.board().row(y as usize);
            let flash = clearing.filter(|(rows, _)| rows & (1 << y) != 0);
            for x in 0..WIDTH as i8 {
                let Some((cx, cy)) = Self::board_cell_origin(layout, x, y) else {
                    continue;
                };
                self.fill_cell(buf, layout, cx, cy, " ", Style::new().bg(theme.board_bg));
                let content = if hidden { None } else { row[x as usize] };
                match (content, flash) {
                    (Some(_), Some((_, progress))) => {
                        let distance = (f32::from(x as u8) + 0.5 - WIDTH as f32 / 2.0).abs();
                        if distance >= progress * WIDTH as f32 / 2.0 {
                            self.fill_cell(buf, layout, cx, cy, " ", Style::new().bg(theme.flash));
                        }
                    }
                    (Some(kind), None) => {
                        self.fill_cell(
                            buf,
                            layout,
                            cx,
                            cy,
                            " ",
                            Style::new().bg(theme.piece(kind)),
                        );
                    }
                    (None, _) => {
                        if let Some(cell) = buf.cell_mut((cx + 2 * s - 1, cy + s / 2)) {
                            cell.set_symbol("·").set_style(empty);
                        }
                    }
                }
            }
        }
        if hidden || matches!(view.screen, Screen::Ready) {
            return;
        }
        if ghost && let Some(ghost) = game.ghost() {
            let style = Style::new().bg(theme.board_bg).fg(theme.piece(ghost.kind));
            for (x, y) in ghost.cells() {
                if let Some((cx, cy)) = Self::board_cell_origin(layout, x, y) {
                    self.fill_cell(buf, layout, cx, cy, "░", style);
                }
            }
        }
        if let Some(active) = game.active() {
            let style = Style::new().bg(theme.piece(active.kind));
            for (x, y) in active.cells() {
                if let Some((cx, cy)) = Self::board_cell_origin(layout, x, y) {
                    self.fill_cell(buf, layout, cx, cy, " ", style);
                }
            }
        }
    }

    fn render_preview(
        &self,
        buf: &mut Buffer,
        layout: &Panel,
        inner: Rect,
        y: u16,
        kind: Kind,
        dim: bool,
    ) {
        let s = layout.scale;
        let piece = Piece {
            kind,
            rotation: Rotation::Spawn,
            x: 0,
            y: 0,
        };
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (i8::MAX, i8::MIN, i8::MAX, i8::MIN);
        for (x, y) in piece.cells() {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        let width = (max_x - min_x + 1) as u16 * 2 * s;
        let height = (max_y - min_y + 1) as u16 * s;
        let origin_x = inner.x + inner.width.saturating_sub(width) / 2;
        let origin_y = y + (2 * s).saturating_sub(height) / 2;
        let color = if dim {
            self.theme.muted
        } else {
            self.theme.piece(kind)
        };
        for (x, py) in piece.cells() {
            let cx = origin_x + (x - min_x) as u16 * 2 * s;
            let cy = origin_y + (max_y - py) as u16 * s;
            self.fill_cell(buf, layout, cx, cy, " ", Style::new().bg(color));
        }
    }

    fn render_left(&mut self, buf: &mut Buffer, layout: &Panel, view: &View, game: &Game) {
        let s = layout.scale;
        let left = layout.left;
        let hold_rect = Rect::new(left.x, left.y, left.width, 2 * s + 4);
        let hold_block = self.theme.block(" HOLD ");
        let hold_inner = hold_block.inner(hold_rect);
        hold_block.render(hold_rect, buf);
        if let Some(kind) = game.hold_piece()
            && view.screen != Screen::Paused
        {
            self.render_preview(
                buf,
                layout,
                hold_inner,
                hold_inner.y + 1,
                kind,
                !game.can_hold(),
            );
        }

        let scoring = game.scoring();
        let elapsed = game.elapsed().as_secs();
        let versus = view.versus.is_some() && view.screen != Screen::Ready;
        let rows: &[(&str, Stat)] = if versus {
            let (apm, pps) = rates(game);
            &[
                ("APM", Stat::Decimal(apm, 1)),
                ("PPS", Stat::Decimal(pps, 2)),
                ("Sent", Stat::Number(u64::from(game.attack_total()))),
                ("Lines", Stat::Number(u64::from(scoring.lines))),
                ("Time", Stat::Time(elapsed)),
            ]
        } else {
            let best = view.scores.best().max(scoring.score);
            let finesse = game.finesse();
            &[
                ("Score", Stat::Number(scoring.score)),
                ("Best", Stat::Number(best)),
                ("Level", Stat::Number(u64::from(scoring.level()))),
                ("Lines", Stat::Number(u64::from(scoring.lines))),
                ("Time", Stat::Time(elapsed)),
                ("Pieces", Stat::Number(u64::from(game.pieces_placed()))),
                ("Finesse", Stat::Percent(finesse.accuracy())),
                ("Faults", Stat::Number(u64::from(finesse.faults))),
            ]
        };
        let stats_rect = Rect::new(
            left.x,
            hold_rect.bottom(),
            left.width,
            rows.len() as u16 + 2,
        );
        let stats_block = self.theme.block(" STATS ");
        let inner = stats_block.inner(stats_rect);
        stats_block.render(stats_rect, buf);
        for (i, &(label, stat)) in rows.iter().enumerate() {
            let y = inner.y + i as u16;
            self.scratch.clear();
            match stat {
                Stat::Number(n) => write_grouped(&mut self.scratch, n),
                Stat::Time(t) => {
                    let _ = write!(self.scratch, "{}:{:02}", t / 60, t % 60);
                }
                Stat::Percent(None) => self.scratch.push('–'),
                Stat::Percent(Some(p)) if p >= 99.95 => self.scratch.push_str("100%"),
                Stat::Percent(Some(p)) => {
                    let _ = write!(self.scratch, "{p:.1}%");
                }
                Stat::Decimal(value, precision) => {
                    let _ = write!(self.scratch, "{value:.precision$}");
                }
            }
            let value_len = self.scratch.chars().count() as u16;
            let margin = u16::from(label.len() as u16 + value_len + 3 <= inner.width);
            buf.set_string(
                inner.x + margin,
                y,
                label,
                Style::new().fg(self.theme.label),
            );
            let x = inner.right().saturating_sub(value_len + margin);
            let style = Style::new()
                .fg(self.theme.value)
                .add_modifier(Modifier::BOLD);
            buf.set_string(x, y, &self.scratch, style);
        }

        let Some(toast) = game.toast() else {
            return;
        };
        let area = Rect::new(
            left.x,
            stats_rect.bottom() + 1,
            left.width,
            left.bottom().saturating_sub(stats_rect.bottom() + 1),
        );
        if area.height == 0 {
            return;
        }
        let event = match toast.event {
            ToastEvent::Clear(event) => event,
            ToastEvent::Fault { fault, retry } => {
                let warn = Style::new()
                    .fg(self.theme.warn)
                    .add_modifier(Modifier::BOLD);
                let label = Style::new().fg(self.theme.label);
                let title = if area.width >= 13 {
                    "FINESSE FAULT"
                } else {
                    "FAULT"
                };
                center(buf, area, area.y, title, warn);
                self.scratch.clear();
                let _ = write!(
                    self.scratch,
                    "{} inputs, need {}",
                    fault.used, fault.optimal
                );
                center(buf, area, area.y + 1, &self.scratch, label);
                if retry {
                    center(buf, area, area.y + 2, "RETRY", warn);
                }
                return;
            }
        };
        let name = match event.lines {
            0 => "",
            1 => "SINGLE",
            2 => "DOUBLE",
            3 => "TRIPLE",
            _ => "TETRIS",
        };
        let headline = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let mut y = area.y;
        if event.back_to_back && y < area.bottom() {
            center(
                buf,
                area,
                y,
                "BACK-TO-BACK",
                Style::new().fg(self.theme.label),
            );
            y += 1;
        }
        let spin = match event.spin {
            Spin::None => None,
            Spin::Mini => Some("T-SPIN MINI"),
            Spin::Full => Some("T-SPIN"),
        };
        for text in [
            spin,
            (!name.is_empty()).then_some(name),
            event.perfect.then_some("ALL CLEAR"),
        ]
        .into_iter()
        .flatten()
        {
            if y < area.bottom() {
                center(buf, area, y, text, headline);
                y += 1;
            }
        }
        if event.combo > 0 && y < area.bottom() {
            self.scratch.clear();
            let _ = write!(self.scratch, "COMBO ×{}", event.combo);
            center(
                buf,
                area,
                y,
                &self.scratch,
                Style::new().fg(self.theme.label),
            );
            y += 1;
        }
        if y < area.bottom() {
            self.scratch.clear();
            self.scratch.push('+');
            if versus {
                write_grouped(&mut self.scratch, u64::from(event.attack));
                self.scratch
                    .push_str(if event.attack == 1 { " line" } else { " lines" });
            } else {
                write_grouped(&mut self.scratch, event.points);
            }
            center(
                buf,
                area,
                y,
                &self.scratch,
                Style::new().fg(self.theme.value),
            );
        }
    }

    fn render_right(&self, buf: &mut Buffer, layout: &Panel, view: &View, game: &Game) {
        if view.preview_count == 0 {
            return;
        }
        let s = layout.scale;
        let slot = 2 * s + 1;
        let height = (3 + view.preview_count as u16 * slot).min(layout.right.height);
        let rect = Rect::new(layout.right.x, layout.right.y, layout.right.width, height);
        let block = self.theme.block(" NEXT ");
        let inner = block.inner(rect);
        block.render(rect, buf);
        if view.screen == Screen::Paused {
            return;
        }
        for (i, kind) in game.preview().take(view.preview_count).enumerate() {
            let y = inner.y + 1 + i as u16 * slot;
            if y + 2 * s > inner.bottom() {
                break;
            }
            self.render_preview(buf, layout, inner, y, kind, false);
        }
    }

    fn render_help(&self, buf: &mut Buffer, area: Rect) {
        let rect = Rect::new(
            area.x,
            area.y,
            area.width,
            (Action::ALL.len() as u16 + 2).min(area.height),
        );
        let block = self.theme.block(" CONTROLS ");
        let inner = block.inner(rect);
        block.render(rect, buf);
        for (i, action) in Action::ALL.into_iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.bottom() {
                break;
            }
            buf.set_string(
                inner.x + 1,
                y,
                action.label(),
                Style::new().fg(self.theme.label),
            );
            let keys = self.key(action);
            let x = inner
                .right()
                .saturating_sub(keys.chars().count() as u16 + 1);
            buf.set_string(
                x,
                y,
                keys,
                Style::new()
                    .fg(self.theme.value)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    fn overlay(&self, buf: &mut Buffer, layout: &Panel, lines: u16) -> Rect {
        let board = layout.board;
        let height = (lines + 2).min(board.height.saturating_sub(2));
        let rect = Rect::new(
            board.x + 1,
            board.y + (board.height - height) / 2,
            board.width.saturating_sub(2),
            height,
        );
        buf.set_style(rect, Style::new().bg(self.theme.board_bg));
        for y in rect.top()..rect.bottom() {
            for x in rect.left()..rect.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ");
                }
            }
        }
        let block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::new().fg(self.theme.accent).bg(self.theme.board_bg));
        let inner = block.inner(rect);
        block.render(rect, buf);
        inner
    }

    fn render_key_line(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        y: u16,
        action: Action,
        label: &str,
    ) {
        if y >= area.bottom() {
            return;
        }
        self.scratch.clear();
        let _ = write!(self.scratch, "{}  {label}", key_of(&self.keys, action));
        center(
            buf,
            area,
            y,
            &self.scratch,
            Style::new().fg(self.theme.label),
        );
    }

    fn render_ready(&mut self, buf: &mut Buffer, layout: &Panel, view: &View) {
        let best = view.scores.best();
        let inner = self.overlay(buf, layout, if best > 0 { 13 } else { 11 });
        let accent = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let value = Style::new()
            .fg(self.theme.value)
            .add_modifier(Modifier::BOLD);
        let label = Style::new().fg(self.theme.label);
        let mut y = inner.y;
        center(buf, inner, y, "BOSTRIS", accent);
        y += 2;
        let title = view.title;
        for (index, text) in ["Marathon", "Versus AI"].into_iter().enumerate() {
            let selected = title.selected == index;
            self.scratch.clear();
            self.scratch.push_str(if selected { "› " } else { "  " });
            self.scratch.push_str(text);
            if index == 1 {
                let _ = write!(self.scratch, "  ‹{:^4}›", title.level.name());
            } else {
                self.scratch.push_str("         ");
            }
            center(
                buf,
                inner,
                y,
                &self.scratch,
                if selected { value } else { label },
            );
            y += 1;
        }
        y += 1;
        if best > 0 {
            self.scratch.clear();
            self.scratch.push_str("Best ");
            write_grouped(&mut self.scratch, best);
            center(buf, inner, y, &self.scratch, label);
            y += 2;
        }
        center(buf, inner, y, "↑↓ Mode  ←→ Level", label);
        center(buf, inner, y + 1, "Enter  Start", value);
        self.render_key_line(buf, inner, y + 3, Action::Settings, "Settings");
        self.render_key_line(buf, inner, y + 4, Action::Quit, "Quit");
    }

    fn render_versus_over(
        &mut self,
        buf: &mut Buffer,
        layout: &Panel,
        player: &Game,
        versus: Versus,
        won: bool,
    ) {
        let inner = self.overlay(buf, layout, 12);
        let accent = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let warn = Style::new()
            .fg(self.theme.warn)
            .add_modifier(Modifier::BOLD);
        let value = Style::new().fg(self.theme.value);
        let label = Style::new().fg(self.theme.label);
        let mut y = inner.y;
        center(
            buf,
            inner,
            y,
            if won { "YOU WIN" } else { "YOU LOSE" },
            if won { accent } else { warn },
        );
        y += 1;
        self.scratch.clear();
        let _ = write!(self.scratch, "vs BOT {}", versus.level.name());
        center(buf, inner, y, &self.scratch, label);
        y += 2;
        center(buf, inner, y, "      YOU    BOT", label);
        y += 1;
        let (player_apm, player_pps) = rates(player);
        let (bot_apm, bot_pps) = rates(versus.opponent);
        let rows = [
            (
                "APM",
                format_args!("{player_apm:>6.1} {bot_apm:>6.1}").to_string(),
            ),
            (
                "PPS",
                format_args!("{player_pps:>6.2} {bot_pps:>6.2}").to_string(),
            ),
            (
                "Sent",
                format_args!(
                    "{:>6} {:>6}",
                    player.attack_total(),
                    versus.opponent.attack_total()
                )
                .to_string(),
            ),
        ];
        for (name, values) in rows {
            self.scratch.clear();
            let _ = write!(self.scratch, "{name:<4} {values}");
            center(buf, inner, y, &self.scratch, value);
            y += 1;
        }
        y += 1;
        self.render_key_line(buf, inner, y, Action::Restart, "Rematch");
        self.render_key_line(buf, inner, y + 1, Action::Pause, "Menu");
        self.render_key_line(buf, inner, y + 2, Action::Quit, "Quit");
    }

    fn render_paused(&mut self, buf: &mut Buffer, layout: &Panel) {
        let inner = self.overlay(buf, layout, 9);
        let title = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        center(buf, inner, inner.y + 1, "PAUSED", title);
        self.render_key_line(buf, inner, inner.y + 3, Action::Pause, "Resume");
        self.render_key_line(buf, inner, inner.y + 4, Action::Restart, "Restart");
        self.render_key_line(buf, inner, inner.y + 5, Action::Settings, "Settings");
        self.render_key_line(buf, inner, inner.y + 6, Action::Quit, "Quit");
        center(
            buf,
            inner,
            inner.y + 7,
            "Bksp  Menu",
            Style::new().fg(self.theme.label),
        );
    }

    fn render_game_over(
        &mut self,
        buf: &mut Buffer,
        layout: &Panel,
        view: &View,
        rank: Option<usize>,
    ) {
        let shown = view.scores.entries().len().min(5) as u16;
        let lines = 6 + if shown > 0 { shown + 2 } else { 0 } + 3;
        let inner = self.overlay(buf, layout, lines);
        let title = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let value = Style::new()
            .fg(self.theme.value)
            .add_modifier(Modifier::BOLD);
        let label = Style::new().fg(self.theme.label);
        let mut y = inner.y;
        center(buf, inner, y, "GAME OVER", title);
        y += 2;
        self.scratch.clear();
        write_grouped(&mut self.scratch, view.game.scoring().score);
        center(buf, inner, y, &self.scratch, value);
        y += 1;
        match rank {
            Some(0) => center(buf, inner, y, "NEW HIGH SCORE!", title),
            Some(r) => {
                self.scratch.clear();
                let _ = write!(self.scratch, "Rank #{}", r + 1);
                center(buf, inner, y, &self.scratch, label);
            }
            None => {}
        }
        y += 1;
        if let Some(accuracy) = view.game.finesse().accuracy() {
            self.scratch.clear();
            let _ = write!(self.scratch, "Finesse {accuracy:.1}%");
            center(buf, inner, y, &self.scratch, label);
        }
        y += 2;
        if shown > 0 && y + shown + 1 < inner.bottom() {
            center(buf, inner, y, "TOP SCORES", label);
            y += 1;
            for (i, entry) in view
                .scores
                .entries()
                .iter()
                .take(shown as usize)
                .enumerate()
            {
                self.scratch.clear();
                let _ = write!(self.scratch, "{}. ", i + 1);
                let prefix = self.scratch.len();
                write_grouped(&mut self.scratch, entry.score);
                let number_len = self.scratch.len() - prefix;
                let pad = 11usize.saturating_sub(number_len);
                self.scratch.insert_str(prefix, &"           "[..pad]);
                let style = if rank == Some(i) { value } else { label };
                center(buf, inner, y, &self.scratch, style);
                y += 1;
            }
            y += 1;
        }
        self.render_key_line(buf, inner, y, Action::Restart, "Play again");
        self.render_key_line(buf, inner, y + 1, Action::Pause, "Menu");
        self.render_key_line(buf, inner, y + 2, Action::Quit, "Quit");
    }

    fn render_settings(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        menu: &SettingsMenu,
        key_releases: bool,
    ) {
        let width = area.width.min(60);
        let height = area.height.min(ITEMS.len() as u16 + 6);
        let label = Style::new().fg(self.theme.label);
        if width < 36 || height < 10 {
            let y = area.y + area.height / 2;
            let title = Style::new()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD);
            center(buf, area, y.saturating_sub(1), "Terminal too small", title);
            center(buf, area, y, "Settings need at least 36×10", label);
            return;
        }
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        let block = self.theme.block(" SETTINGS ");
        let inner = block.inner(rect);
        block.render(rect, buf);

        let list_height = inner.height.saturating_sub(3) as usize;
        let cursor = menu.cursor();
        let offset = if ITEMS.len() <= list_height {
            0
        } else {
            cursor
                .saturating_sub(list_height / 2)
                .min(ITEMS.len() - list_height)
        };
        let value_style = Style::new()
            .fg(self.theme.value)
            .add_modifier(Modifier::BOLD);
        let accent = Style::new()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let warn = Style::new()
            .fg(self.theme.warn)
            .add_modifier(Modifier::BOLD);
        let capture = menu.capture();

        for (row, (index, item)) in ITEMS
            .iter()
            .enumerate()
            .skip(offset)
            .take(list_height)
            .enumerate()
        {
            let y = inner.y + row as u16;
            let selected = index == cursor;
            if selected {
                buf.set_style(
                    Rect::new(inner.x, y, inner.width, 1),
                    Style::new().bg(self.theme.highlight),
                );
                buf.set_string(inner.x + 1, y, "›", accent);
            }
            let left = inner.x + 3;
            let right = inner.right().saturating_sub(2);
            let (text, style) = match *item {
                Item::Header(title) => {
                    buf.set_string(inner.x + 1, y, title, accent);
                    continue;
                }
                Item::Field(field) => {
                    self.scratch.clear();
                    let adjustable = !field.is_toggle();
                    if selected && adjustable {
                        self.scratch.push_str("‹ ");
                    }
                    menu.write_value(field, &mut self.scratch);
                    if selected && adjustable {
                        self.scratch.push_str(" ›");
                    }
                    (field.label(), value_style)
                }
                Item::Key(action) => {
                    self.scratch.clear();
                    let capturing = matches!(
                        capture,
                        Some(Capture::Replace(a) | Capture::Add(a)) if a == action
                    );
                    if capturing {
                        self.scratch.push_str("Press a key…");
                        (action.label(), accent)
                    } else {
                        menu.write_keys(action, &mut self.scratch);
                        (action.label(), value_style)
                    }
                }
                Item::Reset => {
                    self.scratch.clear();
                    ("Reset all to defaults", label)
                }
                Item::Back => {
                    self.scratch.clear();
                    ("Save and return", label)
                }
            };
            let text_style = if selected {
                Style::new().fg(self.theme.value)
            } else {
                label
            };
            buf.set_string(left, y, text, text_style);
            let len = self.scratch.chars().count() as u16;
            buf.set_string(right.saturating_sub(len), y, &self.scratch, style);
        }

        if offset > 0 {
            buf.set_string(inner.right().saturating_sub(1), inner.y, "▲", label);
        }
        if offset + list_height < ITEMS.len() {
            let y = inner.y + list_height as u16 - 1;
            buf.set_string(inner.right().saturating_sub(1), y, "▼", label);
        }

        let footer = Rect::new(
            inner.x + 1,
            inner.bottom() - 2,
            inner.width.saturating_sub(2),
            2,
        );
        let item = menu.selected();
        let (message, message_style) = match (menu.status(), capture) {
            (_, Some(Capture::Add(action))) => {
                self.scratch.clear();
                let _ = write!(self.scratch, "Press a key to add to {}", action.label());
                (self.scratch.as_str(), accent)
            }
            (_, Some(Capture::Replace(action))) => {
                self.scratch.clear();
                let _ = write!(self.scratch, "Press a key to bind {}", action.label());
                (self.scratch.as_str(), accent)
            }
            (Some(status), None) => (status, warn),
            (None, None)
                if !key_releases
                    && matches!(item, Item::Field(Field::Das | Field::Arr | Field::SoftDrop)) =>
            {
                let message = if self.in_tmux {
                    "Inactive in tmux: run outside tmux for real key holds"
                } else {
                    "Inactive: this terminal doesn't report key releases"
                };
                (message, warn)
            }
            (None, None) => (
                match item {
                    Item::Field(field) => field.description(),
                    Item::Key(_) => "Enter replaces the keys, A adds another key",
                    Item::Reset => "Restore every setting and key to its default",
                    Item::Back => "Save changes to config.toml and go back",
                    Item::Header(_) => "",
                },
                label,
            ),
        };
        buf.set_stringn(
            footer.x,
            footer.y,
            message,
            footer.width as usize,
            message_style,
        );
        let hint = match (capture, item) {
            (Some(_), _) => "Any key binds it  ·  Ctrl+C quits",
            (None, Item::Key(_)) => "Enter Rebind  A Add  Del Default  Esc Back",
            (None, Item::Field(field)) if field.is_toggle() => {
                "↑↓ Select  Enter/←→ Toggle  Esc Save & back"
            }
            (None, Item::Field(_)) => "↑↓ Select  ←→ Adjust  Esc Save & back",
            (None, _) => "↑↓ Select  Enter Confirm  Esc Save & back",
        };
        buf.set_stringn(footer.x, footer.y + 1, hint, footer.width as usize, label);
    }
}

fn key_of(keys: &[String; Action::ALL.len()], action: Action) -> &str {
    let index = Action::ALL.iter().position(|&a| a == action).unwrap_or(0);
    &keys[index]
}

#[derive(Clone, Copy)]
enum Stat {
    Number(u64),
    Time(u64),
    Percent(Option<f32>),
    Decimal(f64, usize),
}

fn rates(game: &Game) -> (f64, f64) {
    let seconds = game.elapsed().as_secs_f64();
    if seconds < 1.0 {
        return (0.0, 0.0);
    }
    (
        f64::from(game.attack_total()) * 60.0 / seconds,
        f64::from(game.pieces_placed()) / seconds,
    )
}

fn center(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) {
    if y < area.top() || y >= area.bottom() {
        return;
    }
    let len = text.chars().count() as u16;
    let x = area.x + area.width.saturating_sub(len) / 2;
    buf.set_stringn(x, y, text, area.width as usize, style);
}

fn write_grouped(out: &mut String, value: u64) {
    let mut digits = [0u8; 20];
    let mut n = value;
    let mut len = 0;
    loop {
        digits[len] = (n % 10) as u8;
        n /= 10;
        len += 1;
        if n == 0 {
            break;
        }
    }
    for i in (0..len).rev() {
        out.push((b'0' + digits[i]) as char);
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bag::Rng;
    use crate::game::Settings;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn numbers_are_grouped() {
        let mut out = String::new();
        for (value, expected) in [
            (0, "0"),
            (999, "999"),
            (1000, "1,000"),
            (1234567, "1,234,567"),
        ] {
            out.clear();
            write_grouped(&mut out, value);
            assert_eq!(out, expected);
        }
    }

    #[test]
    fn layout_scales_with_terminal() {
        let (w, h) = min_size(false);
        assert!(fits(w, h, false));
        assert!(!fits(w - 1, h, false));
        assert!(!fits(w, h - 1, false));
        let big = compute_layout(Rect::new(0, 0, 200, 60), false).unwrap();
        assert_eq!(big.player.scale, 2);
        let layout = compute_layout(Rect::new(0, 0, w, h), false).unwrap();
        assert_eq!(layout.player.scale, 1);
        assert!(layout.help.is_none());
    }

    #[test]
    fn versus_layout_fits_two_boards() {
        let (w, h) = min_size(true);
        assert!(fits(w, h, true));
        assert!(!fits(w - 1, h, true));
        let compact = compute_layout(Rect::new(0, 0, w, h), true).unwrap();
        let opponent = compact.opponent.unwrap();
        assert_eq!(opponent.left.width, 0);
        assert!(opponent.board.x > compact.player.right.right());
        let full = compute_layout(Rect::new(0, 0, 100, 24), true).unwrap();
        assert!(full.opponent.unwrap().left.width > 0);
        assert!(full.player.board.right() <= 100);
    }

    #[test]
    fn renders_every_screen_without_panicking() {
        let game = Game::new(Settings::default(), Rng::new(5));
        let scores = HighScores::default();
        let mut renderer = Renderer::new(ColorMode::TrueColor, &Keymap::defaults());
        let menu = SettingsMenu::new(crate::config::Config::default());
        let opponent = Game::new(Settings::default(), Rng::new(6));
        for (w, h) in [
            (20, 5),
            (62, 22),
            (76, 23),
            (90, 30),
            (100, 24),
            (120, 50),
            (200, 50),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            for screen in [
                Screen::Ready,
                Screen::Playing,
                Screen::Paused,
                Screen::GameOver { rank: Some(0) },
                Screen::VersusOver { won: true },
                Screen::Settings,
            ] {
                for versus in [
                    None,
                    Some(Versus {
                        opponent: &opponent,
                        level: Level::APlus,
                    }),
                ] {
                    let view = View {
                        game: &game,
                        screen,
                        scores: &scores,
                        ghost: true,
                        preview_count: 5,
                        menu: Some(&menu),
                        key_releases: false,
                        versus,
                        title: Title {
                            selected: 1,
                            level: Level::APlus,
                        },
                    };
                    terminal
                        .draw(|frame| renderer.render(frame, &view))
                        .unwrap();
                }
            }
        }
    }
}
