//! The dashboard's cell grid. Panels draw theme *tokens*, never
//! colors: a `Canvas` is theme-free, so the headless goldens pin text
//! and style classes once, and the terminal presenter maps tokens to
//! the active theme's colors (SPEC §8.1).

/// A theme token (SPEC §8.1). `Heat(i)` indexes the theme's heat ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Bg,
    Panel,
    Zebra,
    Border,
    BorderFocus,
    Text,
    Dim,
    Faint,
    Accent,
    Accent2,
    Violet,
    Good,
    Warn,
    Bad,
    Info,
    User,
    Plateau,
    Bar,
    Rule,
    Keycap,
    KeyFg,
    Select,
    ChartFill,
    /// `info` blended 50% toward `panel`: the score chart's case-growth
    /// guides (SPEC §9 Score).
    InfoSoft,
    Heat(u8),
}

impl Tok {
    /// One letter per token for the golden style-class grid.
    pub fn letter(self) -> char {
        match self {
            Tok::Bg => '_',
            Tok::Panel => 'p',
            Tok::Zebra => 'z',
            Tok::Border => 'b',
            Tok::BorderFocus => 'B',
            Tok::Text => 't',
            Tok::Dim => 'd',
            Tok::Faint => 'f',
            Tok::Accent => 'A',
            Tok::Accent2 => 'M',
            Tok::Violet => 'V',
            Tok::Good => 'G',
            Tok::Warn => 'W',
            Tok::Bad => 'X',
            Tok::Info => 'I',
            Tok::User => 'U',
            Tok::Plateau => 'P',
            Tok::Bar => 'r',
            Tok::Rule => 'l',
            Tok::Keycap => 'k',
            Tok::KeyFg => 'K',
            Tok::Select => 's',
            Tok::ChartFill => 'c',
            Tok::InfoSoft => 'i',
            Tok::Heat(i) => char::from(b'0' + i.min(9)),
        }
    }
}

/// One cell: a single-width glyph, foreground/background tokens, bold,
/// and the modal-blend flag (the presenter blends a flagged cell 62%
/// toward `bg`, SPEC §7.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Tok,
    pub bg: Tok,
    pub bold: bool,
    pub veiled: bool,
}

impl Cell {
    pub const BLANK: Cell = Cell { ch: ' ', fg: Tok::Text, bg: Tok::Bg, bold: false, veiled: false };
}

/// A rectangle in cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    /// The content box: shrunk by 2 columns each side, 1 row top/bottom.
    pub fn content(&self) -> Rect {
        Rect::new(self.x + 2, self.y + 1, self.w.saturating_sub(4), self.h.saturating_sub(2))
    }
}

/// The theme-free cell grid every frame renders into.
#[derive(Clone, Debug)]
pub struct Canvas {
    pub w: u16,
    pub h: u16,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(w: u16, h: u16) -> Self {
        Canvas { w, h, cells: vec![Cell::BLANK; usize::from(w) * usize::from(h)] }
    }

    pub fn cell(&self, x: u16, y: u16) -> Cell {
        if x < self.w && y < self.h { self.cells[usize::from(y) * usize::from(self.w) + usize::from(x)] } else { Cell::BLANK }
    }

    fn slot(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x < self.w && y < self.h {
            let index = usize::from(y) * usize::from(self.w) + usize::from(x);
            Some(&mut self.cells[index])
        } else {
            None
        }
    }

    /// Put one glyph, keeping the cell's background.
    pub fn put(&mut self, x: u16, y: u16, ch: char, fg: Tok, bold: bool) {
        if let Some(cell) = self.slot(x, y) {
            cell.ch = ch;
            cell.fg = fg;
            cell.bold = bold;
        }
    }

    pub fn set_bg(&mut self, x: u16, y: u16, bg: Tok) {
        if let Some(cell) = self.slot(x, y) {
            cell.bg = bg;
        }
    }

    /// Fill a rectangle with blanks on `bg`.
    pub fn fill(&mut self, r: Rect, bg: Tok) {
        for y in r.y..r.y.saturating_add(r.h) {
            for x in r.x..r.x.saturating_add(r.w) {
                if let Some(cell) = self.slot(x, y) {
                    *cell = Cell { bg, ..Cell::BLANK };
                }
            }
        }
    }

    /// Recolor a rectangle's background only.
    pub fn tint(&mut self, r: Rect, bg: Tok) {
        for y in r.y..r.y.saturating_add(r.h) {
            for x in r.x..r.x.saturating_add(r.w) {
                self.set_bg(x, y, bg);
            }
        }
    }

