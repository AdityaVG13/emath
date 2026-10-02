//! The dashboard's journal fold (SPEC §9 "bind"). A `RunModel` is a
//! pure function of the journal prefix it has ingested: the viewer
//! never computes mathematics, it only projects facts the driver
//! journaled. Every number shown traces to one journal event.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use emath_exec_ir::constructor_layer::CValue;

use super::canvas::Tok;
use crate::dream::journal::{JournalEvent, RecordView};
use crate::dream::export::display_value;

/// An exact rational `(num, den)` with `den > 0`.
pub type Rat = (i128, i128);

/// One archive record at its latest journaled state.
#[derive(Clone, Debug, Default)]
pub struct Rec {
    pub key: i128,
    fields: RecordView,
    /// The batch in which the record was first journaled (0 = seed).
    pub born: u64,
    /// The fields every ordering and state test reads, decoded once per
    /// journaled state instead of once per comparison.
    hot: Hot,
}

#[derive(Clone, Copy, Debug, Default)]
struct Hot {
    score: Option<Rat>,
    parent: Option<i128>,
    tier: i128,
    refusal: Option<i128>,
    counterexample: Option<i128>,
    accepted: bool,
}

impl Hot {
    fn of(fields: &RecordView) -> Self {
        let int = |name: &str| fields.get(name).and_then(cvalue_int);
        Hot {
            score: fields.get("score").and_then(cvalue_rat),
            parent: int("parent").filter(|p| *p >= 0),
            tier: int("tier").unwrap_or(1),
            refusal: int("refusal").filter(|c| *c > 0),
            counterexample: int("counterexample").filter(|c| *c >= 0),
            accepted: matches!(fields.get("accepted"), Some(CValue::Bool(true))),
        }
    }
}

fn cvalue_int(value: &CValue) -> Option<i128> {
    match value {
        CValue::Int(n) => n.to_i128(),
        CValue::Rat { num, den } if den.to_i128() == Some(1) => num.to_i128(),
        CValue::Bool(b) => Some(i128::from(*b)),
        _ => None,
    }
}

fn cvalue_rat(value: &CValue) -> Option<Rat> {
    match value {
        CValue::Int(n) => Some((n.to_i128()?, 1)),
        CValue::Rat { num, den } => {
            let (n, d) = (num.to_i128()?, den.to_i128()?);
            if d == 0 {
                None
            } else if d < 0 {
                Some((n.checked_neg()?, d.checked_neg()?))
            } else {
                Some((n, d))
            }
        }
        _ => None,
    }
}

impl Rec {
    pub fn from_view(fields: RecordView, born: u64) -> Option<Self> {
        let key = fields.get("key").and_then(cvalue_int)?;
        let hot = Hot::of(&fields);
        Some(Rec { key, fields, born, hot })
    }
    /// Replace the record's journaled state.
    fn set_fields(&mut self, fields: RecordView) {
        self.hot = Hot::of(&fields);
        self.fields = fields;
    }
    /// Every projected field, by name (SPEC §5.2).
    pub fn fields(&self) -> &RecordView {
        &self.fields
    }
    pub fn int(&self, name: &str) -> Option<i128> {
        self.fields.get(name).and_then(cvalue_int)
    }
    pub fn rat(&self, name: &str) -> Option<Rat> {
        self.fields.get(name).and_then(cvalue_rat)
    }
    /// Tier 1 = verified exact, 0 = probe; records without the field are exact.
    pub fn tier(&self) -> i128 {
        self.hot.tier
    }
    pub fn score(&self) -> Option<Rat> {
        self.hot.score
    }
    pub fn parent(&self) -> Option<i128> {
        self.hot.parent
    }
    /// The evaluator's refusal code. The archive contract
    /// (`language/modules/search/archive.emath`) uses 0 for "none", not
    /// the -1 sentinel of the ordinal fields.
    pub fn refusal(&self) -> Option<i128> {
        self.hot.refusal
    }
    /// The case ordinal that refuted the record, if any.
    pub fn counterexample(&self) -> Option<i128> {
        self.hot.counterexample
    }
    pub fn accepted(&self) -> bool {
        self.hot.accepted
    }
    /// The record's candidate value: `value` when authored, else the input id.
    pub fn value_text(&self) -> String {
        match self.fields.get("value") {
            Some(value) => display_value(value),
            None => self.int("input_id").map_or_else(|| "-".to_string(), |id| format!("in{id}")),
        }
    }
}

