//! Frame composition (SPEC §7): layout, top bar, KPI strip, panel
//! frames, status bar, toasts, overlays, and the small-terminal modes.
//! `render` is pure: the same model, view, context and size always
//! produce the same `Canvas`, which is what the headless goldens pin.

use super::canvas::{Canvas, Rect, Tok};
use super::model::{dec4, rat_text, units, RunModel};
use super::panels::{self, fit, Pen};
use super::view::{tab_rows, Ctx, Overlay, PanelId, RunBadge, View, ALL_PANELS, TAB_NAMES};

/// Screen mode by terminal size (SPEC §7.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Full,
    Single,
    Text,
}

/// Panel placement for one frame.
#[derive(Clone, Debug)]
pub struct Layout {
    pub mode: Mode,
    pub panels: Vec<(PanelId, Rect)>,
    /// Panels not drawn because their rect is below the minimum.
    pub too_small: usize,
}

/// What a click on a region does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitAct {
    Tab(usize),
    Focus(PanelId),
    Toggle(PanelId),
    Row(PanelId, i128),
    Key(char),
    PanelSwitch(PanelId),
    Outside,
}

/// A clickable region.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub act: HitAct,
}

/// One rendered frame.
#[derive(Clone, Debug)]
pub struct Frame {
    pub canvas: Canvas,
    pub hits: Vec<Hit>,
    pub layout: Layout,
}

/// Largest-remainder split (SPEC §7): `total − gap·(n−1)` shared by
/// weight; leftovers to the largest fractions, ties to the lower index.
pub fn split(total: u16, weights: &[u32], gap: u16) -> Vec<u16> {
    if weights.is_empty() {
        return Vec::new();
    }
    let gaps = gap.saturating_mul(weights.len() as u16 - 1);
    let avail = u64::from(total.saturating_sub(gaps));
    let sum: u64 = weights.iter().map(|w| u64::from(*w)).sum::<u64>().max(1);
    let mut parts: Vec<u64> = weights
        .iter()
        .map(|w| avail * u64::from(*w) / sum)
        .collect();
    let mut rest = avail - parts.iter().sum::<u64>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|a, b| {
        let fa = avail * u64::from(weights[*a]) % sum;
        let fb = avail * u64::from(weights[*b]) % sum;
        fb.cmp(&fa).then(a.cmp(b))
    });
    for index in order {
        if rest == 0 {
            break;
        }
        parts[index] += 1;
        rest -= 1;
    }
    parts.into_iter().map(|p| p as u16).collect()
}

/// The body rectangle: x = 2, rows 7 … H−3.
pub fn body_rect(w: u16, h: u16) -> Rect {
    Rect::new(2, 7, w.saturating_sub(4), h.saturating_sub(9))
}

fn fits(panel: PanelId, rect: Rect) -> bool {
    let (cols, rows) = panel.min();
    rect.w >= cols + 4 && rect.h >= rows + 2
}

/// Place the current tab's visible panels (SPEC §7.4, §7.6).
pub fn layout(view: &View, w: u16, h: u16) -> Layout {
    if w < 60 || h < 16 {
        return Layout {
            mode: Mode::Text,
            panels: Vec::new(),
            too_small: 0,
        };
    }
    let body = body_rect(w, h);
    let visible = view.visible();
    if w < 100 || h < 30 {
        let panel = view
            .expanded
            .or(view.focus.filter(|p| visible.contains(p)))
            .or(visible.first().copied());
        return Layout {
            mode: Mode::Single,
            panels: panel.map(|p| vec![(p, body)]).unwrap_or_default(),
            too_small: 0,
        };
    }
    if let Some(panel) = view.expanded {
        return Layout {
            mode: Mode::Full,
            panels: vec![(panel, body)],
            too_small: 0,
        };
    }
    // Panels are admitted in priority order, each kept only if every
    // kept panel still meets its minimum after the re-split: a small
    // body sheds the least useful panels, never the score chart first.
    let candidates: Vec<PanelId> = PRIORITY
        .iter()
        .copied()
        .filter(|p| visible.contains(p))
        .collect();
    let mut keep: Vec<PanelId> = Vec::new();
    for panel in &candidates {
        let mut trial = keep.clone();
        trial.push(*panel);
        if place(view.tab, body, &trial).is_some() {
            keep = trial;
        }
    }
    let panels = match place(view.tab, body, &keep) {
        Some(placed) if !placed.is_empty() => placed,
        _ => candidates.first().map(|p| vec![(*p, body)]).unwrap_or_default(),
    };
    Layout {
        mode: Mode::Full,
        too_small: candidates.len() - panels.len(),
        panels,
    }
}

