//! The eleven panels (SPEC §9). Each draws inside its content box
//! through a clipping `Pen`, so no panel can write into a border,
//! gutter or margin (SPEC §15 A10).

use std::collections::BTreeMap;

use super::canvas::{Canvas, Rect, Tok};
use super::model::{approx, dec4, fam_move, rat_f64, rat_text, units, verdict_label, verdict_tok, Mark, Rat, Rec, RecState, RunModel};
use super::render::{selected_key, Hit, HitAct};
use super::view::{Ctx, PanelId, View};

/// A clipping pen over one content rectangle (coordinates relative).
pub struct Pen<'a> {
    c: &'a mut Canvas,
    r: Rect,
}

impl<'a> Pen<'a> {
    pub fn new(c: &'a mut Canvas, r: Rect) -> Self {
        Pen { c, r }
    }
    pub fn w(&self) -> u16 {
        self.r.w
    }
    pub fn h(&self) -> u16 {
        self.r.h
    }
    pub fn abs(&self, x: u16, y: u16) -> (u16, u16) {
        (self.r.x + x, self.r.y + y)
    }
    pub fn put(&mut self, x: u16, y: u16, ch: char, fg: Tok, bold: bool) {
        if x < self.r.w && y < self.r.h {
            self.c.put(self.r.x + x, self.r.y + y, ch, fg, bold);
        }
    }
    pub fn text(&mut self, x: u16, y: u16, s: &str, fg: Tok, bold: bool) -> u16 {
        if x >= self.r.w || y >= self.r.h {
            return 0;
        }
        // Text that runs into the content edge ends in `…`, never a
        // silent cut (SPEC §7.5).
        let room = self.r.w - x;
        let shown = if s.chars().count() > usize::from(room) { fit(s, room) } else { s.to_string() };
        self.c.text(self.r.x + x, self.r.y + y, room, &shown, fg, bold)
    }
    /// Draw styled parts left to right; returns the end column.
    pub fn spans(&mut self, x: u16, y: u16, parts: &[(String, Tok, bool)]) -> u16 {
        let mut at = x;
        for (text, tok, bold) in parts {
            at += self.text(at, y, text, *tok, *bold);
        }
        at
    }
    /// Right-aligned text ending at the content edge.
    pub fn right(&mut self, y: u16, s: &str, fg: Tok, bold: bool) {
        let len = s.chars().count() as u16;
        if len <= self.r.w {
            self.text(self.r.w - len, y, s, fg, bold);
        }
    }
    pub fn bg(&mut self, x: u16, y: u16, w: u16, bg: Tok) {
        if y >= self.r.h {
            return;
        }
        for dx in x..x.saturating_add(w).min(self.r.w) {
            self.c.set_bg(self.r.x + dx, self.r.y + y, bg);
        }
    }
    pub fn row_rect(&self, y: u16) -> Rect {
        Rect::new(self.r.x, self.r.y + y, self.r.w, 1)
    }
}