/// Record state (SPEC §9), first match wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecState {
    Incumbent,
    Refuted,
    Refused,
    Accepted,
    Valley,
    Probed,
}

impl RecState {
    pub fn glyph(self) -> char {
        match self {
            RecState::Incumbent => '●',
            RecState::Accepted => '✓',
            RecState::Valley => '◌',
            RecState::Refuted => '✗',
            RecState::Refused => '⊘',
            RecState::Probed => '~',
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            RecState::Incumbent => "incumbent",
            RecState::Accepted => "accepted",
            RecState::Valley => "valley",
            RecState::Refuted => "refuted",
            RecState::Refused => "refused",
            RecState::Probed => "probed",
        }
    }
    pub fn tok(self) -> Tok {
        match self {
            RecState::Incumbent => Tok::Accent,
            RecState::Accepted => Tok::Good,
            RecState::Valley => Tok::Violet,
            RecState::Refuted => Tok::Bad,
            RecState::Refused => Tok::Warn,
            RecState::Probed => Tok::Dim,
        }
    }
}

/// One committed batch.
#[derive(Clone, Debug)]
pub struct BatchRow {
    pub batch: u64,
    pub verdict: i128,
    pub used: i128,
    pub delta: i128,
    pub watermark: i128,
    pub plateau: u32,
    pub incumbent: i128,
    pub score: Option<Rat>,
    pub promoted: Vec<i128>,
    pub quarantined: Vec<i128>,
    pub cases: usize,
}

/// An event-rail mark (SPEC §9 Score row 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Promoted,
    Cases,
    Refuted,
    Budget,
    Steer,
}

impl Mark {
    pub fn glyph(self) -> (char, Tok) {
        match self {
            Mark::Promoted => ('▲', Tok::Good),
            Mark::Cases => ('◆', Tok::Info),
            Mark::Refuted => ('✗', Tok::Bad),
            Mark::Budget => ('$', Tok::Warn),
            Mark::Steer => ('✎', Tok::User),
        }
    }
}

/// One promotion: batch, key, score, gain over the previous incumbent.
#[derive(Clone, Debug)]
pub struct Promotion {
    pub batch: u64,
    pub key: i128,
    pub score: Option<Rat>,
    pub gain: Option<Rat>,
}

/// A closed level.
#[derive(Clone, Debug)]
pub struct LevelInfo {
    pub batch: u64,
    pub reason: String,
    pub key: Option<i128>,
    pub score: Option<Rat>,
    pub file: Option<String>,
    pub certificate: bool,
}

/// The journal's `end`.
#[derive(Clone, Debug)]
pub struct EndInfo {
    pub batch: u64,
    pub outcome: String,
    pub code: Option<String>,
}

/// One Events-panel line: `seq`, batch, and styled parts.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub seq: u64,
    pub batch: u64,
    pub parts: Vec<(String, Tok)>,
}

/// Per-family counts (SPEC §9 Families; `proposed`/`dup` need D1).
#[derive(Clone, Copy, Debug, Default)]
pub struct FamCount {
    pub admitted: u64,
    pub refused: u64,
    pub promoted: u64,
}

/// The fold of a journal prefix.
#[derive(Clone, Debug, Default)]
pub struct RunModel {
    pub started: bool,
    pub target: String,
    pub meaning_id: String,
    pub language_id: String,
    pub level: i128,
    pub budget0: i128,
    pub budget_step: i128,
    pub plateau_close: u64,
    pub max_batches: i128,
    pub max_units: Option<i128>,
    pub checkpoint_every: i128,
    pub emission: bool,
    pub seed_used: i128,
    pub seed_watermark: i128,
    pub seed_score: Option<Rat>,
    pub records: BTreeMap<i128, Rec>,
    pub first_seen: Vec<i128>,
    pub batches: Vec<BatchRow>,
    pub incumbent: Option<i128>,
    /// Watermark rungs: `(batch, watermark)`, the seed first.
    pub rungs: Vec<(u64, i128)>,
    pub case_ids: Vec<i128>,
    pub case_born: BTreeMap<i128, u64>,
    /// Case growth: `(batch, ids, by_user)`.
    pub growths: Vec<(u64, Vec<i128>, bool)>,
    pub steers: Vec<(u64, i128)>,
    pub promotions: Vec<Promotion>,
    /// `(batch, key)` of every quarantine.
    pub quarantines: Vec<(u64, i128)>,
    pub marks: Vec<(u64, Mark)>,
    pub level_info: Option<LevelInfo>,
    pub end: Option<EndInfo>,
    pub log: Vec<LogLine>,
    /// The newest raw journal lines (for the Journal panel).
    pub raw: VecDeque<(u64, String)>,
    pub last_seq: u64,
    pub family_batch: (u64, BTreeMap<i128, FamCount>),
    pub family_run: BTreeMap<i128, FamCount>,
    pending_added: Vec<i128>,
    /// Record orderings derived from the fold; reset by every ingest and
    /// rebuilt on first use (SPEC §9: recomputed when new events arrive,
    /// never per frame).
    ranked: OnceCell<Ranked>,
}