/// Which panels a crowded body keeps first (SPEC §7.6).
const PRIORITY: [PanelId; 11] = [
    PanelId::Score,
    PanelId::Frontier,
    PanelId::Events,
    PanelId::Evidence,
    PanelId::Journal,
    PanelId::Lineage,
    PanelId::Budget,
    PanelId::Refuted,
    PanelId::Verdicts,
    PanelId::Heat,
    PanelId::Families,
];

/// Split the tab's rows among `keep` (SPEC §7.4); `None` when a kept
/// panel lands below its minimum.
fn place(tab: usize, body: Rect, keep: &[PanelId]) -> Option<Vec<(PanelId, Rect)>> {
    let rows: Vec<(u32, Vec<(PanelId, u32)>)> = tab_rows(tab)
        .into_iter()
        .map(|(hw, cols)| (hw, cols.into_iter().filter(|(p, _)| keep.contains(p)).collect::<Vec<_>>()))
        .filter(|(_, cols)| !cols.is_empty())
        .collect();
    if rows.is_empty() {
        return Some(Vec::new());
    }
    let heights = split(body.h, &rows.iter().map(|r| r.0).collect::<Vec<_>>(), 1);
    let mut placed = Vec::new();
    let mut y = body.y;
    for ((_, cols), height) in rows.iter().zip(heights) {
        let widths = split(body.w, &cols.iter().map(|c| c.1).collect::<Vec<_>>(), 2);
        let mut x = body.x;
        for ((panel, _), width) in cols.iter().zip(widths) {
            let rect = Rect::new(x, y, width, height);
            if !fits(*panel, rect) {
                return None;
            }
            placed.push((*panel, rect));
            x += width + 2;
        }
        y += height + 1;
    }
    Some(placed)
}

/// The selected record key (follow ⇒ the incumbent).
pub fn selected_key(view: &View, model: &RunModel) -> Option<i128> {
    if view.follow {
        model.incumbent
    } else {
        view.selected.or(model.incumbent)
    }
}

/// Render one frame.
pub fn render(model: &RunModel, view: &View, ctx: &Ctx, w: u16, h: u16) -> Frame {
    let mut canvas = Canvas::new(w, h);
    let mut hits = Vec::new();
    let layout = layout(view, w, h);
    if layout.mode == Mode::Text {
        text_mode(&mut canvas, model, ctx);
        return Frame {
            canvas,
            hits,
            layout,
        };
    }
    top_bar(&mut canvas, &mut hits, model, view, ctx);
    kpi_strip(&mut canvas, model, ctx);
    for (panel, rect) in &layout.panels {
        draw_panel(&mut canvas, &mut hits, model, view, ctx, *panel, *rect);
    }
    toast_row(&mut canvas, model, ctx);
    status_bar(&mut canvas, &mut hits, model, view, ctx, layout.too_small);
    if view.overlay != Overlay::None {
        overlay(&mut canvas, &mut hits, model, view, ctx);
    }
    Frame {
        canvas,
        hits,
        layout,
    }
}