/// Fit `s` into `max` cells, truncating with `…`.
pub fn fit(s: &str, max: u16) -> String {
    let max = usize::from(max);
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

fn pad(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width { s.chars().take(width).collect() } else { format!("{s}{}", " ".repeat(width - n)) }
}

fn lpad(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width { s.chars().take(width).collect() } else { format!("{}{s}", " ".repeat(width - n)) }
}

/// Exact or probe rendering of a record's score (SPEC §8.2).
fn score_cells(rec: &Rec) -> (String, String, Tok) {
    match rec.score() {
        Some(s) if rec.tier() == 0 => ("-".into(), approx(s), Tok::Dim),
        Some(s) => (rat_text(s), dec4(s), Tok::Text),
        None => ("-".into(), "-".into(), Tok::Dim),
    }
}

/// Draw one panel's content.
#[allow(clippy::too_many_arguments)]
pub fn draw(pen: &mut Pen<'_>, hits: &mut Vec<Hit>, model: &RunModel, view: &View, ctx: &Ctx, panel: PanelId, expanded: bool) {
    if !model.started {
        pen.text(0, 0, "Waiting for the driver's first journal line.", Tok::Dim, false);
        return;
    }
    match panel {
        PanelId::Score => score(pen, model, expanded),
        PanelId::Budget => budget(pen, model, expanded),
        PanelId::Frontier => frontier(pen, hits, model, view, expanded),
        PanelId::Heat => heat(pen, hits, model, view, expanded),
        PanelId::Verdicts => verdicts(pen, model, expanded),
        PanelId::Families => families(pen, model, expanded),
        PanelId::Events => events(pen, model, view, ctx),
        PanelId::Lineage => lineage(pen, model, view, expanded),
        PanelId::Refuted => refuted(pen, model),
        PanelId::Evidence => evidence(pen, model, ctx),
        PanelId::Journal => journal(pen, model, view),
    }
}

const BRAILLE_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// A braille dot grid over `w × h` cells.
struct Dots {
    w: u16,
    h: u16,
    bits: Vec<u8>,
}

impl Dots {
    fn new(w: u16, h: u16) -> Self {
        Dots { w, h, bits: vec![0; usize::from(w) * usize::from(h)] }
    }
    fn set(&mut self, dx: usize, dy: usize) {
        let (cx, cy) = (dx / 2, dy / 4);
        if cx < usize::from(self.w) && cy < usize::from(self.h) {
            self.bits[cy * usize::from(self.w) + cx] |= BRAILLE_BITS[dx % 2][dy % 4];
        }
    }
    fn cell(&self, cx: u16, cy: u16) -> Option<char> {
        let b = self.bits[usize::from(cy) * usize::from(self.w) + usize::from(cx)];
        if b == 0 { None } else { char::from_u32(0x2800 + u32::from(b)) }
    }
}

/// A step series plotted as braille, with the area below filled.
struct Plot {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    lo: f64,
    hi: f64,
}

impl Plot {
    fn dot_y(&self, v: f64) -> usize {
        let rows = f64::from(self.h) * 4.0 - 1.0;
        let t = if self.hi > self.lo { (self.hi - v) / (self.hi - self.lo) } else { 0.5 };
        (t.clamp(0.0, 1.0) * rows).round() as usize
    }
    fn draw(&self, pen: &mut Pen<'_>, series: &[f64], tok: Tok, fill: bool) {
        if series.is_empty() || self.w == 0 || self.h == 0 {
            return;
        }
        let mut dots = Dots::new(self.w, self.h);
        let dw = usize::from(self.w) * 2;
        let mut prev: Option<usize> = None;
        let mut line_row = vec![usize::MAX; usize::from(self.w)];
        for dx in 0..dw {
            let index = (dx * series.len() / dw).min(series.len() - 1);
            let dy = self.dot_y(series[index]);
            let (a, b) = match prev {
                Some(p) => (p.min(dy), p.max(dy)),
                None => (dy, dy),
            };
            for y in a..=b {
                dots.set(dx, y);
            }
            let cx = dx / 2;
            line_row[cx] = line_row[cx].min(dy / 4);
            prev = Some(dy);
        }
        for cx in 0..self.w {
            for cy in 0..self.h {
                if let Some(ch) = dots.cell(cx, cy) {
                    pen.put(self.x + cx, self.y + cy, ch, tok, false);
                } else if fill && usize::from(cy) > line_row[usize::from(cx)] {
                    pen.bg(self.x + cx, self.y + cy, 1, Tok::ChartFill);
                }
            }
        }
    }
}

fn score_series(model: &RunModel) -> Vec<(u64, Option<Rat>)> {
    let mut series = vec![(0, model.seed_score)];
    series.extend(model.batches.iter().map(|b| (b.batch, b.score)));
    series
}

fn score(pen: &mut Pen<'_>, model: &RunModel, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let card_w: u16 = if w >= 84 { if expanded { 44 } else { 27 } } else { 0 };
    let chart_w = if card_w > 0 { w - card_w - 3 } else { w };
    let series = score_series(model);
    let values: Vec<f64> = series.iter().map(|(_, s)| s.map_or(0.0, rat_f64)).collect();
    let (mut lo, mut hi) = values.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    if lo > hi {
        (lo, hi) = (0.0, 1.0);
    }
    // Ticks label the data's own extremes; the plot range adds 8%
    // headroom so the line never rides the panel edge.
    let (tick_lo, tick_hi);
    if (hi - lo).abs() < 1e-12 {
        if hi.abs() > 1e-9 {
            let pad = hi.abs() * 0.1;
            lo -= pad;
            hi += pad;
        } else {
            (lo, hi) = (0.0, 1.0);
        }
        (tick_lo, tick_hi) = (lo, hi);
    } else {
        (tick_lo, tick_hi) = (lo, hi);
        let pad = (hi - lo) * 0.08;
        lo -= pad;
        hi += pad;
    }
    let chart_h = h.saturating_sub(2);
    // Each tick sits on the row its value plots to, so a line at a
    // tick value runs along that tick's grid row.
    let ticks: u32 = if expanded { 5 } else { 3 };
    let probe = Plot { x: 0, y: 1, w: 1, h: chart_h, lo, hi };
    let marks: Vec<(u16, String)> = (0..ticks)
        .map(|i| {
            let v = tick_hi - f64::from(i) / f64::from(ticks - 1) * (tick_hi - tick_lo);
            let v = if (v * 100.0).round() == 0.0 { 0.0 } else { v };
            ((probe.dot_y(v) / 4) as u16, format!("{v:.2}"))
        })
        .collect();
    let lw = marks.iter().map(|(_, l)| l.chars().count()).max().unwrap_or(4) as u16 + 2;
    let plot = Plot { x: lw, y: 1, w: chart_w.saturating_sub(lw), h: chart_h, lo, hi };
    for (row, label) in &marks {
        let row = *row;
        pen.text(0, 1 + row, &lpad(label, usize::from(lw - 2)), Tok::Dim, false);
        pen.put(lw - 1, 1 + row, '┤', Tok::Faint, false);
        for x in 0..plot.w {
            pen.put(lw + x, 1 + row, '┈', Tok::Faint, false);
        }
    }
    let n = series.len().max(1);
    let col_of = |batch: u64| -> u16 {
        let index = series.binary_search_by_key(&batch, |s| s.0).unwrap_or(0);
        (index * usize::from(plot.w) / n).min(usize::from(plot.w.saturating_sub(1))) as u16
    };
    for (batch, _, _) in &model.growths {
        let x = lw + col_of(*batch);
        for y in 0..chart_h {
            pen.put(x, 1 + y, '┆', Tok::InfoSoft, false);
        }
    }
    plot.draw(pen, &values, Tok::Accent, true);
    pen.text(0, 0, "events", Tok::Dim, false);
    let mut rail: BTreeMap<u16, (char, Tok)> = BTreeMap::new();
    for (batch, mark) in model.marks.iter().filter(|(_, m)| *m != Mark::Promoted) {
        rail.insert(lw + col_of(*batch), mark.glyph());
    }
    for (batch, mark) in model.marks.iter().filter(|(_, m)| *m == Mark::Promoted) {
        rail.entry(lw + col_of(*batch)).or_insert(mark.glyph());
    }
    for (x, (ch, tok)) in rail {
        if x < chart_w {
            pen.put(x, 0, ch, tok, false);
        }
    }
    let first = format!("b{}", series.first().map_or(0, |s| s.0));
    let last = format!("b{}", series.last().map_or(0, |s| s.0));
    let bottom = h.saturating_sub(1);
    pen.text(lw, bottom, &first, Tok::Dim, false);
    let lx = chart_w.saturating_sub(last.chars().count() as u16);
    pen.text(lx, bottom, &last, Tok::Dim, false);
    let legend = "▲ promoted  ◆ cases  ✗ refuted  $ budget  ✎ you";
    let len = legend.chars().count() as u16;
    let room = chart_w.saturating_sub(lw);
    if len + 16 < room {
        pen.text(lw + (room - len) / 2, bottom, legend, Tok::Dim, false);
    }
    if card_w > 0 {
        for y in 0..h {
            pen.put(chart_w + 1, y, '│', Tok::Border, false);
        }
        incumbent_card(pen, model, chart_w + 3, card_w, expanded);
    }
}

fn incumbent_card(pen: &mut Pen<'_>, model: &RunModel, x: u16, w: u16, expanded: bool) {
    let Some(rec) = model.incumbent_rec() else {
        pen.text(x, 0, "INCUMBENT  -", Tok::Dim, true);
        return;
    };
    let key = format!("#{}", rec.key);
    pen.text(x, 0, "INCUMBENT", Tok::Dim, true);
    pen.text(x + w.saturating_sub(key.chars().count() as u16), 0, &key, Tok::Accent, true);
    match rec.score() {
        Some(s) if rec.tier() == 0 => {
            pen.text(x, 1, &approx(s), Tok::Dim, false);
        }
        Some(s) => {
            let big = dec4(s);
            let at = x + pen.text(x, 1, &big, Tok::Good, true);
            pen.text(at, 1, &fit(&format!("  {}", rat_text(s)), w.saturating_sub(big.chars().count() as u16)), Tok::Dim, false);
        }
        None => {
            pen.text(x, 1, "-", Tok::Dim, false);
        }
    }
    let promo = model.promotions.iter().rev().find(|p| p.key == rec.key);
    match promo.and_then(|p| p.gain) {
        Some(g) => pen.text(x, 2, &super::model::gain_text(g), Tok::Good, false),
        None => pen.text(x, 2, "seed incumbent", Tok::Dim, false),
    };
    let since = promo.map_or(0, |p| p.batch);
    let depth = lineage_chain(model, rec.key).len().saturating_sub(1);
    let close = model.plateau_close;
    let streak = u64::from(model.plateau_streak()).min(close);
    let plateau = if close <= 8 { format!("{}  {streak} of {close}", model.plateau_dots()) } else { format!("{streak} of {close}") };
    let rows: Vec<(&str, String, Tok)> = vec![
        ("value", rec.value_text(), Tok::Text),
        ("tier", if rec.tier() == 1 { "✓ exact".into() } else { "~ probe".into() }, if rec.tier() == 1 { Tok::Good } else { Tok::Dim }),
        ("move", fam_move(rec), Tok::Text),
        ("parent", rec.parent().map_or_else(|| "seed".to_string(), |p| format!("#{p}")), Tok::Text),
        ("held", format!("since b{since}  {} ago", model.last_batch().saturating_sub(since)), Tok::Text),
        ("depth", depth.to_string(), Tok::Text),
        ("plateau", plateau, Tok::Warn),
    ];
    let mut y = 4;
    for (label, value, tok) in rows {
        if y >= pen.h() {
            return;
        }
        pen.text(x, y, label, Tok::Dim, false);
        pen.text(x + 9, y, &fit(&value, w.saturating_sub(9)), tok, false);
        y += 1;
    }
    if expanded && y + 2 < pen.h() {
        y += 1;
        pen.text(x, y, "PROMOTIONS", Tok::Dim, true);
        y += 1;
        for p in model.promotions.iter().rev() {
            if y >= pen.h() {
                break;
            }
            let score = p.score.map_or_else(|| "-".to_string(), dec4);
            let gain = p.gain.map_or_else(String::new, super::model::signed4);
            let fm = model.records.get(&p.key).map(fam_move).unwrap_or_default();
            let line = format!("b{:<4} #{:<6} {:<8} {:<8} {fm}", p.batch, p.key, score, gain);
            pen.text(x, y, &fit(&line, w), Tok::Text, false);
            y += 1;
        }
    }
}

fn budget(pen: &mut Pen<'_>, model: &RunModel, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let used = model.used();
    let wm = model.watermark().max(1);
    let ratio = (used.max(0) as f64 / wm as f64).clamp(0.0, 1.0);
    let pct = format!("{}%", (ratio * 100.0).round() as i64);
    let mut x = pen.spans(0, 0, &[
        ("used ".into(), Tok::Dim, false),
        (units(used), Tok::Text, true),
        ("  of  ".into(), Tok::Dim, false),
        (units(wm), Tok::Text, false),
        (" u".into(), Tok::Dim, false),
    ]);
    x += 2;
    let gauge_w = w.saturating_sub(x + pct.chars().count() as u16 + 2);
    let lit = (ratio * f64::from(gauge_w)).round() as u16;
    let tok = if ratio >= 0.85 { Tok::Warn } else { Tok::Accent };
    for i in 0..gauge_w {
        pen.put(x + i, 0, '━', if i < lit { tok } else { Tok::Faint }, false);
    }
    pen.right(0, &pct, Tok::Text, false);
    let ladder_w = w.min(72);
    let ladder_x = if expanded && w > 110 { w - ladder_w } else { 0 };
    if expanded && w > 110 {
        used_chart(pen, model, ladder_x.saturating_sub(3), h);
    }
    let rows_avail = h.saturating_sub(5);
    let bar_w = ladder_w.saturating_sub(22).max(4);
    pen.text(ladder_x, 2, "raise", Tok::Dim, false);
    pen.text(ladder_x + 7, 2, "watermark · log scale", Tok::Dim, false);
    pen.text(ladder_x + 7 + bar_w + 1, 2, &lpad("×", 5), Tok::Dim, false);
    pen.text(ladder_x + ladder_w.saturating_sub(8), 2, &lpad("units", 8), Tok::Dim, false);
    let max_to = model.rungs.iter().map(|r| r.1).max().unwrap_or(1).max(2);
    let shown: Vec<(usize, &(u64, i128))> = model.rungs.iter().enumerate().collect();
    let skip = shown.len().saturating_sub(usize::from(rows_avail));
    let last_index = model.rungs.len().saturating_sub(1);
    for (row, (index, (batch, to))) in shown.into_iter().skip(skip).enumerate() {
        let y = 3 + row as u16;
        let current = index == last_index;
        let label = if index == 0 { "seed".to_string() } else { format!("b{batch}") };
        pen.text(ladder_x, y, &lpad(&label, 5), if current { Tok::Text } else { Tok::Dim }, false);
        pen.put(ladder_x + 6, y, '▕', Tok::Faint, false);
        let len = (((*to).max(2) as f64).ln() / (max_to as f64).ln() * f64::from(bar_w)).round().clamp(1.0, f64::from(bar_w)) as u16;
        let lit = if current { (ratio * f64::from(len)).round() as u16 } else { len };
        let lit_tok = if !current { Tok::Plateau } else if ratio > 0.85 { Tok::Warn } else { Tok::Accent };
        for i in 0..len {
            pen.put(ladder_x + 7 + i, y, '█', if i < lit { lit_tok } else { Tok::ChartFill }, false);
        }
        if index > 0 {
            let prev = model.rungs[index - 1].1.max(1);
            let r = format!("×{:.1}", *to as f64 / prev as f64);
            pen.text(ladder_x + 7 + bar_w + 1, y, &lpad(&r, 5), Tok::Dim, false);
        }
        let u = lpad(&units(*to), 8);
        pen.text(ladder_x + ladder_w.saturating_sub(8), y, &u, if current { Tok::Accent } else { Tok::Text }, current);
    }
    if h >= 4 {
        let spend_y = h - 2;
        pen.text(ladder_x, spend_y, "spend / batch", Tok::Dim, false);
        let last_delta = model.batches.last().map_or(0, |b| b.delta);
        let delta = format!("Δ {} u", units(last_delta));
        let spark_x = ladder_x + 15;
        let spark_w = ladder_w.saturating_sub(15 + delta.chars().count() as u16 + 2);
        let deltas: Vec<i128> = model.batches.iter().rev().take(usize::from(spark_w)).map(|b| b.delta).collect::<Vec<_>>().into_iter().rev().collect();
        let top = deltas.iter().copied().max().unwrap_or(1).max(1);
        const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        for (i, d) in deltas.iter().enumerate() {
            let level = ((*d).max(0) as f64 / top as f64 * 7.0).round() as usize;
            pen.put(spark_x + i as u16, spend_y, BLOCKS[level.min(7)], Tok::Info, false);
        }
        pen.text(ladder_x + ladder_w.saturating_sub(delta.chars().count() as u16), spend_y, &delta, Tok::Text, false);
        pen.spans(ladder_x, h - 1, &[
            ("charged ".into(), Tok::Dim, false),
            (format!("{} u", units(used)), Tok::Text, false),
            ("   computed -   cache -".into(), Tok::Dim, false),
        ]);
    }
}

fn used_chart(pen: &mut Pen<'_>, model: &RunModel, w: u16, h: u16) {
    pen.text(0, 2, "USED vs WATERMARK", Tok::Dim, true);
    let note = [
        "A budget-exhausted batch commits partial work; the driver",
        "resumes at 2·used + step. Doubling outruns the per-batch",
        "refresh cost, so the ladder never locksteps.",
    ];
    let chart_h = h.saturating_sub(4 + note.len() as u16 + 1);
    let used: Vec<f64> = model.batches.iter().map(|b| b.used as f64).collect();
    let marks: Vec<f64> = model.batches.iter().map(|b| b.watermark as f64).collect();
    let hi = marks.iter().chain(used.iter()).fold(1.0f64, |a, v| a.max(*v)) * 1.05;
    let plot = Plot { x: 0, y: 3, w, h: chart_h, lo: 0.0, hi };
    if !marks.is_empty() {
        for cx in 0..w {
            let index = (usize::from(cx) * marks.len() / usize::from(w.max(1))).min(marks.len() - 1);
            let row = (plot.dot_y(marks[index]) / 4) as u16;
            pen.put(cx, 3 + row, '─', Tok::Warn, false);
        }
    }
    plot.draw(pen, &used, Tok::Accent, true);
    for (i, line) in note.iter().enumerate() {
        pen.text(0, 3 + chart_h + 1 + i as u16, &fit(line, w), Tok::Dim, false);
    }
}

struct Column {
    head: &'static str,
    width: usize,
    right: bool,
}

fn frontier(pen: &mut Pen<'_>, hits: &mut Vec<Hit>, model: &RunModel, view: &View, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let mut cols = vec![
        Column { head: "", width: 1, right: false },
        Column { head: "key", width: 7, right: false },
        Column { head: "value", width: 9, right: false },
        Column { head: "score", width: 11, right: true },
        Column { head: "decimal", width: 9, right: true },
        Column { head: "tier", width: 8, right: false },
        Column { head: "family", width: 6, right: false },
        Column { head: "move", width: 5, right: false },
        Column { head: "parent", width: 7, right: false },
        Column { head: "state", width: 16, right: false },
    ];
    if expanded {
        cols.extend([
            Column { head: "born", width: 6, right: false },
            Column { head: "children", width: 8, right: true },
            Column { head: "bound", width: 11, right: true },
            Column { head: "witness", width: 8, right: true },
        ]);
    }
    // Narrow panels shed detail columns first; key, score and state stay.
    let mut keep: Vec<usize> = (0..cols.len()).collect();
    let width = |keep: &[usize]| keep.iter().map(|i| cols[*i].width + 1).sum::<usize>();
    for head in ["witness", "bound", "children", "born", "parent", "move", "family", "decimal", "tier", "value"] {
        if width(&keep) <= usize::from(w) + 1 {
            break;
        }
        keep.retain(|i| cols[*i].head != head);
    }
    while keep.len() > 3 && width(&keep) > usize::from(w) + 1 {
        keep.pop();
    }
    let mut x = 0u16;
    for col in keep.iter().map(|i| &cols[*i]) {
        let head = if col.right { lpad(col.head, col.width) } else { pad(col.head, col.width) };
        pen.text(x, 0, &head, Tok::Dim, false);
        x += col.width as u16 + 1;
    }
    let rows = usize::from(h.saturating_sub(1));
    let selected = selected_key(view, model);
    let list = model.record_list(rows, expanded);
    if list.is_empty() {
        pen.text(0, 1, "No records yet.", Tok::Dim, false);
        return;
    }
    let sel_index = list.iter().position(|r| Some(r.key) == selected).unwrap_or(0);
    let start = if list.len() > rows { sel_index.saturating_sub(rows.saturating_sub(1)).min(list.len() - rows) } else { 0 };
    for (row, rec) in list.iter().skip(start).take(rows).enumerate() {
        let y = 1 + row as u16;
        let state = model.state_of(rec);
        let is_selected = Some(rec.key) == selected;
        if is_selected {
            pen.bg(0, y, w, Tok::Select);
        } else if row % 2 == 1 {
            pen.bg(0, y, w, Tok::Zebra);
        }
        let muted = matches!(state, RecState::Valley | RecState::Refuted | RecState::Probed);
        let base = if muted { Tok::Dim } else { Tok::Text };
        let (score_text, decimal, score_tok) = score_cells(rec);
        let children = model.children(rec.key).len();
        let cells: Vec<(String, Tok, bool)> = vec![
            (if is_selected { "▶".into() } else { " ".into() }, Tok::Accent, true),
            (format!("#{}", rec.key), if state == RecState::Incumbent { Tok::Accent } else { base }, state == RecState::Incumbent),
            (rec.value_text(), base, false),
            (score_text, if muted { Tok::Dim } else { score_tok }, false),
            (decimal, if muted { Tok::Dim } else { score_tok }, false),
            (if rec.tier() == 1 { "✓ exact".into() } else { "~ probe".into() }, if rec.tier() == 1 && !muted { Tok::Good } else { Tok::Dim }, false),
            (rec.int("family").filter(|f| *f >= 0).map_or_else(|| "-".into(), |f| format!("f{f}")), base, false),
            (rec.int("move").filter(|m| *m >= 0).map_or_else(|| "-".into(), |m| format!("m{m}")), base, false),
            (rec.parent().map_or_else(|| "seed".into(), |p| format!("#{p}")), base, false),
            (state_text(rec, state), state.tok(), state == RecState::Incumbent),
            (format!("b{}", rec.born), Tok::Dim, false),
            (children.to_string(), base, false),
            (rec.rat("bound").map_or_else(|| "-".into(), rat_text), base, false),
            (rec.int("witness").filter(|w| *w >= 0).map_or_else(|| "-".into(), |w| format!("c{w}")), base, false),
        ];
        let mut x = 0u16;
        for i in &keep {
            let (col, (text, tok, bold)) = (&cols[*i], &cells[*i]);
            let (tok, bold) = (*tok, *bold);
            let cell = if col.right { lpad(&fit(text, col.width as u16), col.width) } else { pad(&fit(text, col.width as u16), col.width) };
            pen.text(x, y, &cell, tok, bold);
            x += col.width as u16 + 1;
        }
        hits.push(Hit { rect: pen.row_rect(y), act: HitAct::Row(PanelId::Frontier, rec.key) });
    }
}

