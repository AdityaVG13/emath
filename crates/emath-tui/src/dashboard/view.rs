//! The viewer's local state (SPEC §7, §10): tab, focus, expansion,
//! overlays, hidden panels, theme, selection, scrolling. None of it is
//! persisted and none of it reaches the journal.

use std::cell::Cell as StdCell;
use std::collections::BTreeSet;

use super::canvas::Tok;

/// The eleven panels (SPEC §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PanelId {
    Score,
    Budget,
    Frontier,
    Heat,
    Verdicts,
    Families,
    Events,
    Lineage,
    Refuted,
    Evidence,
    Journal,
}

pub const ALL_PANELS: [PanelId; 11] = [
    PanelId::Score,
    PanelId::Budget,
    PanelId::Frontier,
    PanelId::Heat,
    PanelId::Verdicts,
    PanelId::Families,
    PanelId::Events,
    PanelId::Lineage,
    PanelId::Refuted,
    PanelId::Evidence,
    PanelId::Journal,
];

/// A panel's rendering cost class (SPEC §10.2 Panels overlay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cost {
    Light,
    Medium,
    Heavy,
}

impl PanelId {
    pub fn index(self) -> usize {
        ALL_PANELS.iter().position(|p| *p == self).unwrap_or(0)
    }
    pub fn name(self) -> &'static str {
        match self {
            PanelId::Score => "score",
            PanelId::Budget => "budget",
            PanelId::Frontier => "frontier",
            PanelId::Heat => "heat",
            PanelId::Verdicts => "verdicts",
            PanelId::Families => "families",
            PanelId::Events => "events",
            PanelId::Lineage => "lineage",
            PanelId::Refuted => "refuted",
            PanelId::Evidence => "evidence",
            PanelId::Journal => "journal",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        ALL_PANELS.iter().copied().find(|p| p.name() == name)
    }
    pub fn title(self) -> &'static str {
        match self {
            PanelId::Score => "Incumbent score",
            PanelId::Budget => "Budget ladder",
            PanelId::Frontier => "Frontier",
            PanelId::Heat => "Candidates × cases",
            PanelId::Verdicts => "Verdicts",
            PanelId::Families => "Move families",
            PanelId::Events => "Events",
            PanelId::Lineage => "Lineage",
            PanelId::Refuted => "Refuted",
            PanelId::Evidence => "Evidence",
            PanelId::Journal => "journal.jsonl",
        }
    }
    pub fn cost(self) -> Cost {
        match self {
            PanelId::Heat | PanelId::Lineage => Cost::Heavy,
            PanelId::Score | PanelId::Frontier | PanelId::Journal => Cost::Medium,
            _ => Cost::Light,
        }
    }
    /// Minimum content size (cols, rows), SPEC §7.6.
    pub fn min(self) -> (u16, u16) {
        match self {
            PanelId::Score => (60, 10),
            PanelId::Budget => (40, 10),
            PanelId::Frontier => (60, 8),
            PanelId::Heat => (36, 8),
            PanelId::Verdicts => (30, 6),
            PanelId::Families => (36, 6),
            PanelId::Events => (50, 4),
            PanelId::Lineage => (36, 8),
            PanelId::Refuted => (32, 6),
            PanelId::Evidence => (50, 12),
            PanelId::Journal => (60, 6),
        }
    }
    pub fn describe(self) -> &'static str {
        match self {
            PanelId::Score => "incumbent score per batch, event rail, incumbent card",
            PanelId::Budget => "watermark ladder, spend per batch",
            PanelId::Frontier => "incumbent, accepted and valley records, refutations",
            PanelId::Heat => "records × audit cases (colors need D2)",
            PanelId::Verdicts => "one cell per batch, plateau streak",
            PanelId::Families => "admitted, refused, promoted per move family",
            PanelId::Events => "journal events as sentences",
            PanelId::Lineage => "parent chain of the selected record",
            PanelId::Refuted => "withdrawn records and the case that refuted them",
            PanelId::Evidence => "level certificate, run identity, checkpoints",
            PanelId::Journal => "raw journal.jsonl, tail-following",
        }
    }
}

pub const TAB_NAMES: [&str; 4] = ["Overview", "Search", "Evidence", "Journal"];

/// A tab's rows: `(height weight, [(panel, width weight)])` (SPEC §7.4).
pub fn tab_rows(tab: usize) -> Vec<(u32, Vec<(PanelId, u32)>)> {
    use PanelId::*;
    match tab {
        0 => vec![
            (14, vec![(Score, 64), (Budget, 36)]),
            (15, vec![(Frontier, 63), (Heat, 37)]),
            (10, vec![(Verdicts, 25), (Families, 28), (Events, 47)]),
        ],
        1 => vec![(24, vec![(Frontier, 62), (Heat, 38)]), (15, vec![(Lineage, 34), (Families, 36), (Refuted, 30)])],
        2 => vec![(22, vec![(Evidence, 55), (Refuted, 45)]), (17, vec![(Lineage, 45), (Budget, 55)])],
        _ => vec![(25, vec![(Journal, 1)]), (14, vec![(Events, 1)])],
    }
}

/// The panels of a tab in reading order.
pub fn tab_panels(tab: usize) -> Vec<PanelId> {
    tab_rows(tab).into_iter().flat_map(|(_, cols)| cols.into_iter().map(|(p, _)| p)).collect()
}