fn top_bar(c: &mut Canvas, hits: &mut Vec<Hit>, model: &RunModel, view: &View, ctx: &Ctx) {
    let w = c.w;
    c.fill(Rect::new(0, 0, w, 1), Tok::Bar);
    for x in 0..w {
        c.put(x, 1, '─', Tok::Rule, false);
    }
    let mut x = 2;
    c.put(x, 0, '◆', Tok::Accent, true);
    x += 1;
    x += c.text(x, 0, w.saturating_sub(x), " emath dream", Tok::Text, true);
    x += 5;
    for (index, name) in TAB_NAMES.iter().enumerate() {
        let active = index == view.tab;
        let start = x;
        let label = format!("{} {name}", index + 1);
        if x + label.chars().count() as u16 >= w {
            break;
        }
        c.put(
            x,
            0,
            char::from(b'1' + index as u8),
            if active { Tok::Accent } else { Tok::Faint },
            active,
        );
        c.text(
            x + 2,
            0,
            w,
            name,
            if active { Tok::Text } else { Tok::Dim },
            active,
        );
        x += label.chars().count() as u16;
        if active {
            for ux in start.saturating_sub(1)..(x + 1).min(w) {
                c.put(ux, 1, '━', Tok::Accent, false);
            }
        }
        hits.push(Hit {
            rect: Rect::new(start, 0, x - start, 1),
            act: HitAct::Tab(index),
        });
        x += 4;
    }
    if let Some(panel) = view.expanded {
        let text = format!("›  {}  expanded", panel.title());
        x += c.text(x, 0, w.saturating_sub(x + 2), &text, Tok::Dim, false) + 4;
    }
    let right = format!(
        "run {}    {}  →  {}",
        ctx.run_name, ctx.module_file, model.target
    );
    let len = right.chars().count() as u16;
    if x + len + 2 <= w {
        let rx = w - 2 - len;
        let run_len = 4 + ctx.run_name.chars().count() as u16;
        c.text(rx, 0, run_len, &right, Tok::Text, false);
        c.text(
            rx + run_len,
            0,
            len - run_len,
            &right.chars().skip(run_len as usize).collect::<String>(),
            Tok::Dim,
            false,
        );
    }
}

fn tile(
    c: &mut Canvas,
    rect: Rect,
    label: &str,
    right_label: Option<(String, Tok)>,
    left: &[(String, Tok, bool)],
    right: &[(String, Tok, bool)],
) {
    c.frame(rect, Tok::Border);
    let inner = rect.w.saturating_sub(4);
    let lw = c.text(
        rect.x + 2,
        rect.y,
        inner,
        &format!(" {label} "),
        Tok::Dim,
        false,
    );
    if let Some((text, tok)) = right_label {
        let text = format!(" {text} ");
        let len = text.chars().count() as u16;
        if lw + len + 2 <= inner {
            c.text(rect.x + rect.w - 2 - len, rect.y, len, &text, tok, false);
        }
    }
    let width = |parts: &[(String, Tok, bool)]| {
        parts
            .iter()
            .map(|p| p.0.chars().count() as u16)
            .sum::<u16>()
    };
    let (lwid, rwid) = (width(left), width(right));
    let mut x = rect.x + 2;
    let y = rect.y + 1;
    for (text, tok, bold) in left {
        let room = (rect.x + 2 + inner).saturating_sub(x);
        x += c.text(x, y, room, &fit(text, room), *tok, *bold);
    }
    if !right.is_empty() && lwid + 1 + rwid <= inner {
        let mut rx = rect.x + 2 + inner - rwid;
        for (text, tok, bold) in right {
            rx += c.text(rx, y, rwid, text, *tok, *bold);
        }
    }
}

fn measure(value: Option<String>) -> (String, Tok, bool) {
    (value.unwrap_or_else(|| "—".into()), Tok::Dim, false)
}