/// The Frontier state cell: the refuting case or the refusal code
/// rides along, verbatim.
fn state_text(rec: &Rec, state: RecState) -> String {
    let base = format!("{} {}", state.glyph(), state.name());
    match state {
        RecState::Refuted => rec.counterexample().map_or(base.clone(), |c| format!("{base} · c{c}")),
        RecState::Refused => rec.refusal().map_or(base.clone(), |c| format!("{base} {c}")),
        _ => base,
    }
}

fn heat(pen: &mut Pen<'_>, hits: &mut Vec<Hit>, model: &RunModel, view: &View, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let mut cases = model.case_ids.clone();
    cases.sort_unstable();
    if cases.is_empty() || model.records.is_empty() {
        let text = if model.records.is_empty() { "No records yet." } else { "No frozen cases yet: the audit is empty." };
        pen.text(0, 0, text, Tok::Dim, false);
        return;
    }
    let label_w: u16 = if expanded { 9 } else { 0 };
    let score_w: u16 = if expanded { 10 } else { 0 };
    let room = w.saturating_sub(label_w + score_w);
    let max_label = cases.iter().map(|c| c.to_string().len()).max().unwrap_or(1) as u16;
    let cw: u16 = if expanded {
        (room / cases.len().max(1) as u16).clamp(2, 5).max(max_label.min(5))
    } else if cases.len() as u16 * 3 <= room {
        3
    } else {
        2
    };
    let fit_n = usize::from(room / cw).min(cases.len());
    let shown: Vec<i128> = cases[cases.len() - fit_n..].to_vec();
    let last = model.last_batch();
    for (i, id) in shown.iter().enumerate() {
        if cw < 3 && i % 2 == 1 {
            continue;
        }
        let born = model.case_born.get(id).copied().unwrap_or(0);
        let new = last.saturating_sub(born) < 12 && born > 0;
        pen.text(label_w + i as u16 * cw, 0, &fit(&id.to_string(), cw.max(2) * if cw < 3 { 2 } else { 1 }), if new { Tok::Info } else { Tok::Dim }, new);
    }
    let rows = usize::from(h.saturating_sub(1));
    let selected = selected_key(view, model);
    let list = model.record_list(rows, false);
    for (row, rec) in list.iter().take(rows).enumerate() {
        let y = 1 + row as u16;
        if Some(rec.key) == selected {
            pen.bg(0, y, w, Tok::Select);
        }
        if expanded {
            let marker = if Some(rec.key) == selected { "▶" } else { " " };
            pen.text(0, y, &fit(&format!("{marker} #{}", rec.key), label_w - 1), Tok::Text, false);
        }
        let ce = rec.int("counterexample").filter(|c| *c >= 0);
        for (i, id) in shown.iter().enumerate() {
            let x = label_w + i as u16 * cw;
            if ce == Some(*id) {
                pen.put(x, y, '✗', Tok::Bad, true);
            } else {
                pen.put(x, y, '·', Tok::Faint, false);
            }
        }
        if expanded {
            let (_, decimal, tok) = score_cells(rec);
            pen.text(w - score_w, y, &lpad(&decimal, usize::from(score_w)), tok, false);
        }
        hits.push(Hit { rect: pen.row_rect(y), act: HitAct::Row(PanelId::Heat, rec.key) });
    }
}