/// Modal overlays (SPEC §7.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Panels,
    Keys,
    Grow,
    Stop,
    Fork,
}

/// The RUN tile's state (SPEC §6.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunBadge {
    Starting,
    Running,
    Paused(u64),
    Pausing,
    Stopping,
    Stalled,
    Ended { outcome: String, code: Option<String> },
}

impl RunBadge {
    pub fn text(&self) -> (String, Tok) {
        match self {
            RunBadge::Starting => ("○ STARTING".into(), Tok::Dim),
            RunBadge::Running => ("● RUNNING".into(), Tok::Good),
            RunBadge::Paused(b) => (format!("❚❚ PAUSED b{b}"), Tok::Warn),
            RunBadge::Pausing => ("⋯ PAUSING".into(), Tok::Warn),
            RunBadge::Stopping => ("⋯ STOPPING".into(), Tok::Warn),
            RunBadge::Stalled => ("! STALLED".into(), Tok::Bad),
            RunBadge::Ended { outcome, code } => super::model::outcome_badge(outcome, code.as_deref()),
        }
    }
    pub fn dot(&self) -> Tok {
        match self {
            RunBadge::Running => Tok::Good,
            RunBadge::Paused(_) | RunBadge::Pausing | RunBadge::Stopping => Tok::Warn,
            RunBadge::Stalled => Tok::Bad,
            RunBadge::Ended { outcome, .. } if outcome == "fault" => Tok::Bad,
            _ => Tok::Dim,
        }
    }
    pub fn ended(&self) -> bool {
        matches!(self, RunBadge::Ended { .. })
    }
}

/// Everything the renderer reads that is not a journal fact: the run's
/// context and the viewer's measurements (`None` renders `—`).
#[derive(Clone, Debug)]
pub struct Ctx {
    pub run_name: String,
    pub module_file: String,
    pub badge: RunBadge,
    pub elapsed_secs: Option<u64>,
    pub bps: Option<f64>,
    pub frame_ms: Option<f64>,
    /// `(batch, file name, bytes)`, ascending.
    pub checkpoints: Vec<(u64, String, u64)>,
    /// Newest toast: text, color, remaining fraction in (0, 1].
    pub toast: Option<(String, Tok, f32)>,
    /// Journal seqs newer than 1.4 s (flash highlight).
    pub fresh: BTreeSet<u64>,
    pub fork_name: String,
    /// A request awaiting its ack (`pause`, `grow_case` …).
    pub pending: Option<String>,
}

impl Ctx {
    /// A measurement-free context (`--no-measure`, goldens).
    pub fn headless(run_name: &str, module_file: &str, badge: RunBadge) -> Self {
        Ctx {
            run_name: run_name.into(),
            module_file: module_file.into(),
            badge,
            elapsed_secs: None,
            bps: None,
            frame_ms: None,
            checkpoints: Vec::new(),
            toast: None,
            fresh: BTreeSet::new(),
            fork_name: String::new(),
            pending: None,
        }
    }
}

/// The viewer's state.
#[derive(Debug)]
pub struct View {
    pub tab: usize,
    pub focus: Option<PanelId>,
    pub expanded: Option<PanelId>,
    pub overlay: Overlay,
    pub hidden: BTreeSet<PanelId>,
    pub theme: usize,
    /// Selection follows the incumbent (SPEC §9 Selection).
    pub follow: bool,
    pub selected: Option<i128>,
    /// `None` = follow the tail; `Some(n)` = lines scrolled up from the tail.
    pub events_scroll: Option<usize>,
    pub journal_scroll: Option<usize>,
    pub grow_input: String,
    /// Derived-data computations per panel (SPEC §15 A9).
    pub computed: [StdCell<u64>; 11],
}

impl Default for View {
    fn default() -> Self {
        View {
            tab: 0,
            focus: None,
            expanded: None,
            overlay: Overlay::None,
            hidden: BTreeSet::new(),
            theme: 0,
            follow: true,
            selected: None,
            events_scroll: None,
            journal_scroll: None,
            grow_input: String::new(),
            computed: Default::default(),
        }
    }
}

impl View {
    pub fn computed(&self, panel: PanelId) -> u64 {
        self.computed[panel.index()].get()
    }
    pub(crate) fn count(&self, panel: PanelId) {
        let cell = &self.computed[panel.index()];
        cell.set(cell.get() + 1);
    }
    /// Visible panels of the current tab (hidden ones removed).
    pub fn visible(&self) -> Vec<PanelId> {
        tab_panels(self.tab).into_iter().filter(|p| !self.hidden.contains(p)).collect()
    }
    /// Hide or show a panel; the last visible panel of a tab stays.
    pub fn toggle_hidden(&mut self, panel: PanelId) {
        if self.hidden.contains(&panel) {
            self.hidden.remove(&panel);
            return;
        }
        self.hidden.insert(panel);
        for tab in 0..TAB_NAMES.len() {
            if tab_panels(tab).iter().all(|p| self.hidden.contains(p)) {
                self.hidden.remove(&panel);
                return;
            }
        }
        if self.expanded == Some(panel) {
            self.expanded = None;
        }
    }
}