/// The shared record list's groups and the parent → children index,
/// each group in its display order.
#[derive(Clone, Debug, Default)]
struct Ranked {
    accepted: Vec<i128>,
    valley: Vec<i128>,
    probed: Vec<i128>,
    /// Newest first.
    refuted: Vec<i128>,
    /// Newest first.
    refused: Vec<i128>,
    children: BTreeMap<i128, Vec<i128>>,
}

/// Raw journal lines kept for the Journal panel.
pub const RAW_KEEP: usize = 600;

impl RunModel {
    pub fn new() -> Self {
        RunModel { level: 1, plateau_close: 3, ..RunModel::default() }
    }

    pub fn fold_all(events: &[JournalEvent]) -> Self {
        let mut model = RunModel::new();
        for event in events {
            model.ingest(event);
        }
        model
    }

    pub fn last_batch(&self) -> u64 {
        self.batches.last().map_or(0, |b| b.batch)
    }

    pub fn used(&self) -> i128 {
        self.batches.last().map_or(self.seed_used, |b| b.used)
    }

    pub fn watermark(&self) -> i128 {
        self.rungs.last().map_or(self.seed_watermark, |r| r.1)
    }

    pub fn plateau_streak(&self) -> u32 {
        self.batches.last().map_or(0, |b| b.plateau)
    }

    /// The plateau streak toward `plateau_close` (SPEC §7.2): one dot
    /// per step while the close is short enough to draw, else
    /// `streak/close` (the loop's own numbers, never capped).
    pub fn plateau_dots(&self) -> String {
        let close = self.plateau_close;
        let streak = u64::from(self.plateau_streak()).min(close);
        if close <= 8 {
            format!("{}{}", "●".repeat(streak as usize), "○".repeat((close - streak) as usize))
        } else {
            format!("{streak}/{close}")
        }
    }

    pub fn incumbent_rec(&self) -> Option<&Rec> {
        self.incumbent.and_then(|k| self.records.get(&k))
    }

    pub fn state_of(&self, rec: &Rec) -> RecState {
        if Some(rec.key) == self.incumbent {
            RecState::Incumbent
        } else if rec.counterexample().is_some() {
            RecState::Refuted
        } else if rec.refusal().is_some() {
            RecState::Refused
        } else if rec.accepted() {
            RecState::Accepted
        } else if rec.tier() == 1 {
            RecState::Valley
        } else {
            RecState::Probed
        }
    }

    fn upsert(&mut self, view: RecordView, batch: u64) -> Option<i128> {
        let key = view.get("key").and_then(cvalue_int)?;
        match self.records.get_mut(&key) {
            Some(rec) => rec.set_fields(view),
            None => {
                self.records.insert(key, Rec::from_view(view, batch)?);
                self.first_seen.push(key);
            }
        }
        Some(key)
    }

    fn log(&mut self, event: &JournalEvent, batch: u64, parts: Vec<(String, Tok)>) {
        self.log.push(LogLine { seq: event.seq, batch, parts });
    }

