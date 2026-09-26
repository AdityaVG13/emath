//! `emath dream` viewer acceptance (bead emath-3vium, SPEC §15 A8-A10,
//! A13): the dashboard is a pure fold + render of the journal, pinned
//! by text and style-class goldens at the reference size and at each
//! size tier; the spacing grid holds on every golden; hidden panels
//! cost nothing; a dead driver shows as stalled, never as paused.
//!
//! Re-bless (fixture journal + goldens) with `EMATH_DREAM_BLESS=1`.

use std::path::{Path, PathBuf};

use emath_test_harness::Probe;
use emath_tui::dashboard::canvas::{color, Glyphs, Tok, THEMES};
use emath_tui::dashboard::input::{on_click, on_key, on_wheel, Action, Key};
use emath_tui::dashboard::model::RunModel;
use emath_tui::dashboard::render::{body_rect, render, Frame, HitAct, Mode};
use emath_tui::dashboard::view::{Ctx, Overlay, PanelId, RunBadge, View, ALL_PANELS};
use emath_tui::dashboard::watch::{derive_badge, RunWatch};
use emath_tui::dream::journal::{read_events, JournalEvent};
use emath_tui::dream::rundir::{create_run, send_request, Ack, Driver, Request, RunConfig, RunDir};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

fn bless() -> bool {
    std::env::var_os("EMATH_DREAM_BLESS").is_some()
}

const FIXTURE: &str = "tests/fixtures/dream/linear_steered.jsonl";
const RUN: &str = "stepdreamfamilies-001";