fn kpi_strip(c: &mut Canvas, model: &RunModel, ctx: &Ctx) {
    let w = c.w;
    let (badge, badge_tok) = ctx.badge.text();
    let used = model.used();
    let wm = model.watermark().max(1);
    let score = model.incumbent_rec().and_then(|r| r.score());
    if w < 100 {
        let mut line = format!("{badge}  L{} b{}", model.level, model.last_batch());
        if let (Some(s), Some(k)) = (score, model.incumbent) {
            line.push_str(&format!("  {} #{k}", dec4(s)));
        }
        line.push_str(&format!("  {}/{} u", units(used), units(wm)));
        let bl = badge.chars().count() as u16;
        c.text(2, 3, w.saturating_sub(4), &line, Tok::Text, false);
        c.text(2, 3, bl.min(w.saturating_sub(4)), &badge, badge_tok, true);
        return;
    }
    let count = if w >= 150 {
        6
    } else if w >= 125 {
        5
    } else {
        4
    };
    let weights = [22u32, 24, 28, 34, 24, 24];
    let mut widths = split(w - 4, &weights[..count], 2);
    // The RUN badge is never cut: RUN borrows the cells it needs from
    // BUDGET, the tile with the most slack.
    let need = badge.chars().count() as u16 + 4;
    if widths[0] < need {
        let give = (need - widths[0]).min(widths[3].saturating_sub(24));
        widths[0] += give;
        widths[3] -= give;
    }
    let mut x = 2;
    let mut rects = Vec::new();
    for width in widths {
        rects.push(Rect::new(x, 3, width, 3));
        x += width + 2;
    }
    let elapsed = ctx
        .elapsed_secs
        .map(|s| format!("{:02}:{:02}", s / 60, s % 60));
    tile(
        c,
        rects[0],
        "RUN",
        None,
        &[(badge, badge_tok, true)],
        &[measure(elapsed)],
    );
    let streak = model.plateau_streak();
    let dots = model.plateau_dots();
    tile(
        c,
        rects[1],
        "PROGRESS",
        None,
        &[(
            format!("L{}  ·  batch {}", model.level, model.last_batch()),
            Tok::Text,
            false,
        )],
        &[(
            dots,
            if streak > 0 { Tok::Warn } else { Tok::Plateau },
            false,
        )],
    );
    let gain = model
        .promotions
        .last()
        .filter(|p| Some(p.key) == model.incumbent)
        .and_then(|p| p.gain)
        .map(|g| (super::model::gain_text(g), Tok::Good));
    let (left, right) = match (score, model.incumbent) {
        (Some(s), Some(k)) => (
            vec![
                (dec4(s), Tok::Good, true),
                (format!("  {}", rat_text(s)), Tok::Dim, false),
            ],
            vec![(format!("#{k}"), Tok::Accent, true)],
        ),
        _ => (vec![("—".to_string(), Tok::Dim, false)], Vec::new()),
    };
    tile(c, rects[2], "INCUMBENT", gain, &left, &right);
    let pct = (used.max(0) as f64 / wm as f64 * 100.0).round() as i64;
    let gauge_w = rects[3].w.saturating_sub(4 + 6) as usize;
    let lit = ((used.max(0) as f64 / wm as f64) * gauge_w as f64)
        .round()
        .clamp(0.0, gauge_w as f64) as usize;
    let gauge_tok = if pct >= 85 { Tok::Warn } else { Tok::Accent };
    tile(
        c,
        rects[3],
        "BUDGET",
        Some((format!("{} / {} u", units(used), units(wm)), Tok::Dim)),
        &[
            ("━".repeat(lit), gauge_tok, false),
            ("━".repeat(gauge_w - lit), Tok::Faint, false),
        ],
        &[(format!("{pct}%"), Tok::Text, false)],
    );
    if count >= 5 {
        let bps = ctx.bps.map(|b| format!("{b:.1} b/s"));
        tile(
            c,
            rects[4],
            "THROUGHPUT",
            None,
            &[measure(bps)],
            &[("cache —".to_string(), Tok::Dim, false)],
        );
    }
    if count >= 6 {
        let last = model
            .growths
            .last()
            .map(|(b, ids, _)| format!("+{} b{b}", ids.len()));
        let right = match last {
            Some(text) => (text, Tok::Info, false),
            None => ("—".to_string(), Tok::Dim, false),
        };
        tile(
            c,
            rects[5],
            "AUDIT",
            None,
            &[(plural(model.case_ids.len(), "case"), Tok::Text, false)],
            &[right],
        );
    }
}