    /// Fold one journal event.
    pub fn ingest(&mut self, event: &JournalEvent) {
        self.ranked = OnceCell::new();
        self.last_seq = event.seq;
        self.raw.push_back((event.seq, abbreviate(event)));
        while self.raw.len() > RAW_KEEP {
            self.raw.pop_front();
        }
        let batch = event.batch().unwrap_or(0);
        match event.t.as_str() {
            "start" => {
                self.started = true;
                self.target = event.str("target").unwrap_or_default().to_string();
                self.meaning_id = event.str("meaning_id").unwrap_or_default().to_string();
                self.language_id = event.str("language_id").unwrap_or_default().to_string();
                self.level = event.int("level").unwrap_or(1);
                self.emission = event.bool("emission").unwrap_or(false);
                self.budget0 = event.nested_int("config", "budget0").unwrap_or(0);
                self.budget_step = event.nested_int("config", "budget_step").unwrap_or(0);
                self.plateau_close = u64::try_from(event.nested_int("config", "plateau_close").unwrap_or(3)).unwrap_or(u64::MAX).max(1);
                self.max_batches = event.nested_int("config", "max_batches").unwrap_or(0);
                self.max_units = event.nested_int("config", "max_units");
                self.checkpoint_every = event.nested_int("config", "checkpoint_every").unwrap_or(25);
            }
            "seed" => {
                self.seed_used = event.int("used").unwrap_or(0);
                self.seed_watermark = event.int("watermark").unwrap_or(0);
                self.rungs = vec![(0, self.seed_watermark)];
                for view in event.records("archive") {
                    self.upsert(view, 0);
                }
                if let Some(view) = event.record("incumbent") {
                    self.incumbent = self.upsert(view, 0);
                }
                self.seed_score = self.incumbent_rec().and_then(Rec::score);
                self.case_ids = event.ints("case_ids");
                for id in self.case_ids.clone() {
                    self.case_born.insert(id, 0);
                }
            }
            "records" => {
                self.pending_added.clear();
                for view in event.records("added") {
                    if let Some(key) = self.upsert(view, batch) {
                        self.pending_added.push(key);
                    }
                }
                for view in event.records("changed") {
                    self.upsert(view, batch);
                }
                let mut counts: BTreeMap<i128, FamCount> = BTreeMap::new();
                for key in self.pending_added.clone() {
                    let Some(rec) = self.records.get(&key) else { continue };
                    let family = rec.int("family").unwrap_or(-1);
                    let refused = rec.refusal().is_some();
                    for slot in [counts.entry(family).or_default(), self.family_run.entry(family).or_default()] {
                        slot.admitted += 1;
                        slot.refused += u64::from(refused);
                    }
                }
                self.family_batch = (batch, counts);
            }
            "cases" => {
                let added = event.ints("added");
                self.note_cases(batch, &added, false);
                let mut parts = vec![(format!("◆ cases +{} ", added.len()), Tok::Info)];
                parts.push((format!("({}) ", added.iter().map(|c| format!("c{c}")).collect::<Vec<_>>().join(" ")), Tok::Text));
                parts.push((format!("· audit now {} · archive re-verified", self.case_ids.len()), Tok::Dim));
                self.log(event, batch, parts);
            }
            "steer" => {
                if let Some(id) = event.int("case_id") {
                    self.steers.push((batch, id));
                    self.note_cases(batch, &[id], true);
                    self.marks.push((batch, Mark::Steer));
                    let parts = vec![
                        ("✎ you grew the audit: ".to_string(), Tok::User),
                        (format!("c{id}"), Tok::Text),
                        (" · recorded in journal".to_string(), Tok::Dim),
                    ];
                    self.log(event, batch, parts);
                }
            }
            "batch" => self.ingest_batch(event, batch),
            "budget" => {
                let (from, to) = (event.int("from").unwrap_or(0), event.int("to").unwrap_or(0));
                self.rungs.push((batch, to));
                self.marks.push((batch, Mark::Budget));
                let parts = vec![
                    (format!("$ budget {} → {}", units(from), units(to)), Tok::Warn),
                    (" · batch committed partial, resumed at 2·used + step".to_string(), Tok::Dim),
                ];
                self.log(event, batch, parts);
            }
            "level" => {
                let incumbent = event.record("incumbent");
                let key = incumbent.as_ref().and_then(|r| r.get("key")).and_then(cvalue_int);
                let score = incumbent.as_ref().and_then(|r| r.get("score")).and_then(cvalue_rat);
                let info = LevelInfo {
                    batch,
                    reason: event.str("reason").unwrap_or_default().to_string(),
                    key,
                    score,
                    file: event.str("file").map(str::to_string),
                    certificate: event.bool("certificate").unwrap_or(false),
                };
                let mut parts = vec![(format!("■ level closed · {}", info.reason), Tok::Accent2)];
                if let Some(key) = key {
                    parts.push((format!(" · #{key} {}", score.map(rat_text).unwrap_or_default()), Tok::Text));
                }
                match &info.file {
                    Some(file) => {
                        parts.push((format!(" · {file}"), Tok::Dim));
                        parts.push(if info.certificate {
                            (" ✓ certificate".to_string(), Tok::Good)
                        } else {
                            (" ✗ certificate failed".to_string(), Tok::Bad)
                        });
                    }
                    None => parts.push((" · no emission".to_string(), Tok::Dim)),
                }
                self.level_info = Some(info);
                self.log(event, batch, parts);
            }
            "end" => {
                let end = EndInfo {
                    batch,
                    outcome: event.str("outcome").unwrap_or_default().to_string(),
                    code: event.str("code").map(str::to_string),
                };
                let (text, tok) = ended_banner(self, &end);
                self.end = Some(end);
                self.log(event, batch, vec![(text, tok)]);
            }
            _ => {}
        }
    }