fn verdicts(pen: &mut Pen<'_>, model: &RunModel, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let legend: Vec<(i128, &str)> = vec![(0, "running"), (3, "plateau"), (4, "budget"), (1, "goal"), (2, "domain")];
    let mut legend_lines: Vec<Vec<(String, Tok)>> = vec![Vec::new()];
    let mut line_w = 0usize;
    for (verdict, name) in &legend {
        let item = 2 + name.len() + 2;
        if line_w + item > usize::from(w) && line_w > 0 {
            legend_lines.push(Vec::new());
            line_w = 0;
        }
        let line = legend_lines.last_mut().expect("legend line");
        line.push(("■ ".into(), verdict_tok(*verdict)));
        line.push((format!("{name}  "), Tok::Dim));
        line_w += item;
    }
    let mut top = 0u16;
    if expanded {
        let mut counts: BTreeMap<i128, usize> = BTreeMap::new();
        for b in &model.batches {
            *counts.entry(b.verdict).or_default() += 1;
        }
        let head: Vec<String> = legend.iter().map(|(v, n)| format!("{n} {}", counts.get(v).copied().unwrap_or(0))).collect();
        let state = model.level_info.as_ref().map_or_else(|| "L1 open".to_string(), |l| format!("L1 closed · {}", l.reason));
        pen.text(0, 0, &fit(&format!("{}   ·   {state}", head.join("  ")), w), Tok::Dim, false);
        top = 2;
    }
    let reserved = legend_lines.len() as u16 + 1;
    let rows = h.saturating_sub(top + reserved + 1);
    let step: u16 = if expanded { 2 } else { 1 };
    let per_line = if expanded { (w / step).min(100) } else { w }.max(1);
    let capacity = usize::from(per_line) * usize::from(rows);
    let skip = model.batches.len().saturating_sub(capacity);
    let count = model.batches.len();
    for (i, b) in model.batches.iter().enumerate().skip(skip) {
        let slot = i - skip;
        let (x, y) = ((slot % usize::from(per_line)) as u16 * step, top + (slot / usize::from(per_line)) as u16);
        pen.put(x, y, '■', verdict_tok(b.verdict), false);
        if i + 1 == count {
            pen.bg(x, y, 1, Tok::Select);
        }
    }
    let legend_y = h.saturating_sub(reserved);
    for (i, line) in legend_lines.iter().enumerate() {
        let mut x = 0;
        for (text, tok) in line {
            x += pen.text(x, legend_y + i as u16, text, *tok, false);
        }
    }
    let now = model.batches.last().map_or("-", |b| verdict_label(b.verdict));
    let now_tok = model.batches.last().map_or(Tok::Dim, |b| verdict_tok(b.verdict));
    let y = h.saturating_sub(1);
    let x = pen.spans(0, y, &[("now ".into(), Tok::Dim, false), (now.into(), now_tok, true)]);
    pen.text(x, y, &format!("  ·  {} close", super::render::plural(model.plateau_close as usize, "plateau")), Tok::Dim, false);
}