    /// Draw text clipped to `[x, x+max)`; returns the columns used.
    pub fn text(&mut self, x: u16, y: u16, max: u16, text: &str, fg: Tok, bold: bool) -> u16 {
        let mut used = 0u16;
        for ch in text.chars() {
            if used >= max {
                break;
            }
            self.put(x + used, y, ch, fg, bold);
            used += 1;
        }
        used
    }

    /// A rounded box outline (SPEC §7.5).
    pub fn frame(&mut self, r: Rect, border: Tok) {
        if r.w < 2 || r.h < 2 {
            return;
        }
        let (x1, y1) = (r.x + r.w - 1, r.y + r.h - 1);
        for x in r.x + 1..x1 {
            self.put(x, r.y, '─', border, false);
            self.put(x, y1, '─', border, false);
        }
        for y in r.y + 1..y1 {
            self.put(r.x, y, '│', border, false);
            self.put(x1, y, '│', border, false);
        }
        self.put(r.x, r.y, '╭', border, false);
        self.put(x1, r.y, '╮', border, false);
        self.put(r.x, y1, '╰', border, false);
        self.put(x1, y1, '╯', border, false);
    }

    /// Flag every cell for the modal blend.
    pub fn veil(&mut self) {
        for cell in &mut self.cells {
            cell.veiled = true;
        }
    }