    fn note_cases(&mut self, batch: u64, ids: &[i128], by_user: bool) {
        for id in ids {
            if !self.case_ids.contains(id) {
                self.case_ids.push(*id);
            }
            self.case_born.entry(*id).or_insert(batch);
        }
        self.growths.push((batch, ids.to_vec(), by_user));
        if !by_user {
            self.marks.push((batch, Mark::Cases));
        }
    }

    fn ingest_batch(&mut self, event: &JournalEvent, batch: u64) {
        let previous = self.incumbent;
        let previous_score = self.batches.last().map_or(self.seed_score, |b| b.score);
        if let Some(view) = event.record("incumbent") {
            self.incumbent = self.upsert(view, batch);
        }
        let promoted = event.ints("promoted");
        let quarantined = event.ints("quarantined");
        let row = BatchRow {
            batch,
            verdict: event.int("verdict").unwrap_or(-1),
            used: event.int("used").unwrap_or(0),
            delta: event.int("delta_used").unwrap_or(0),
            watermark: event.int("watermark").unwrap_or(0),
            plateau: event.uint("plateau_streak").unwrap_or(0) as u32,
            incumbent: self.incumbent.unwrap_or(-1),
            score: self.incumbent_rec().and_then(Rec::score),
            promoted: promoted.clone(),
            quarantined: quarantined.clone(),
            cases: event.ints("case_ids").len(),
        };
        let case_ids = event.ints("case_ids");
        if !case_ids.is_empty() {
            self.case_ids = case_ids;
        }
        for key in &promoted {
            let rec = self.records.get(key).cloned();
            let score = rec.as_ref().and_then(Rec::score);
            let gain = match (score, previous_score) {
                (Some(now), Some(before)) if previous != Some(*key) => rat_sub(now, before),
                _ => None,
            };
            self.promotions.push(Promotion { batch, key: *key, score, gain });
            self.marks.push((batch, Mark::Promoted));
            let family = rec.as_ref().and_then(|r| r.int("family"));
            if let Some(family) = family {
                self.family_run.entry(family).or_default().promoted += 1;
                if self.family_batch.0 == batch {
                    self.family_batch.1.entry(family).or_default().promoted += 1;
                }
            }
            let mut parts = vec![(format!("▲ promoted #{key}"), Tok::Good)];
            if let Some(parent) = rec.as_ref().and_then(Rec::parent) {
                parts.push((format!(" ← #{parent}"), Tok::Text));
            }
            if let Some(rec) = &rec {
                parts.push((format!("  {}", fam_move(rec)), Tok::Dim));
            }
            if let Some(score) = score {
                parts.push((format!("  {}", rat_text(score)), Tok::Text));
                parts.push((format!("  ({})", dec4(score)), Tok::Dim));
            }
            self.log(event, batch, parts);
        }
        for key in &quarantined {
            self.quarantines.push((batch, *key));
            self.marks.push((batch, Mark::Refuted));
            let ce = self.records.get(key).and_then(|r| r.int("counterexample")).unwrap_or(-1);
            let parts = vec![
                (format!("✗ refuted #{key} by c{ce}"), Tok::Bad),
                (" · acceptance withdrawn, record kept".to_string(), Tok::Dim),
            ];
            self.log(event, batch, parts);
        }
        if row.verdict == 3 {
            let parts = vec![
                ("○ plateau".to_string(), Tok::Plateau),
                (format!(" · no promotion · {}/{} toward close", row.plateau, self.plateau_close), Tok::Dim),
            ];
            self.log(event, batch, parts);
        }
        self.batches.push(row);
    }