/// The right-hand counts of one family row: `p→a`, `dup`, `ref`, `▲`.
fn family_tail(admitted: u64, refused: u64, promoted: u64) -> Vec<(String, Tok, bool)> {
    let count = |n: u64, width: usize, tok: Tok| {
        if n == 0 { (lpad("·", width), Tok::Faint, false) } else { (lpad(&n.to_string(), width), tok, tok == Tok::Good) }
    };
    vec![
        (lpad(&format!("-→{admitted}"), 6), Tok::Text, false),
        (" ".into(), Tok::Faint, false),
        (lpad("-", 3), Tok::Faint, false),
        (" ".into(), Tok::Faint, false),
        count(refused, 3, Tok::Warn),
        (" ".into(), Tok::Faint, false),
        count(promoted, 2, Tok::Good),
    ]
}

fn family_table(pen: &mut Pen<'_>, x: u16, y: u16, w: u16, rows: u16, title: &str, counts: &BTreeMap<i128, FamCountRef>) -> u16 {
    // Header: the table's own label in the family column, then the
    // admitted bar and the counts (`p→a` and `dup` need D1).
    let tail_head = format!("{} {} {} {}", lpad("p→a", 6), lpad("dup", 3), lpad("ref", 3), lpad("▲", 2));
    let tail_w = tail_head.chars().count() as u16;
    pen.text(x, y, &fit(title, 10), Tok::Dim, true);
    if w >= 11 + 8 + 1 + tail_w {
        pen.text(x + 11, y, "admitted", Tok::Dim, false);
    }
    pen.text(x + w.saturating_sub(tail_w), y, &tail_head, Tok::Dim, false);
    let mut line = y + 1;
    let bar_w = w.saturating_sub(11 + tail_w + 1);
    let max = counts.values().map(|c| c.0).max().unwrap_or(1).max(1);
    if counts.is_empty() {
        pen.text(x, line, "no admissions", Tok::Dim, false);
        return line + 1;
    }
    for (family, (admitted, refused, promoted)) in counts {
        if line >= y + rows {
            break;
        }
        pen.text(x, line, &pad(&format!("f{family}"), 10), Tok::Text, false);
        let len = ((*admitted as f64 / max as f64) * f64::from(bar_w)).round() as u16;
        for i in 0..len.max(u16::from(*admitted > 0)) {
            pen.put(x + 11 + i, line, '█', Tok::Accent, false);
        }
        let tail = family_tail(*admitted, *refused, *promoted);
        pen.spans(x + w.saturating_sub(tail_w), line, &tail);
        line += 1;
    }
    line
}