/// `1 case`, `2 cases`.
pub(crate) fn plural(n: usize, noun: &str) -> String {
    if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

fn panel_subtitle(model: &RunModel, view: &View, panel: PanelId) -> String {
    match panel {
        PanelId::Score => format!(
            "level {} · {} · {}",
            model.level,
            plural(model.promotions.len(), "promotion"),
            plural(model.case_ids.len(), "case")
        ),
        PanelId::Budget => "2 · used + step".into(),
        PanelId::Frontier => format!("{} records  ·  ↑↓ select", model.records.len()),
        PanelId::Heat => {
            let mut ids = model.case_ids.clone();
            ids.sort_unstable();
            match (ids.first(), ids.last()) {
                (Some(a), Some(b)) if a == b => format!("c{a}"),
                (Some(a), Some(b)) => format!("c{a}–c{b}"),
                _ => "no cases".into(),
            }
        }
        PanelId::Verdicts => {
            format!("plateau {}", model.plateau_dots())
        }
        PanelId::Families => format!("batch {}", model.family_batch.0),
        PanelId::Events => match view.events_scroll {
            Some(_) => format!("journal seq {}  ·  following ▸ paused", model.last_seq),
            None => format!("journal seq {}", model.last_seq),
        },
        PanelId::Lineage => {
            selected_key(view, model).map_or_else(|| "none".into(), |k| format!("#{k}"))
        }
        PanelId::Refuted => format!("{} this run", model.quarantines.len()),
        PanelId::Evidence => {
            if model.level_info.is_some() {
                "level closed".into()
            } else {
                "level open".into()
            }
        }
        PanelId::Journal => "canonical · metrics are separate".into(),
    }
}

/// Trim a `·`-separated label to `max` cells: drop trailing parts
/// first, then cut what remains with `…` (SPEC §7.5).
fn fit_parts(text: &str, max: u16) -> String {
    let mut parts: Vec<&str> = text.split('·').collect();
    let mut out = text.to_string();
    while out.chars().count() > usize::from(max) && parts.len() > 1 {
        parts.pop();
        out = parts.join("·").trim_end().to_string();
    }
    fit(&out, max)
}

fn panel_footer(model: &RunModel, panel: PanelId) -> Option<String> {
    match panel {
        PanelId::Heat if !model.records.is_empty() => {
            Some("per-case scores need D2 · refutations shown".into())
        }
        PanelId::Families => Some("proposed/dup need D1".into()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_panel(
    c: &mut Canvas,
    hits: &mut Vec<Hit>,
    model: &RunModel,
    view: &View,
    ctx: &Ctx,
    panel: PanelId,
    rect: Rect,
) {
    let expanded = view.expanded == Some(panel);
    let focused = view.focus == Some(panel) || expanded;
    c.fill(rect, Tok::Panel);
    c.frame(
        rect,
        if focused {
            Tok::BorderFocus
        } else {
            Tok::Border
        },
    );
    hits.push(Hit {
        rect,
        act: HitAct::Focus(panel),
    });
    let inner = rect.w.saturating_sub(4);
    let title_w = c.text(
        rect.x + 2,
        rect.y,
        inner,
        &format!(" {} ", panel.title()),
        if focused { Tok::Accent } else { Tok::Text },
        true,
    );
    let icon = if expanded { " ✕ " } else { " ⤢ " };
    let icon_x = rect.x + rect.w.saturating_sub(5);
    if rect.w > title_w + 10 {
        c.text(
            icon_x,
            rect.y,
            3,
            icon,
            if focused { Tok::Accent } else { Tok::Dim },
            false,
        );
        hits.push(Hit {
            rect: Rect::new(icon_x, rect.y, 3, 1),
            act: HitAct::Toggle(panel),
        });
        let mut subtitle = panel_subtitle(model, view, panel);
        if expanded {
            subtitle.push_str("   esc to close");
        }
        let room = icon_x.saturating_sub(rect.x + 2 + title_w + 3);
        let sub = format!(" {} ", fit_parts(&subtitle, room.saturating_sub(2)));
        let len = sub.chars().count() as u16;
        if len > 2 {
            c.text(icon_x - len - 1, rect.y, len, &sub, Tok::Dim, false);
        }
    }
    if let Some(footer) = panel_footer(model, panel) {
        let text = format!(" {} ", fit_parts(&footer, inner.saturating_sub(2)));
        c.text(
            rect.x + 2,
            rect.y + rect.h - 1,
            inner,
            &text,
            Tok::Dim,
            false,
        );
    }
    view.count(panel);
    let mut pen = Pen::new(c, rect.content());
    panels::draw(&mut pen, hits, model, view, ctx, panel, expanded);
}

fn toast_row(c: &mut Canvas, model: &RunModel, ctx: &Ctx) {
    let y = c.h - 2;
    let w = c.w;
    if let Some(end) = &model.end {
        let (mut text, tok) = super::model::ended_banner(model, end);
        // Undo for a steer that faulted the run: fork from the newest
        // checkpoint before the steer (a fork at the steer's own batch
        // would replay it).
        if end.outcome == "fault" {
            if let Some((batch, _)) = model.steers.last().filter(|(b, _)| *b == end.batch) {
                if let Some((at, _, _)) = ctx.checkpoints.iter().filter(|(b, _, _)| b < batch).last() {
                    text.push_str(&format!(" · undo: emath dream fork {} --at {at}", ctx.run_name));
                }
            }
        }
        c.text(2, y, w.saturating_sub(4), &fit(&text, w.saturating_sub(4)), tok, true);
        return;
    }
    if ctx.badge == RunBadge::Stalled {
        let text = format!("driver not running · emath dream resume {}", ctx.run_name);
        c.text(2, y, w.saturating_sub(4), &text, Tok::Bad, true);
        return;
    }
    if let Some((text, tok, _)) = &ctx.toast {
        let text = fit(&format!("✎  {text}"), w.saturating_sub(4));
        let len = text.chars().count() as u16;
        c.text(w - 2 - len, y, len, &text, *tok, false);
    }
}

/// Status-bar entries: key label, hit key, text, and whether the run
/// can take that control right now (SPEC §7.3). An ended or stalled run
/// takes no run control; step needs a paused run.
fn status_entries(view: &View, ctx: &Ctx) -> Vec<(&'static str, char, &'static str, bool)> {
    let paused = matches!(ctx.badge, RunBadge::Paused(_));
    let live = !ctx.badge.ended() && ctx.badge != RunBadge::Stalled;
    vec![
        ("space", ' ', if paused { "resume" } else { "pause" }, live),
        ("s", 's', "step", live && paused),
        ("g", 'g', "grow", live),
        ("X", 'X', "stop", live),
        ("f", 'f', "fork", true),
        ("⇥", '\t', "focus", true),
        (
            "⏎",
            '\n',
            if view.expanded.is_some() {
                "close"
            } else {
                "expand"
            },
            true,
        ),
        ("v", 'v', "panels", true),
        ("t", 't', "theme", true),
        ("?", '?', "help", true),
    ]
}

fn status_bar(
    c: &mut Canvas,
    hits: &mut Vec<Hit>,
    model: &RunModel,
    view: &View,
    ctx: &Ctx,
    too_small: usize,
) {
    let y = c.h - 1;
    let w = c.w;
    c.fill(Rect::new(0, y, w, 1), Tok::Bar);
    let journal = format!(" journal #{}", model.last_seq);
    let frame = match ctx.frame_ms {
        Some(ms) => format!("    frame {ms:.1} ms"),
        None => "    frame —".to_string(),
    };
    let right_w = 1 + journal.chars().count() as u16 + frame.chars().count() as u16;
    let rx = w.saturating_sub(2 + right_w);
    c.put(rx, y, '●', ctx.badge.dot(), false);
    c.text(rx + 1, y, right_w, &journal, Tok::Dim, false);
    let frame_tok = if ctx.frame_ms.is_some_and(|ms| ms > 16.0) {
        Tok::Warn
    } else {
        Tok::Dim
    };
    c.text(
        rx + 1 + journal.chars().count() as u16,
        y,
        right_w,
        &frame,
        frame_tok,
        false,
    );
    let mut limit = rx.saturating_sub(4);
    if too_small > 0 {
        let note = format!("+{too_small} panels need a larger terminal");
        let len = note.chars().count() as u16;
        if len + 6 < limit {
            c.text(limit - len, y, len, &note, Tok::Dim, false);
            limit = limit.saturating_sub(len + 4);
        }
    }
    let mut x = 2;
    for (key, hit, label, enabled) in status_entries(view, ctx) {
        let cap = format!(" {key} ");
        let entry = cap.chars().count() as u16 + 1 + label.chars().count() as u16;
        if x + entry > limit {
            break;
        }
        // A control the run cannot take keeps its place and its click
        // (which explains why) but drops the keycap chip.
        for (offset, ch) in cap.chars().enumerate() {
            let cx = x + offset as u16;
            if enabled {
                c.put(cx, y, ch, Tok::KeyFg, true);
                c.set_bg(cx, y, Tok::Keycap);
            } else {
                c.put(cx, y, ch, Tok::Dim, false);
            }
        }
        let cap_w = cap.chars().count() as u16;
        c.text(x + cap_w + 1, y, label.len() as u16, label, Tok::Dim, false);
        hits.push(Hit {
            rect: Rect::new(x, y, entry, 1),
            act: HitAct::Key(hit),
        });
        x += entry + 3;
    }
}

fn text_mode(c: &mut Canvas, model: &RunModel, ctx: &Ctx) {
    let (badge, tok) = ctx.badge.text();
    let w = c.w;
    let mut lines: Vec<(String, Tok)> = vec![
        (badge, tok),
        (
            format!("L{} · batch {}", model.level, model.last_batch()),
            Tok::Text,
        ),
        (
            match (
                model.incumbent_rec().and_then(|r| r.score()),
                model.incumbent,
            ) {
                (Some(s), Some(k)) => format!("incumbent #{k}  {}  {}", dec4(s), rat_text(s)),
                _ => "incumbent —".to_string(),
            },
            Tok::Good,
        ),
        (
            format!(
                "budget {} / {} u",
                units(model.used()),
                units(model.watermark())
            ),
            Tok::Text,
        ),
    ];
    for line in model
        .log
        .iter()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let text: String = line.parts.iter().map(|p| p.0.as_str()).collect();
        lines.push((format!("b{} {text}", line.batch), Tok::Dim));
    }
    lines.push((
        "enlarge the terminal for the dashboard".to_string(),
        Tok::Dim,
    ));
    for (row, (text, tok)) in lines.iter().enumerate() {
        if row as u16 >= c.h {
            break;
        }
        c.text(
            1,
            row as u16,
            w.saturating_sub(2),
            &fit(text, w.saturating_sub(2)),
            *tok,
            row == 0,
        );
    }
}

fn modal(c: &mut Canvas, hits: &mut Vec<Hit>, title: &str, width: u16, height: u16) -> Rect {
    c.veil();
    hits.push(Hit {
        rect: Rect::new(0, 0, c.w, c.h),
        act: HitAct::Outside,
    });
    let width = width.min(c.w.saturating_sub(4));
    let height = height.min(c.h.saturating_sub(2));
    let rect = Rect::new((c.w - width) / 2, (c.h - height) / 2, width, height);
    c.fill(rect, Tok::Panel);
    c.frame(rect, Tok::BorderFocus);
    hits.push(Hit {
        rect,
        act: HitAct::Focus(PanelId::Score),
    });
    hits.pop();
    c.text(
        rect.x + 2,
        rect.y,
        width.saturating_sub(4),
        &format!(" {title} "),
        Tok::Accent,
        true,
    );
    let close = " esc ✕ ";
    let len = close.chars().count() as u16;
    c.text(
        rect.x + width - 2 - len,
        rect.y,
        len,
        close,
        Tok::Dim,
        false,
    );
    rect
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn overlay(c: &mut Canvas, hits: &mut Vec<Hit>, model: &RunModel, view: &View, ctx: &Ctx) {
    match view.overlay {
        Overlay::None => {}
        Overlay::Panels => {
            let rect = modal(c, hits, "Panels", 96, 23);
            let content = rect.content();
            let mut y = content.y + 1;
            for line in wrap("Turn panels off to save CPU and memory. A hidden panel stops folding the journal, and the layout reflows to give its space to the others.", content.w as usize) {
                c.text(content.x, y, content.w, &line, Tok::Text, false);
                y += 1;
            }
            y += 1;
            for (index, panel) in ALL_PANELS.iter().enumerate() {
                let letter = char::from(b'a' + index as u8);
                let mut x = content.x;
                for (offset, ch) in format!(" {letter} ").chars().enumerate() {
                    c.put(x + offset as u16, y, ch, Tok::KeyFg, true);
                    c.set_bg(x + offset as u16, y, Tok::Keycap);
                }
                x += 4;
                let on = !view.hidden.contains(panel);
                c.put(
                    x,
                    y,
                    if on { '●' } else { '○' },
                    if on { Tok::Good } else { Tok::Dim },
                    false,
                );
                x += 2;
                c.text(
                    x,
                    y,
                    20,
                    panel.title(),
                    if on { Tok::Text } else { Tok::Dim },
                    on,
                );
                x += 21;
                let (cost, tok) = match panel.cost() {
                    super::view::Cost::Heavy => ("heavy", Tok::Warn),
                    super::view::Cost::Medium => ("medium", Tok::Text),
                    super::view::Cost::Light => ("light", Tok::Dim),
                };
                c.text(x, y, 7, cost, tok, false);
                x += 8;
                c.text(
                    x,
                    y,
                    (content.x + content.w).saturating_sub(x),
                    &fit(panel.describe(), (content.x + content.w).saturating_sub(x)),
                    Tok::Dim,
                    false,
                );
                hits.push(Hit {
                    rect: Rect::new(content.x, y, content.w, 1),
                    act: HitAct::PanelSwitch(*panel),
                });
                y += 1;
            }
            y += 1;
            for line in wrap("When frame time stays above 16 ms, heavy panels are offered for switching off first.", content.w as usize) {
                c.text(content.x, y, content.w, &line, Tok::Dim, false);
                y += 1;
            }
        }
        Overlay::Keys => {
            let rect = modal(c, hits, "Keys", 76, 24);
            let content = rect.content();
            let rows: &[(&str, &str)] = &[
                ("1–4  ← →", "switch tab"),
                ("⇥  ⇧⇥", "focus next / previous panel"),
                ("⏎  double-click", "expand / close the focused panel"),
                ("esc", "close overlay or expanded panel"),
                ("↑ ↓ PgUp PgDn", "select a record / scroll events"),
                ("Home", "select the incumbent (follow)"),
                ("End", "events / journal: follow the tail"),
                ("space", "pause / resume"),
                ("s", "step one batch (while paused)"),
                ("g", "grow the audit by one case"),
                ("X", "stop the run (resumable)"),
                ("c", "write a checkpoint"),
                ("f", "fork from the newest checkpoint"),
                ("p", "pin a family (needs D5)"),
                ("v", "panels on / off"),
                ("t  Ctrl+T", "next theme"),
                ("?", "this help"),
                ("q  Ctrl+C", "detach · the run continues"),
            ];
            let mut y = content.y + 1;
            for (keys, what) in rows {
                if y >= content.y + content.h {
                    break;
                }
                c.text(content.x, y, 18, keys, Tok::KeyFg, true);
                c.text(
                    content.x + 20,
                    y,
                    content.w.saturating_sub(20),
                    what,
                    Tok::Text,
                    false,
                );
                y += 1;
            }
        }
        Overlay::Grow => {
            let rect = modal(c, hits, "Grow case", 66, 10);
            let content = rect.content();
            let mut y = content.y + 1;
            for line in wrap("Freeze one more case ordinal. Applies at the next batch boundary; the next batch re-scores the archive. A case the module cannot evaluate ends the run with a fault.", content.w as usize) {
                c.text(content.x, y, content.w, &line, Tok::Text, false);
                y += 1;
            }
            y += 1;
            c.text(content.x, y, 6, "case ", Tok::Dim, false);
            let field = Rect::new(content.x + 6, y, 14, 1);
            c.tint(field, Tok::Select);
            c.text(field.x + 1, y, 12, &view.grow_input, Tok::Accent, true);
            c.put(
                field.x + 1 + view.grow_input.chars().count() as u16,
                y,
                '▏',
                Tok::Accent,
                false,
            );
            c.text(
                content.x + 24,
                y,
                content.w.saturating_sub(24),
                "⏎ send · esc cancel",
                Tok::Dim,
                false,
            );
        }
        Overlay::Stop => {
            let rect = modal(c, hits, "Stop the run", 66, 9);
            let content = rect.content();
            let mut y = content.y + 1;
            for line in wrap("Stop the run after the current batch? It can be resumed later with `emath dream resume`.", content.w as usize) {
                c.text(content.x, y, content.w, &line, Tok::Text, false);
                y += 1;
            }
            y += 1;
            c.text(
                content.x,
                y,
                content.w,
                "y stop · n cancel",
                Tok::Warn,
                true,
            );
        }
        Overlay::Fork => {
            let rect = modal(c, hits, "Fork the run", 66, 9);
            let content = rect.content();
            let mut y = content.y + 1;
            match ctx.checkpoints.last() {
                Some((batch, _, _)) => {
                    let text = format!("Fork from checkpoint b{batch} into `{}`. The fork runs detached; this view stays on {}.", ctx.fork_name, ctx.run_name);
                    for line in wrap(&text, content.w as usize) {
                        c.text(content.x, y, content.w, &line, Tok::Text, false);
                        y += 1;
                    }
                    y += 1;
                    c.text(
                        content.x,
                        y,
                        content.w,
                        "y fork · n cancel",
                        Tok::Accent,
                        true,
                    );
                }
                None => {
                    let text = format!("no checkpoint yet · first at b{}", model.checkpoint_every);
                    c.text(content.x, y, content.w, &text, Tok::Warn, false);
                }
            }
        }
    }
}