    /// Whether the loop minimizes its score: every nonzero promotion
    /// gain so far is negative (display ordering only; default maximize).
    pub fn minimizing(&self) -> bool {
        let gains: Vec<Rat> = self.promotions.iter().filter_map(|p| p.gain).filter(|g| g.0 != 0).collect();
        !gains.is_empty() && gains.iter().all(|g| g.0 < 0)
    }

    /// The shared record list (SPEC §9) for `n` visible rows; `full`
    /// lists everything (expanded panels), probed records last. Rows
    /// the capped tail leaves empty are filled with further refuted,
    /// refused and probed records, newest first.
    pub fn record_list(&self, n: usize, full: bool) -> Vec<&Rec> {
        let ranked = self.ranked();
        let recs = |keys: &'_ [i128]| keys.iter().filter_map(|k| self.records.get(k)).collect::<Vec<&Rec>>();
        let incumbent: Vec<&Rec> = self.incumbent_rec().into_iter().collect();
        if full {
            let mut all = incumbent;
            for group in [&ranked.accepted, &ranked.valley, &ranked.refuted, &ranked.refused, &ranked.probed] {
                all.extend(recs(group));
            }
            return all;
        }
        let refuted = recs(&ranked.refuted[..ranked.refuted.len().min((n / 6).max(1))]);
        let refused = recs(&ranked.refused[..ranked.refused.len().min(1)]);
        let tail = refuted.len() + refused.len();
        let main_cap = n.saturating_sub(tail);
        let mut main: Vec<&Rec> = incumbent
            .into_iter()
            .chain(ranked.accepted.iter().chain(&ranked.valley).filter_map(|k| self.records.get(k)))
            .take(main_cap)
            .collect();
        let fill = n.saturating_sub(main.len() + tail);
        let seen: BTreeSet<i128> = main.iter().chain(refuted.iter()).chain(refused.iter()).map(|r| r.key).collect();
        let extra: Vec<&Rec> = ranked
            .refuted
            .iter()
            .chain(&ranked.refused)
            .chain(&ranked.probed)
            .filter(|k| !seen.contains(k))
            .filter_map(|k| self.records.get(k))
            .take(fill)
            .collect();
        main.extend(refuted);
        main.extend(refused);
        main.extend(extra);
        main.truncate(n);
        main
    }

    /// Records whose parent is `key`, in key order.
    pub fn children(&self, key: i128) -> Vec<&Rec> {
        self.ranked().children.get(&key).map_or_else(Vec::new, |keys| keys.iter().filter_map(|k| self.records.get(k)).collect())
    }

    fn ranked(&self) -> &Ranked {
        self.ranked.get_or_init(|| {
            // Scores are read once per record, not once per comparison.
            let minimizing = self.minimizing();
            let mut ranked = Ranked::default();
            let mut groups: [Vec<(Option<Rat>, i128)>; 3] = Default::default();
            for rec in self.records.values() {
                let slot = match self.state_of(rec) {
                    RecState::Accepted => 0,
                    RecState::Valley => 1,
                    RecState::Probed => 2,
                    _ => continue,
                };
                groups[slot].push((rec.score(), rec.key));
            }
            for group in &mut groups {
                group.sort_by(|a, b| {
                    let order = rat_cmp_opt(b.0, a.0);
                    (if minimizing { order.reverse() } else { order }).then(a.1.cmp(&b.1))
                });
            }
            let [accepted, valley, probed] = groups;
            ranked.accepted = accepted.into_iter().map(|g| g.1).collect();
            ranked.valley = valley.into_iter().map(|g| g.1).collect();
            ranked.probed = probed.into_iter().map(|g| g.1).collect();
            for key in self.first_seen.iter().rev() {
                let Some(rec) = self.records.get(key) else { continue };
                match self.state_of(rec) {
                    RecState::Refuted => ranked.refuted.push(*key),
                    RecState::Refused => ranked.refused.push(*key),
                    _ => {}
                }
            }
            for rec in self.records.values() {
                if let Some(parent) = rec.parent() {
                    ranked.children.entry(parent).or_default().push(rec.key);
                }
            }
            ranked
        })
    }
}