type FamCountRef = (u64, u64, u64);

fn fam_map(source: &BTreeMap<i128, super::model::FamCount>) -> BTreeMap<i128, FamCountRef> {
    source.iter().map(|(k, c)| (*k, (c.admitted, c.refused, c.promoted))).collect()
}

fn families(pen: &mut Pen<'_>, model: &RunModel, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let this = fam_map(&model.family_batch.1);
    let run = fam_map(&model.family_run);
    if expanded {
        let half = w / 2 - 1;
        section(pen, 0, "THIS BATCH", "");
        pen.text(half + 2, 0, "WHOLE RUN", Tok::Dim, true);
        let rows = h.saturating_sub(10);
        let a = family_table(pen, 0, 2, half, rows, "family", &this);
        let b = family_table(pen, half + 2, 2, half, rows, "family", &run);
        let mut y = a.max(b) + 1;
        pen.text(0, y, "STEERING", Tok::Dim, true);
        y += 1;
        if model.steers.is_empty() {
            pen.text(0, y, "no steering yet · g grows the audit", Tok::Dim, false);
        }
        for (batch, id) in model.steers.iter().rev() {
            if y + 1 >= h {
                break;
            }
            pen.text(0, y, &format!("b{batch}  grow_case c{id}  applied"), Tok::User, false);
            y += 1;
        }
        return;
    }
    if h >= 11 {
        let half_rows = h / 2;
        let label = format!("batch {}", model.family_batch.0);
        let end = family_table(pen, 0, 0, w, half_rows, &label, &this);
        let start = end.max(half_rows);
        family_table(pen, 0, start, w, h.saturating_sub(start), "whole run", &run);
    } else {
        family_table(pen, 0, 0, w, h, "family", &this);
    }
}