/// The fixture journal: dream_world_linear.emath's two-family trickle
/// surface (StepDreamFamilies, 16/16), the audit grown by hand to c4
/// after b2: a steered run with budget raises, promotions, refutations,
/// audit growth, plateaus and a moving incumbent score that closes on
/// plateau with a level certificate.
fn regenerate_fixture() {
    let root = std::env::temp_dir().join(format!("emath_dream_fixture_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("root");
    let mut config = RunConfig::new(repo("tests/fixtures/constructor/dream_world_linear.emath"));
    config.target = Some("StepDreamFamilies".into());
    config.budget0 = 16;
    config.budget_step = 16;
    let run = create_run(&root, Some("fixture"), true, config, Vec::new()).expect("create");
    let mut driver = Driver::open(&run.dir).expect("open");
    driver.run_batch().expect("b1");
    driver.run_batch().expect("b2");
    send_request(&run, "grow_case", Some(4)).expect("grow");
    driver.poll_control().expect("apply");
    driver.run().expect("run");
    let target = repo(FIXTURE);
    std::fs::create_dir_all(target.parent().expect("dir")).expect("fixture dir");
    std::fs::copy(run.journal(), target).expect("copy fixture");
}

fn events() -> Vec<JournalEvent> {
    read_events(&repo(FIXTURE)).expect("fixture journal parses")
}

/// A live-looking prefix: everything through batch `upto`, no level/end.
fn prefix(events: &[JournalEvent], upto: u64) -> RunModel {
    let kept: Vec<JournalEvent> = events
        .iter()
        .filter(|e| e.t != "level" && e.t != "end" && e.batch().is_none_or(|b| b <= upto))
        .cloned()
        .collect();
    RunModel::fold_all(&kept)
}

fn ctx(badge: RunBadge) -> Ctx {
    let mut ctx = Ctx::headless(RUN, "dream_world_linear.emath", badge);
    ctx.checkpoints = vec![(4, "b000004.scratch".into(), 5_300)];
    ctx.fork_name = "stepdreamfamilies-002".into();
    ctx
}

fn tab_of(panel: PanelId) -> usize {
    (0..4).find(|t| emath_tui::dashboard::view::tab_panels(*t).contains(&panel)).unwrap_or(0)
}

struct Scenario {
    name: String,
    view: View,
    w: u16,
    h: u16,
    full: bool,
}

fn scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    for (tab, name) in ["overview", "search", "evidence", "journal"].iter().enumerate() {
        out.push(Scenario { name: (*name).into(), view: View { tab, ..View::default() }, w: 160, h: 50, full: false });
    }
    for panel in ALL_PANELS {
        let view = View { tab: tab_of(panel), focus: Some(panel), expanded: Some(panel), ..View::default() };
        out.push(Scenario { name: format!("expanded-{}", panel.name()), view, w: 160, h: 50, full: false });
    }
    out.push(Scenario { name: "overlay-panels".into(), view: View { overlay: Overlay::Panels, ..View::default() }, w: 160, h: 50, full: false });
    out.push(Scenario { name: "overlay-keys".into(), view: View { overlay: Overlay::Keys, ..View::default() }, w: 160, h: 50, full: false });
    out.push(Scenario { name: "ended".into(), view: View::default(), w: 160, h: 50, full: true });
    for (w, h) in [(120, 36), (90, 28), (58, 15)] {
        out.push(Scenario { name: format!("size-{w}x{h}"), view: View::default(), w, h, full: false });
    }
    out
}

fn golden_path(name: &str) -> PathBuf {
    repo(&format!("tests/fixtures/dream/golden/{name}.txt"))
}

fn grids(frame: &Frame) -> String {
    format!("{}--- style ---\n{}", frame.canvas.dump_text(), frame.canvas.dump_style())
}

/// SPEC §15 A10: nothing in a margin or gutter, no text crossing a
/// border, and the status bar's left group ends ≥ 4 cells before the
/// right group.
fn spacing_violations(frame: &Frame, overlay: bool) -> Vec<String> {
    let c = &frame.canvas;
    let mut out = Vec::new();
    if frame.layout.mode == Mode::Text {
        return out;
    }
    let body = body_rect(c.w, c.h);
    if !overlay {
        for y in body.y..body.y + body.h {
            for x in 0..c.w {
                let inside = frame.layout.panels.iter().any(|(_, r)| r.contains(x, y));
                if !inside && c.cell(x, y).ch != ' ' {
                    out.push(format!("glyph {:?} outside panels at ({x},{y})", c.cell(x, y).ch));
                }
            }
        }
        for (panel, r) in &frame.layout.panels {
            for y in r.y + 1..r.y + r.h - 1 {
                for x in [r.x, r.x + r.w - 1] {
                    if c.cell(x, y).ch != '│' {
                        out.push(format!("{panel:?} border crossed at ({x},{y}) by {:?}", c.cell(x, y).ch));
                    }
                }
            }
        }
    }
    let y = c.h - 1;
    let row: Vec<char> = (0..c.w).map(|x| c.cell(x, y).ch).collect();
    if let Some(dot) = row.iter().rposition(|ch| *ch == '●') {
        let left_end = row[..dot].iter().rposition(|ch| *ch != ' ');
        if let Some(end) = left_end {
            if dot - end - 1 < 4 {
                out.push(format!("status bar groups {} cells apart", dot - end - 1));
            }
        }
    }
    out
}

fn check_goldens(p: &mut Probe, live: &RunModel, full: &RunModel) {
    for scenario in scenarios() {
        let model = if scenario.full { full } else { live };
        let badge = if scenario.full {
            RunBadge::Ended { outcome: "plateau".into(), code: None }
        } else {
            RunBadge::Running
        };
        let frame = render(model, &scenario.view, &ctx(badge), scenario.w, scenario.h);
        let text = grids(&frame);
        let path = golden_path(&scenario.name);
        if bless() {
            std::fs::create_dir_all(path.parent().expect("dir")).expect("golden dir");
            std::fs::write(&path, &text).expect("write golden");
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        p.demand(&format!("golden-{}", scenario.name), text == want, format!("render differs from {}", path.display()));
        let violations = spacing_violations(&frame, scenario.view.overlay != Overlay::None);
        p.demand(&format!("spacing-{}", scenario.name), violations.is_empty(), violations.join("; "));
    }
}

#[test]
fn dream_dashboard() {
    if bless() || !Path::new(&repo(FIXTURE)).exists() {
        regenerate_fixture();
    }
    let events = events();
    let live = prefix(&events, 11);
    let full = RunModel::fold_all(&events);
    let mut probe = Probe::new("emath dream viewer: goldens, spacing, panel switches, stalled");

    probe.case("a8-a10-goldens-and-spacing", |p| {
        p.demand("fixture-steered", full.steers.len() == 1, format!("{} steers", full.steers.len()));
        p.demand("fixture-promotes", !full.promotions.is_empty(), "no promotions in the fixture");
        p.demand("fixture-closes", full.level_info.as_ref().is_some_and(|l| l.certificate), "the fixture closes with a certificate");
        check_goldens(p, &live, &full);
    });

    probe.case("a9-hidden-panel-costs-nothing", |p| {
        let mut view = View::default();
        view.toggle_hidden(PanelId::Heat);
        let frame = render(&live, &view, &ctx(RunBadge::Running), 160, 50);
        p.demand("heat-not-placed", frame.layout.panels.iter().all(|(panel, _)| *panel != PanelId::Heat), "hidden Heat was laid out");
        p.eq("heat-derived", view.computed(PanelId::Heat), 0);
        p.demand("frontier-derived", view.computed(PanelId::Frontier) > 0, "Frontier was not drawn");
        let frontier = frame.layout.panels.iter().find(|(panel, _)| *panel == PanelId::Frontier).map(|(_, r)| r.w);
        p.eq("frontier-takes-the-row", frontier, Some(body_rect(160, 50).w));
        let mut last = View::default();
        for panel in emath_tui::dashboard::view::tab_panels(3) {
            last.toggle_hidden(panel);
        }
        p.demand("last-panel-stays", !last.hidden.contains(&PanelId::Events) || !last.hidden.contains(&PanelId::Journal), "every Journal-tab panel was hidden");
    });

    // SPEC §7.3: a control the run cannot take right now drops its
    // keycap chip (key and label dim); available controls keep it.
    probe.case("status-bar-disabled-controls", |p| {
        let cap = |badge: RunBadge, key: char| {
            let frame = render(&live, &View::default(), &ctx(badge), 160, 50);
            let y = frame.canvas.h - 1;
            frame
                .hits
                .iter()
                .find(|h| h.act == HitAct::Key(key) && h.rect.y == y)
                .map(|h| {
                    let cell = frame.canvas.cell(h.rect.x + 1, y);
                    (cell.fg, cell.bg)
                })
        };
        let on = Some((Tok::KeyFg, Tok::Keycap));
        let off = Some((Tok::Dim, Tok::Bar));
        p.eq("running-pause-on", cap(RunBadge::Running, ' '), on);
        p.eq("running-step-off", cap(RunBadge::Running, 's'), off);
        p.eq("running-fork-on", cap(RunBadge::Running, 'f'), on);
        p.eq("paused-step-on", cap(RunBadge::Paused(11), 's'), on);
        for (name, badge) in [("stalled", RunBadge::Stalled), ("ended", RunBadge::Ended { outcome: "plateau".into(), code: None })] {
            for key in [' ', 's', 'g', 'X'] {
                p.eq(&format!("{name}-{key:?}-off"), cap(badge.clone(), key), off);
            }
            p.eq(&format!("{name}-help-on"), cap(badge.clone(), '?'), on);
        }
    });

    probe.case("a13-stalled-and-badges", |p| {
        let badge = derive_badge(&live, None, Some(false), &[], &[]);
        p.eq("stalled", badge.clone(), RunBadge::Stalled);
        let text = render(&live, &View::default(), &ctx(badge), 160, 50).canvas.dump_text();
        p.demand("stalled-tile", text.contains("! STALLED"), "RUN tile lacks ! STALLED");
        p.demand("resume-hint", text.contains(&format!("emath dream resume {RUN}")), "no resume hint");
        let request = |id: &str, action: &str| Request { id: id.into(), action: action.into(), arg: None };
        let ack = |id: &str, batch: u64| Ack { id: id.into(), outcome: "applied".into(), batch, code: None };
        let pause = [request("1", "pause")];
        p.eq("pausing", derive_badge(&live, None, Some(true), &[], &pause), RunBadge::Pausing);
        p.eq("paused", derive_badge(&live, None, Some(true), &[ack("1", 11)], &pause), RunBadge::Paused(11));
        let resumed = [request("1", "pause"), request("2", "resume")];
        p.eq("resumed", derive_badge(&live, None, Some(true), &[ack("1", 11), ack("2", 11)], &resumed), RunBadge::Running);
        p.eq("stopping", derive_badge(&live, None, Some(true), &[], &[request("3", "stop")]), RunBadge::Stopping);
        p.eq("ended-wins", derive_badge(&full, None, Some(false), &[], &[]), RunBadge::Ended { outcome: "plateau".into(), code: None });
    });

    // SPEC §15 A8 "each theme": the goldens are theme-free token grids,
    // so each theme is pinned by its token table, transcribed from
    // SPEC §8.1 (token, Aurora, Nordic, Paper).
    probe.case("a8-theme-tables", |p| {
        let table: &[(Tok, [&str; 3])] = &[
            (Tok::Bg, ["#060a12", "#1b1f27", "#f3ede0"]),
            (Tok::Panel, ["#0a101d", "#222731", "#fbf8ef"]),
            (Tok::Zebra, ["#0d1424", "#262c37", "#f5efe2"]),
            (Tok::Border, ["#1c2642", "#353c4a", "#dccfb2"]),
            (Tok::BorderFocus, ["#2de2e6", "#88c0d0", "#b4561b"]),
            (Tok::Text, ["#d6deee", "#d8dee9", "#2f2a24"]),
            (Tok::Dim, ["#6f7d9c", "#7d8799", "#80766a"]),
            (Tok::Faint, ["#2d3856", "#3d4555", "#d3c6aa"]),
            (Tok::Accent, ["#2de2e6", "#88c0d0", "#b4561b"]),
            (Tok::Accent2, ["#ff4fd8", "#b48ead", "#2b6f8f"]),
            (Tok::Violet, ["#9d7bff", "#b48ead", "#7a4b8c"]),
            (Tok::Good, ["#3ee58f", "#a3be8c", "#4d7d1f"]),
            (Tok::Warn, ["#ffc94d", "#ebcb8b", "#a8760a"]),
            (Tok::Bad, ["#ff5c7a", "#d0707a", "#b3261e"]),
            (Tok::Info, ["#5aa9ff", "#81a1c1", "#2b6f8f"]),
            (Tok::User, ["#ff9f43", "#d08770", "#8a3fa0"]),
            (Tok::Plateau, ["#56627f", "#4c566a", "#a89b82"]),
            (Tok::Bar, ["#080d18", "#171a21", "#e9e1cd"]),
            (Tok::Rule, ["#172039", "#2c323e", "#d8cbae"]),
            (Tok::Keycap, ["#141d35", "#2b313d", "#e2d8c0"]),
            (Tok::KeyFg, ["#2de2e6", "#88c0d0", "#b4561b"]),
            (Tok::Select, ["#12243f", "#2e3a4c", "#f0e2c4"]),
            (Tok::ChartFill, ["#0c2231", "#26303d", "#f1e4c6"]),
        ];
        let heat: [&[&str]; 3] = [
            &["#1a1030", "#40206a", "#76308e", "#b53d7f", "#e5596a", "#f88a4f", "#ffc46b"],
            &["#2a303c", "#2e4454", "#355f6f", "#467f8f", "#5f9fae", "#7fbac7", "#9fd0da"],
            &["#efe6cf", "#e8d19f", "#dca763", "#c6773a", "#9c4a1c", "#5e2a0e"],
        ];
        let hex = |(r, g, b): (u8, u8, u8)| format!("#{r:02x}{g:02x}{b:02x}");
        p.eq("theme-names", THEMES.to_vec(), vec!["aurora", "nordic", "paper"]);
        for (theme, name) in THEMES.iter().enumerate() {
            for (tok, colors) in table {
                p.eq(&format!("{name}-{tok:?}"), hex(color(theme, *tok)), colors[theme].to_string());
            }
            for (i, want) in heat[theme].iter().enumerate() {
                p.eq(&format!("{name}-heat{i}"), hex(color(theme, Tok::Heat(i as u8))), (*want).to_string());
            }
        }
    });

    // SPEC §8.3: a glyph the terminal measures wider than one cell (or
    // any glyph in ASCII glyph mode) falls back to its ASCII stand-in,
    // so no frame can shift a column.
    probe.case("glyph-fallback", |p| {
        let narrow = Glyphs::measured(false, |_| 1);
        p.eq("narrow-keeps", ['◆', '▲', '─', '⣀'].map(|c| narrow.map(c)), ['◆', '▲', '─', '⣀']);
        let wide = Glyphs::measured(false, |c| if c == '◆' || c == '●' { 2 } else { 1 });
        p.eq("wide-falls-back", [wide.map('◆'), wide.map('●')], ['#', '*']);
        p.eq("narrow-neighbours-keep", [wide.map('◇'), wide.map('▲')], ['◇', '▲']);
        let spec = [('⤢', '+'), ('✕', 'x'), ('✎', '*'), ('✗', 'x'), ('⊘', '!'), ('◌', 'o'), ('◆', '#'), ('◇', '%'), ('❚', '|'), ('▲', '^'), ('┃', '|')];
        let ascii = Glyphs::measured(true, |_| 1);
        p.eq("spec-fallbacks", spec.map(|(g, _)| ascii.map(g)), spec.map(|(_, a)| a));
        let ended = ctx(RunBadge::Ended { outcome: "plateau".into(), code: None });
        for scenario in scenarios() {
            let model = if scenario.full { &full } else { &live };
            let frame = render(model, &scenario.view, &ended, scenario.w, scenario.h);
            let left: String = frame.canvas.dump_text().chars().map(|c| ascii.map(c)).filter(|c| !c.is_ascii()).collect();
            p.demand(&format!("ascii-{}", scenario.name), left.is_empty(), format!("no ASCII stand-in for {left:?}"));
        }
    });

    // A fault right after a steer names the steer and the undo (a fork
    // from the newest checkpoint before it).
    probe.case("fault-after-steer", |p| {
        let mut kept: Vec<JournalEvent> = events.iter().filter(|e| e.batch().is_none_or(|b| b <= 2) && e.t != "level" && e.t != "end").cloned().collect();
        let end = JournalEvent::parse(r#"{"seq":9999,"t":"end","batch":2,"outcome":"fault","code":"invalid_index"}"#).expect("end line");
        kept.push(end);
        let faulted = RunModel::fold_all(&kept);
        let mut ended = ctx(RunBadge::Ended { outcome: "fault".into(), code: Some("invalid_index".into()) });
        ended.checkpoints = vec![(1, "b000001.scratch".into(), 4_000), (2, "b000002.scratch".into(), 4_000)];
        let text = render(&faulted, &View::default(), &ended, 160, 50).canvas.dump_text();
        let banner = text.lines().filter(|l| l.contains("RUN ENDED")).last().unwrap_or_default().trim().to_string();
        p.eq("banner", banner, format!("■ RUN ENDED · ✗ FAULT invalid_index · after your grow c4 at b2 · undo: emath dream fork {RUN} --at 1"));
    });

    // A resume rewrites journal.jsonl (the resume law truncates it, then
    // the driver appends): an attached viewer refolds from the new file
    // instead of reading on from a stale byte offset.
    probe.case("watch-follows-a-resume", |p| {
        let root = std::env::temp_dir().join(format!("emath_dream_watch_resume_{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("root");
        let mut config = RunConfig::new(repo("tests/fixtures/constructor/dream_world_linear.emath"));
        config.target = Some("StepDreamFamilies".into());
        config.budget0 = 16;
        config.budget_step = 16;
        config.checkpoint_every = 2;
        let run = create_run(&root, Some("resumed"), true, config, Vec::new()).expect("create");
        let mut driver = Driver::open(&run.dir).expect("open");
        for _ in 0..5 {
            driver.run_batch().expect("batch");
        }
        send_request(&run, "stop", None).expect("stop");
        driver.poll_control().expect("apply stop");
        drop(driver);
        let mut watch = RunWatch::open(RunDir::new(&run.dir));
        watch.poll();
        p.eq("saw-stop", watch.model.end.as_ref().map(|e| e.outcome.clone()), Some("stopped".to_string()));
        let mut resumed = Driver::open(&run.dir).expect("resume the stopped run");
        resumed.run().expect("run to the end");
        watch.poll();
        let events = read_events(&run.journal()).expect("parse");
        p.eq("batches-once", watch.model.batches.len(), events.iter().filter(|e| e.t == "batch").count());
        p.eq("last-seq", watch.model.last_seq, events.last().map_or(0, |e| e.seq));
        p.eq("final-end", watch.model.end.as_ref().map(|e| e.outcome.clone()), Some("plateau".to_string()));
        p.eq("no-parse-fault", watch.fault.clone(), None);
    });

    // The model's derived orderings are rebuilt after every ingest: a
    // viewer that rendered a prefix and then folded the rest shows the
    // same frame as a fresh fold of the whole journal.
    probe.case("incremental-fold-renders-like-a-fresh-fold", |p| {
        let running = ctx(RunBadge::Running);
        let mut model = RunModel::new();
        let (head, rest) = events.split_at(events.len() / 2);
        for event in head {
            model.ingest(event);
        }
        for tab in 0..4 {
            let _ = render(&model, &View { tab, ..View::default() }, &running, 160, 50);
        }
        for event in rest {
            model.ingest(event);
        }
        for tab in 0..4 {
            let view = View { tab, ..View::default() };
            let got = render(&model, &view, &running, 160, 50).canvas.dump_text();
            let want = render(&full, &view, &running, 160, 50).canvas.dump_text();
            p.demand(&format!("tab-{tab}"), got == want, "the incrementally folded model renders differently");
        }
    });

    // SPEC §10.1 mouse: every clickable region the frame registers does
    // what its key does.
    probe.case("mouse", |p| {
        let running = ctx(RunBadge::Running);
        let at = |frame: &Frame, act: HitAct| frame.hits.iter().find(|h| h.act == act).map(|h| (h.rect.x + h.rect.w / 2, h.rect.y));
        let mut view = View::default();
        let frame = render(&live, &view, &running, 160, 50);
        let Some((x, y)) = at(&frame, HitAct::Tab(1)) else { p.fail("tab-hit", "no hit region for tab 2"); return; };
        on_click(&mut view, &live, &running, &frame, x, y, false);
        p.eq("click-tab", view.tab, 1);
        let mut view = View::default();
        let frame = render(&live, &view, &running, 160, 50);
        let Some((x, y)) = at(&frame, HitAct::Toggle(PanelId::Score)) else { p.fail("expand-hit", "no ⤢ for Score"); return; };
        on_click(&mut view, &live, &running, &frame, x, y, false);
        p.eq("click-expand", view.expanded, Some(PanelId::Score));
        let frame = render(&live, &view, &running, 160, 50);
        let Some((x, y)) = at(&frame, HitAct::Toggle(PanelId::Score)) else { p.fail("close-hit", "no ✕ on the expanded Score"); return; };
        on_click(&mut view, &live, &running, &frame, x, y, false);
        p.eq("click-close", view.expanded, None);
        let frame = render(&live, &view, &running, 160, 50);
        let row = frame.hits.iter().find_map(|h| match h.act { HitAct::Row(PanelId::Frontier, key) if Some(key) != live.incumbent => Some((h.rect, key)), _ => None });
        let Some((rect, key)) = row else { p.fail("row-hit", "no Frontier row besides the incumbent"); return; };
        on_click(&mut view, &live, &running, &frame, rect.x + 2, rect.y, false);
        p.eq("click-row-selects", (view.selected, view.follow), (Some(key), false));
        let events = frame.layout.panels.iter().find(|(panel, _)| *panel == PanelId::Events).map(|(_, r)| *r);
        let Some(events) = events else { p.fail("events-placed", "Events not laid out"); return; };
        on_wheel(&mut view, &live, &frame, events.x + 4, events.y + 2, true);
        p.demand("wheel-scrolls-events", view.events_scroll.is_some_and(|s| s > 0), format!("{:?}", view.events_scroll));
        let Some((x, y)) = at(&frame, HitAct::Key(' ')) else { p.fail("keycap-hit", "no space keycap"); return; };
        p.eq("click-keycap", on_click(&mut view, &live, &running, &frame, x, y, false), Action::Request("pause", None));
        let mut view = View { overlay: Overlay::Panels, ..View::default() };
        let frame = render(&live, &view, &running, 160, 50);
        let Some((x, y)) = at(&frame, HitAct::PanelSwitch(PanelId::Heat)) else { p.fail("switch-hit", "no Heat row in the Panels overlay"); return; };
        on_click(&mut view, &live, &running, &frame, x, y, false);
        p.demand("click-switch-hides", view.hidden.contains(&PanelId::Heat), "clicking the Heat row did not hide it");
        let Some((x, y)) = at(&frame, HitAct::Outside) else { p.fail("outside-hit", "no outside region"); return; };
        let _ = (x, y);
        on_click(&mut view, &live, &running, &frame, 0, 0, false);
        p.eq("click-outside-closes", view.overlay, Overlay::None);
    });

    probe.case("keys-map-to-control-requests", |p| {
        let running = ctx(RunBadge::Running);
        let mut view = View::default();
        let frame = render(&live, &view, &running, 160, 50);
        p.eq("space-pauses", on_key(&mut view, &live, &running, &frame, Key::Char(' ')), Action::Request("pause", None));
        p.demand("s-needs-pause", matches!(on_key(&mut view, &live, &running, &frame, Key::Char('s')), Action::Toast(..)), "s stepped a running run");
        let paused = ctx(RunBadge::Paused(6));
        p.eq("space-resumes", on_key(&mut view, &live, &paused, &frame, Key::Char(' ')), Action::Request("resume", None));
        p.eq("s-steps", on_key(&mut view, &live, &paused, &frame, Key::Char('s')), Action::Request("step", None));
        on_key(&mut view, &live, &running, &frame, Key::Char('g'));
        let next = live.case_ids.iter().max().map_or(0, |m| m + 1);
        p.eq("grow-prefill", view.grow_input.clone(), next.to_string());
        p.eq("grow-sends", on_key(&mut view, &live, &running, &frame, Key::Enter), Action::Request("grow_case", Some(next)));
        on_key(&mut view, &live, &running, &frame, Key::Char('X'));
        p.eq("stop-confirms", on_key(&mut view, &live, &running, &frame, Key::Char('y')), Action::Request("stop", None));
        on_key(&mut view, &live, &running, &frame, Key::Char('X'));
        p.eq("stop-cancels", on_key(&mut view, &live, &running, &frame, Key::Char('n')), Action::None);
        let ended = ctx(RunBadge::Ended { outcome: "plateau".into(), code: None });
        p.demand("ended-refuses", matches!(on_key(&mut view, &live, &ended, &frame, Key::Char(' ')), Action::Toast(..)), "an ended run accepted a request");
        on_key(&mut view, &live, &running, &frame, Key::Tab);
        p.eq("tab-focuses-first", view.focus, Some(PanelId::Score));
        on_key(&mut view, &live, &running, &frame, Key::Enter);
        p.eq("enter-expands", view.expanded, Some(PanelId::Score));
        p.eq("q-detaches", on_key(&mut view, &live, &running, &frame, Key::Char('q')), Action::Detach);
    });

    probe.finish();
}