    /// The text grid, one line per row, trailing blanks trimmed.
    pub fn dump_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.h {
            let line: String = (0..self.w).map(|x| self.cell(x, y).ch).collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    /// The style-class grid: the foreground token letter of every
    /// non-blank cell, `.` for blanks; trailing blanks trimmed.
    pub fn dump_style(&self) -> String {
        let mut out = String::new();
        for y in 0..self.h {
            let line: String = (0..self.w)
                .map(|x| {
                    let cell = self.cell(x, y);
                    if cell.ch == ' ' { '.' } else { cell.fg.letter() }
                })
                .collect();
            out.push_str(line.trim_end_matches('.'));
            out.push('\n');
        }
        out
    }
}

/// Theme names, in `t` cycle order.
pub const THEMES: [&str; 3] = ["aurora", "nordic", "paper"];

fn hex(value: u32) -> (u8, u8, u8) {
    (((value >> 16) & 0xff) as u8, ((value >> 8) & 0xff) as u8, (value & 0xff) as u8)
}

const AURORA: [u32; 23] = [
    0x060a12, 0x0a101d, 0x0d1424, 0x1c2642, 0x2de2e6, 0xd6deee, 0x6f7d9c, 0x2d3856, 0x2de2e6, 0xff4fd8, 0x9d7bff, 0x3ee58f,
    0xffc94d, 0xff5c7a, 0x5aa9ff, 0xff9f43, 0x56627f, 0x080d18, 0x172039, 0x141d35, 0x2de2e6, 0x12243f, 0x0c2231,
];
const NORDIC: [u32; 23] = [
    0x1b1f27, 0x222731, 0x262c37, 0x353c4a, 0x88c0d0, 0xd8dee9, 0x7d8799, 0x3d4555, 0x88c0d0, 0xb48ead, 0xb48ead, 0xa3be8c,
    0xebcb8b, 0xd0707a, 0x81a1c1, 0xd08770, 0x4c566a, 0x171a21, 0x2c323e, 0x2b313d, 0x88c0d0, 0x2e3a4c, 0x26303d,
];
const PAPER: [u32; 23] = [
    0xf3ede0, 0xfbf8ef, 0xf5efe2, 0xdccfb2, 0xb4561b, 0x2f2a24, 0x80766a, 0xd3c6aa, 0xb4561b, 0x2b6f8f, 0x7a4b8c, 0x4d7d1f,
    0xa8760a, 0xb3261e, 0x2b6f8f, 0x8a3fa0, 0xa89b82, 0xe9e1cd, 0xd8cbae, 0xe2d8c0, 0xb4561b, 0xf0e2c4, 0xf1e4c6,
];
const HEAT: [&[u32]; 3] = [
    &[0x1a1030, 0x40206a, 0x76308e, 0xb53d7f, 0xe5596a, 0xf88a4f, 0xffc46b],
    &[0x2a303c, 0x2e4454, 0x355f6f, 0x467f8f, 0x5f9fae, 0x7fbac7, 0x9fd0da],
    &[0xefe6cf, 0xe8d19f, 0xdca763, 0xc6773a, 0x9c4a1c, 0x5e2a0e],
];

/// The RGB of a token under theme `theme` (index into [`THEMES`]).
pub fn color(theme: usize, tok: Tok) -> (u8, u8, u8) {
    let table = match theme % 3 {
        0 => &AURORA,
        1 => &NORDIC,
        _ => &PAPER,
    };
    let index = match tok {
        Tok::InfoSoft => return blend(color(theme, Tok::Info), color(theme, Tok::Panel), 0.5),
        Tok::Heat(i) => {
            let ramp = HEAT[theme % 3];
            return hex(ramp[usize::from(i).min(ramp.len() - 1)]);
        }
        Tok::Bg => 0,
        Tok::Panel => 1,
        Tok::Zebra => 2,
        Tok::Border => 3,
        Tok::BorderFocus => 4,
        Tok::Text => 5,
        Tok::Dim => 6,
        Tok::Faint => 7,
        Tok::Accent => 8,
        Tok::Accent2 => 9,
        Tok::Violet => 10,
        Tok::Good => 11,
        Tok::Warn => 12,
        Tok::Bad => 13,
        Tok::Info => 14,
        Tok::User => 15,
        Tok::Plateau => 16,
        Tok::Bar => 17,
        Tok::Rule => 18,
        Tok::Keycap => 19,
        Tok::KeyFg => 20,
        Tok::Select => 21,
        Tok::ChartFill => 22,
    };
    hex(table[index])
}

/// ASCII stand-ins (SPEC §8.3): the SPEC's list first, then every other
/// non-ASCII glyph the dashboard draws, so a terminal that measures any
/// of them wider than one cell (East Asian ambiguous width) or runs in
/// ASCII glyph mode still keeps every column in place.
const FALLBACKS: &[(char, char)] = &[
    ('⤢', '+'), ('✕', 'x'), ('✎', '*'), ('✗', 'x'), ('⊘', '!'), ('◌', 'o'), ('◆', '#'), ('◇', '%'), ('❚', '|'), ('▲', '^'),
    ('┃', '|'),
    ('▼', 'v'), ('●', '*'), ('○', 'o'), ('■', '#'), ('▶', '>'), ('▸', '>'), ('◉', '@'), ('✓', 'v'), ('⋯', '.'), ('…', '.'),
    ('·', '.'), ('×', 'x'), ('Δ', 'd'), ('–', '-'), ('—', '-'), ('−', '-'), ('›', '>'), ('←', '<'), ('→', '>'), ('↑', '^'),
    ('↓', 'v'), ('⇒', '>'), ('⇥', '>'), ('⇧', '^'), ('≈', '~'), ('⏎', '<'), ('§', 'S'),
    ('─', '-'), ('━', '='), ('│', '|'), ('┆', ':'), ('┈', '-'), ('└', '+'), ('├', '+'), ('┤', '+'), ('╭', '+'), ('╮', '+'),
    ('╯', '+'), ('╰', '+'),
    ('▁', '_'), ('▂', '_'), ('▃', '-'), ('▄', '-'), ('▅', '='), ('▆', '='), ('▇', '#'), ('█', '#'), ('▏', '|'), ('▕', '|'),
];

/// The glyph substitutions for one terminal (SPEC §8.3). The presenter
/// builds it once from FrankenTUI's glyph policy and width tables and
/// maps every painted cell through it; the canvas and the goldens stay
/// in the canonical glyphs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Glyphs {
    swaps: Vec<(char, char)>,
    braille: bool,
}

impl Glyphs {
    /// Each glyph whose `width` is not 1 (every glyph when `ascii`)
    /// falls back. Braille chart cells fall back to `.` together.
    pub fn measured(ascii: bool, width: impl Fn(char) -> usize) -> Self {
        let swaps = FALLBACKS.iter().copied().filter(|(glyph, _)| ascii || width(*glyph) != 1).collect();
        Glyphs { swaps, braille: ascii || width('⣿') != 1 }
    }

    pub fn map(&self, ch: char) -> char {
        if self.braille && ('\u{2800}'..='\u{28ff}').contains(&ch) {
            return if ch == '\u{2800}' { ' ' } else { '.' };
        }
        self.swaps.iter().find(|(glyph, _)| *glyph == ch).map_or(ch, |(_, ascii)| *ascii)
    }
}

/// Linear blend of `a` toward `b` by `t` in [0, 1].
pub fn blend(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}