fn log_parts(line: &super::model::LogLine, width: u16) -> Vec<(String, Tok)> {
    let mut parts = line.parts.clone();
    let total = |p: &Vec<(String, Tok)>| p.iter().map(|x| x.0.chars().count()).sum::<usize>();
    while total(&parts) > usize::from(width) && parts.len() > 1 && parts.last().is_some_and(|p| p.1 == Tok::Dim) {
        parts.pop();
    }
    parts
}

fn events(pen: &mut Pen<'_>, model: &RunModel, view: &View, ctx: &Ctx) {
    let (w, h) = (pen.w(), pen.h());
    if model.log.is_empty() {
        pen.text(0, 0, "Waiting for journal events.", Tok::Dim, false);
        return;
    }
    let rows = usize::from(h);
    let back = view.events_scroll.unwrap_or(0).min(model.log.len().saturating_sub(1));
    let end = model.log.len() - back;
    let start = end.saturating_sub(rows);
    for (row, line) in model.log[start..end].iter().enumerate() {
        let y = row as u16;
        if ctx.fresh.contains(&line.seq) {
            pen.bg(0, y, w, Tok::Select);
        }
        let prefix = format!("{:>8} │ ", format!("L1 b{}", line.batch));
        let mut x = pen.text(0, y, &prefix, Tok::Dim, false);
        let room = w.saturating_sub(x);
        let parts = log_parts(line, room);
        let mut used = 0u16;
        let total: usize = parts.iter().map(|p| p.0.chars().count()).sum();
        for (text, tok) in parts {
            let left = room.saturating_sub(used);
            if left == 0 {
                break;
            }
            let shown = if total > usize::from(room) && text.chars().count() as u16 >= left { fit(&text, left) } else { text };
            let n = pen.text(x, y, &shown, tok, false);
            x += n;
            used += n;
        }
    }
}

fn lineage_chain(model: &RunModel, key: i128) -> Vec<i128> {
    let mut chain = vec![key];
    let mut at = key;
    while let Some(parent) = model.records.get(&at).and_then(Rec::parent) {
        if chain.contains(&parent) || chain.len() > 4096 {
            break;
        }
        chain.push(parent);
        at = parent;
    }
    chain.reverse();
    chain
}

fn rec_line(rec: &Rec) -> String {
    let score = match rec.score() {
        Some(s) if rec.tier() == 0 => approx(s),
        Some(s) => dec4(s),
        None => "-".into(),
    };
    format!("#{}  {}  {score}", rec.key, fam_move(rec))
}

fn best_children<'m>(model: &'m RunModel, key: i128, except: Option<i128>) -> Vec<&'m Rec> {
    let mut children: Vec<&Rec> = model.children(key).into_iter().filter(|r| Some(r.key) != except && r.tier() == 1).collect();
    children.sort_by(|a, b| match (a.score(), b.score()) {
        (Some(x), Some(y)) => super::model::rat_cmp(y, x).then(a.key.cmp(&b.key)),
        _ => a.key.cmp(&b.key),
    });
    children
}

fn lineage(pen: &mut Pen<'_>, model: &RunModel, view: &View, expanded: bool) {
    let (w, h) = (pen.w(), pen.h());
    let Some(key) = selected_key(view, model) else {
        pen.text(0, 0, "No record selected.", Tok::Dim, false);
        return;
    };
    let chain = lineage_chain(model, key);
    let mut lines: Vec<(bool, String, Tok, bool)> = Vec::new();
    for (i, k) in chain.iter().enumerate() {
        let Some(rec) = model.records.get(k) else { continue };
        let is_sel = *k == key;
        let glyph = if is_sel { '▶' } else if i == 0 { '◉' } else { '●' };
        lines.push((true, format!("{glyph} {}", rec_line(rec)), if is_sel { Tok::Accent } else { Tok::Text }, is_sel));
        if !is_sel {
            let next = chain.get(i + 1).copied();
            let others = best_children(model, *k, next);
            let total_children = model.children(*k).len();
            let show = if expanded { 2 } else { 1 };
            for other in others.iter().take(show) {
                lines.push((false, format!("├─ {}", rec_line(other)), Tok::Dim, false));
            }
            let more = total_children.saturating_sub(1 + others.len().min(show));
            if more > 0 {
                lines.push((false, format!("│  +{more} more tried"), Tok::Dim, false));
            }
        }
    }
    for child in best_children(model, key, None).iter().take(3) {
        lines.push((false, format!("└─ {}", rec_line(child)), Tok::Dim, false));
    }
    let rows = usize::from(h);
    let mut start = lines.len().saturating_sub(rows);
    while start < lines.len() && !lines[start].0 {
        start += 1;
    }
    for (row, (spine, text, tok, sel)) in lines.iter().skip(start).take(rows).enumerate() {
        let y = row as u16;
        if *sel {
            pen.bg(0, y, w, Tok::Select);
        }
        let indent = if *spine { 0 } else { 2 };
        pen.text(indent, y, &fit(text, w.saturating_sub(indent)), *tok, *sel);
    }
}