/// The ended-banner text and color (SPEC §7.8 / §8.3).
pub fn ended_banner(model: &RunModel, end: &EndInfo) -> (String, Tok) {
    let (badge, tok) = outcome_badge(&end.outcome, end.code.as_deref());
    let detail = match end.outcome.as_str() {
        "fault" => {
            // The badge already names the code. A fault in the batch right
            // after a steer is most likely the steer's doing (a case the
            // module cannot evaluate): say so.
            match model.steers.last() {
                Some((batch, id)) if end.batch == *batch => format!("after your grow c{id} at b{batch}"),
                _ => format!("at b{}", end.batch),
            }
        }
        "stopped" => format!("stopped by you at b{}", end.batch),
        "batch_cap" => format!("batch cap at b{}", end.batch),
        "budget_exhausted" => format!("unit ceiling at b{}", end.batch),
        _ => match &model.level_info {
            Some(LevelInfo { file: Some(file), certificate, .. }) => {
                format!("{file} {}", if *certificate { "✓ certificate" } else { "✗ certificate failed" })
            }
            Some(_) => "no emission seam".into(),
            None => format!("b{}", end.batch),
        },
    };
    (format!("■ RUN ENDED · {badge} · {detail}"), tok)
}

/// The outcome badge (SPEC §8.3).
pub fn outcome_badge(outcome: &str, code: Option<&str>) -> (String, Tok) {
    match outcome {
        "goal_attained" => ("◆ GOAL".into(), Tok::Accent),
        "domain_exhausted" => ("◇ DOMAIN EXHAUSTED".into(), Tok::Violet),
        "plateau" => ("○ PLATEAU".into(), Tok::Plateau),
        "budget_exhausted" => ("$ BUDGET".into(), Tok::Warn),
        "fault" => (format!("✗ FAULT {}", code.unwrap_or("")).trim_end().to_string(), Tok::Bad),
        "stopped" => ("■ STOPPED".into(), Tok::Dim),
        "batch_cap" => ("■ BATCH CAP".into(), Tok::Dim),
        other => (other.to_uppercase(), Tok::Dim),
    }
}

/// The verdict color and name (SPEC §8.3).
pub fn verdict_tok(verdict: i128) -> Tok {
    match verdict {
        0 => Tok::Good,
        1 => Tok::Accent,
        2 => Tok::Violet,
        3 => Tok::Plateau,
        4 => Tok::Warn,
        _ => Tok::Bad,
    }
}

pub fn verdict_label(verdict: i128) -> &'static str {
    match verdict {
        0 => "running",
        1 => "goal",
        2 => "domain",
        3 => "plateau",
        4 => "budget",
        _ => "unknown",
    }
}

/// `f<id> m<id>` of a record.
pub fn fam_move(rec: &Rec) -> String {
    let f = rec.int("family").filter(|f| *f >= 0).map_or_else(|| "f-".to_string(), |f| format!("f{f}"));
    let m = rec.int("move").filter(|m| *m >= 0).map_or_else(|| "m-".to_string(), |m| format!("m{m}"));
    format!("{f} {m}")
}

/// `n/d`, or `n` when `d = 1`.
pub fn rat_text(r: Rat) -> String {
    if r.1 == 1 { r.0.to_string() } else { format!("{}/{}", r.0, r.1) }
}

/// A signed exact gain: `+x` or `-x`, 4 places.
pub fn signed4(r: Rat) -> String {
    let text = dec4(r);
    if text.starts_with('-') { text } else { format!("+{text}") }
}

/// A promotion's gain with its direction: `▲ +x` when the score rose,
/// `▼ -x` when it fell (a minimizing module's improvement).
pub fn gain_text(r: Rat) -> String {
    let text = signed4(r);
    if text.starts_with('-') { format!("▼ {text}") } else { format!("▲ {text}") }
}

/// 4-place decimal computed exactly (round half away from zero).
pub fn dec4(r: Rat) -> String {
    dec_places(r, 4)
}