fn refuted(pen: &mut Pen<'_>, model: &RunModel) {
    let (w, h) = (pen.w(), pen.h());
    if model.quarantines.is_empty() {
        pen.text(0, 0, "No refutations yet.", Tok::Dim, true);
        let text = "When the audit grows, every accepted record is re-verified. A record that fails a new case is withdrawn here, kept for lineage, with the case that refuted it.";
        let mut y = 2;
        let mut line = String::new();
        for word in text.split(' ') {
            if line.chars().count() + word.chars().count() + 1 > usize::from(w) && !line.is_empty() {
                pen.text(0, y, &line, Tok::Dim, false);
                y += 1;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            pen.text(0, y, &line, Tok::Dim, false);
        }
        return;
    }
    let mut y = 0;
    for (batch, key) in model.quarantines.iter().rev() {
        if y + 1 >= h {
            break;
        }
        let rec = model.records.get(key);
        let ce = rec.and_then(|r| r.int("counterexample")).unwrap_or(-1);
        pen.spans(0, y, &[
            (format!("✗ #{key}"), Tok::Bad, true),
            (format!("  refuted by c{ce}"), Tok::Text, false),
            (format!("  at b{batch}"), Tok::Dim, false),
        ]);
        let was = rec.and_then(Rec::score).map_or_else(|| "-".into(), dec4);
        let family = rec.and_then(|r| r.int("family")).filter(|f| *f >= 0).map_or_else(|| "f-".into(), |f| format!("f{f}"));
        let mut detail = format!("was {was}  ·  {family}");
        let tail = "  ·  withdrawn, kept for lineage";
        if detail.chars().count() + tail.chars().count() + 2 <= usize::from(w) {
            detail.push_str(tail);
        }
        pen.text(2, y + 1, &detail, Tok::Dim, false);
        y += 2;
    }
}

fn section(pen: &mut Pen<'_>, y: u16, title: &str, hint: &str) {
    pen.text(0, y, title, Tok::Dim, true);
    pen.right(y, hint, Tok::Dim, false);
}

fn evidence(pen: &mut Pen<'_>, model: &RunModel, ctx: &Ctx) {
    let w = pen.w();
    let mut y = 0;
    let open = model.level_info.is_none();
    section(pen, y, "LEVEL", if model.emission && open { "certificate at close" } else { "" });
    y += 1;
    if !model.emission {
        pen.text(0, y, "no emission seam (dream_world / dream_pred)", Tok::Dim, false);
        y += 1;
        if let Some(level) = &model.level_info {
            pen.text(0, y, &format!("closed · {}", level.reason), Tok::Accent2, true);
            y += 1;
        }
    } else if let Some(level) = &model.level_info {
        pen.text(0, y, &format!("closed · {} at b{}", level.reason, level.batch), Tok::Accent2, true);
        y += 1;
        let score = level.score.map_or_else(String::new, |s| format!("  {}  ({})", rat_text(s), dec4(s)));
        pen.text(0, y, &format!("incumbent #{}{score}", level.key.map_or_else(|| "-".into(), |k| k.to_string())), Tok::Text, false);
        y += 1;
        let file = level.file.clone().unwrap_or_else(|| "-".into());
        let x = pen.text(0, y, &fit(&file, w.saturating_sub(24)), Tok::Info, false);
        if level.certificate {
            pen.text(x + 2, y, "✓ certificate", Tok::Good, true);
        } else {
            pen.text(x + 2, y, "✗ certificate failed", Tok::Bad, true);
        }
        y += 1;
    } else {
        pen.text(0, y, &format!("L1 open · {} · batch {} · certificate at close", super::render::plural(model.case_ids.len(), "case"), model.last_batch()), Tok::Text, false);
        y += 1;
    }
    y += 1;
    section(pen, y, "RUN", "");
    y += 1;
    let hash = model.meaning_id.rsplit(':').next().unwrap_or(&model.meaning_id);
    let meaning: String = hash.chars().take(16).collect();
    let max_units = model.max_units.map_or_else(|| "none".into(), units);
    let rows = [
        ("meaning", format!("{meaning}…")),
        ("language", model.language_id.clone()),
        ("target", model.target.clone()),
        ("config", format!("budget0 {} · step {} · plateau {} · ckpt {} · max {} b · ceiling {max_units}", model.budget0, model.budget_step, model.plateau_close, model.checkpoint_every, model.max_batches)),
    ];
    for (label, value) in rows {
        pen.text(0, y, label, Tok::Dim, false);
        pen.text(10, y, &fit(&value, w.saturating_sub(10)), Tok::Text, false);
        y += 1;
    }
    y += 1;
    section(pen, y, "NATIVE PARITY", "");
    y += 1;
    pen.text(0, y, "not enabled (D4)", Tok::Dim, false);
    y += 2;
    section(pen, y, "CHECKPOINTS", "resume · fork --at N");
    y += 1;
    if ctx.checkpoints.is_empty() {
        pen.text(0, y, &format!("none yet · first at b{}", model.checkpoint_every), Tok::Dim, false);
    }
    for (batch, file, bytes) in ctx.checkpoints.iter().rev() {
        if y >= pen.h() {
            break;
        }
        pen.text(0, y, &format!("b{batch:<6} {file}  {:.1} kB", *bytes as f64 / 1024.0), Tok::Text, false);
        y += 1;
    }
}

fn json_tokens(line: &str) -> Vec<(String, Tok)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(chars.len());
            let text: String = chars[i..end].iter().collect();
            let mut k = end;
            while k < chars.len() && chars[k] == ' ' {
                k += 1;
            }
            let is_key = k < chars.len() && chars[k] == ':';
            out.push((text, if is_key { Tok::Info } else { Tok::Good }));
            i = end;
        } else if ch == '-' || ch.is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            out.push((chars[i..j].iter().collect(), Tok::Warn));
            i = j;
        } else if ch.is_ascii_alphabetic() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_alphabetic() {
                j += 1;
            }
            out.push((chars[i..j].iter().collect(), Tok::Violet));
            i = j;
        } else {
            out.push((ch.to_string(), Tok::Faint));
            i += 1;
        }
    }
    out
}

fn journal(pen: &mut Pen<'_>, model: &RunModel, view: &View) {
    let (w, h) = (pen.w(), pen.h());
    if model.raw.is_empty() {
        pen.text(0, 0, "journal.jsonl is empty.", Tok::Dim, false);
        return;
    }
    let rows = usize::from(h);
    let back = view.journal_scroll.unwrap_or(0).min(model.raw.len().saturating_sub(1));
    let end = model.raw.len() - back;
    let start = end.saturating_sub(rows);
    for (row, (_, line)) in model.raw.iter().skip(start).take(end - start).enumerate() {
        let y = row as u16;
        let mut x = 0u16;
        let total = line.chars().count();
        for (text, tok) in json_tokens(line) {
            if x >= w {
                break;
            }
            let left = w - x;
            let shown = if total > usize::from(w) && text.chars().count() as u16 >= left { fit(&text, left) } else { text };
            x += pen.text(x, y, &shown, tok, false);
        }
        if total > usize::from(w) {
            pen.put(w - 1, y, '…', Tok::Dim, false);
        }
    }
}