/// `places`-place decimal computed exactly from the rational.
pub fn dec_places(r: Rat, places: u32) -> String {
    let scale = 10i128.pow(places);
    let (n, d) = r;
    let negative = (n < 0) != (d < 0) && n != 0;
    let (an, ad) = (n.unsigned_abs(), d.unsigned_abs());
    let exact = an
        .checked_mul(scale as u128)
        .and_then(|v| v.checked_mul(2))
        .and_then(|v| v.checked_add(ad))
        .map(|v| v / (2 * ad));
    match exact {
        Some(q) => {
            let sign = if negative && q != 0 { "-" } else { "" };
            let whole = q / scale as u128;
            let frac = q % scale as u128;
            if places == 0 { format!("{sign}{whole}") } else { format!("{sign}{whole}.{frac:0width$}", width = places as usize) }
        }
        None => format!("{:.*}", places as usize, n as f64 / d as f64),
    }
}

/// Probe-tier rendering: `≈` + 3 places.
pub fn approx(r: Rat) -> String {
    format!("≈{}", dec_places(r, 3))
}

/// Exact difference `a − b`.
pub fn rat_sub(a: Rat, b: Rat) -> Option<Rat> {
    let n = a.0.checked_mul(b.1)?.checked_sub(b.0.checked_mul(a.1)?)?;
    let d = a.1.checked_mul(b.1)?;
    let g = gcd(n.unsigned_abs(), d.unsigned_abs()).max(1) as i128;
    Some((n / g, d / g))
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Exact comparison of two rationals (denominators positive).
pub fn rat_cmp(a: Rat, b: Rat) -> std::cmp::Ordering {
    match (a.0.checked_mul(b.1), b.0.checked_mul(a.1)) {
        (Some(x), Some(y)) => x.cmp(&y),
        _ => (a.0 as f64 / a.1 as f64).total_cmp(&(b.0 as f64 / b.1 as f64)),
    }
}

fn rat_cmp_opt(a: Option<Rat>, b: Option<Rat>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => rat_cmp(a, b),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Plotting-only float of a rational (never shown as a number).
pub fn rat_f64(r: Rat) -> f64 {
    r.0 as f64 / r.1 as f64
}

/// Unit counts (SPEC §8.2).
pub fn units(n: i128) -> String {
    let a = n.unsigned_abs();
    let sign = if n < 0 { "-" } else { "" };
    if a < 10_000 {
        let s = a.to_string();
        if a >= 1000 { format!("{sign}{},{}", &s[..s.len() - 3], &s[s.len() - 3..]) } else { format!("{sign}{s}") }
    } else if a < 1_000_000 {
        format!("{sign}{:.1}k", a as f64 / 1000.0)
    } else {
        // From a million up: two places under the largest suffix that
        // keeps the mantissa below 1000 (M, G, T), then scientific.
        let v = a as f64;
        for (scale, suffix) in [(1e6, "M"), (1e9, "G"), (1e12, "T")] {
            if v < scale * 1000.0 {
                return format!("{sign}{:.2}{suffix}", v / scale);
            }
        }
        format!("{sign}{v:.2e}")
    }
}

/// A journal line with `records` arrays abbreviated to `{key, score, family}`.
fn abbreviate(event: &JournalEvent) -> String {
    if event.t != "records" && event.t != "seed" {
        return event.line.clone();
    }
    let short = |views: Vec<RecordView>| -> String {
        let items: Vec<String> = views
            .iter()
            .map(|r| {
                let get = |k: &str| r.get(k).map_or_else(|| "null".to_string(), display_value);
                format!("{{\"key\":{},\"score\":\"{}\",\"family\":{}}}", get("key"), get("score"), get("family"))
            })
            .collect();
        format!("[{}]", items.join(","))
    };
    if event.t == "records" {
        format!(
            "{{\"seq\":{},\"t\":\"records\",\"batch\":{},\"added\":{},\"changed\":{}}}",
            event.seq,
            event.batch().unwrap_or(0),
            short(event.records("added")),
            short(event.records("changed"))
        )
    } else {
        format!(
            "{{\"seq\":{},\"t\":\"seed\",\"batch\":0,\"used\":{},\"watermark\":{},\"archive\":{}}}",
            event.seq,
            event.int("used").unwrap_or(0),
            event.int("watermark").unwrap_or(0),
            short(event.records("archive"))
        )
    }
}
