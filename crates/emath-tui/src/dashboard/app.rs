//! FrankenTUI presenter for `emath dream` (feature `dashboard`, SPEC
//! §13). FrankenTUI owns the terminal, input, resize and buffer diff;
//! this module only polls the run directory, maps keys and clicks onto
//! the pure `input` handlers, and paints the theme-free canvas with the
//! active theme's colors. Closing the viewer never stops the run.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ftui::render::cell::StyleFlags;
use ftui::{tick_every, App, Cell as FtCell, CellAttrs, Cmd, Event, Frame, KeyCode, KeyEventKind, Model, Modifiers, MouseButton, MouseEventKind, PackedRgba};

use super::canvas::{blend, color, Canvas, Glyphs, Tok};
use super::input::{on_click, on_key, on_wheel, Action, Key};
use super::render::{render, Frame as Rendered};
use super::view::{Ctx, PanelId, RunBadge, View};
use super::watch::RunWatch;
use crate::dream::driver::spawn_driver;
use crate::dream::resume::fork_run;
use crate::dream::rundir::{next_run_name, send_request, RunDir};

/// Viewer options (`--theme`, `--hide`, `--no-measure`).
#[derive(Clone, Debug, Default)]
pub struct WatchOptions {
    pub theme: usize,
    pub hidden: Vec<PanelId>,
    pub no_measure: bool,
    /// The `emath` executable, for spawning a fork's driver.
    pub exe: Option<PathBuf>,
    /// The runs root, for forks.
    pub root: PathBuf,
}

/// Runtime messages.
pub enum Msg {
    Input(Event),
    Tick,
}

impl From<Event> for Msg {
    fn from(event: Event) -> Self {
        Msg::Input(event)
    }
}

const TOAST_SECS: f32 = 3.2;
const FLASH: Duration = Duration::from_millis(1400);

struct Dashboard {
    watch: RunWatch,
    view: View,
    opts: WatchOptions,
    toast: Option<(String, Tok, Instant)>,
    arrivals: BTreeMap<u64, Instant>,
    last_refresh: Instant,
    last_click: Option<(u16, u16, Instant)>,
    pending: Option<(String, &'static str)>,
    slow_hint: Option<Instant>,
    rendered: RefCell<Option<Rendered>>,
    frame_ms: RefCell<VecDeque<f64>>,
    glyphs: Glyphs,
    /// Whether the next `view` must re-render (SPEC §13: redraw on new
    /// run data, input, or animation; otherwise repaint the last frame).
    stale: Cell<bool>,
}

impl Dashboard {
    fn new(run: RunDir, opts: WatchOptions) -> Self {
        let mut view = View { theme: opts.theme, ..View::default() };
        for panel in &opts.hidden {
            view.toggle_hidden(*panel);
        }
        let mut watch = RunWatch::open(run);
        watch.refresh();
        watch.poll();
        Dashboard {
            watch,
            view,
            opts,
            toast: None,
            arrivals: BTreeMap::new(),
            last_refresh: Instant::now(),
            last_click: None,
            pending: None,
            slow_hint: None,
            glyphs: {
                let policy = ftui::core::glyph_policy::GlyphPolicy::detect();
                let ascii = policy.mode == ftui::core::glyph_policy::GlyphMode::Ascii;
                Glyphs::measured(ascii, ftui::core::text_width::char_width)
            },
            rendered: RefCell::new(None),
            frame_ms: RefCell::new(VecDeque::new()),
            stale: Cell::new(true),
        }
    }

    /// Forks land next to the watched run, under the name `fork_run`
    /// would pick, so the dialog names the directory the fork creates.
    fn fork_root(&self) -> PathBuf {
        self.watch.run.dir.parent().map_or_else(|| self.opts.root.clone(), PathBuf::from)
    }

    fn fork_name(&self) -> String {
        let target = self.watch.info.as_ref().map_or_else(|| self.watch.model.target.clone(), |i| i.target.clone());
        next_run_name(&self.fork_root(), &target)
    }

    fn say(&mut self, text: impl Into<String>, tok: Tok) {
        self.toast = Some((text.into(), tok, Instant::now()));
    }

    fn ctx(&self) -> Ctx {
        let badge = match (&self.pending, self.watch.badge()) {
            (Some((_, "pause")), RunBadge::Running) => RunBadge::Pausing,
            (Some((_, "stop")), RunBadge::Running | RunBadge::Paused(_)) => RunBadge::Stopping,
            (_, badge) => badge,
        };
        let name = self.watch.run.name();
        let module_file = self
            .watch
            .info
            .as_ref()
            .and_then(|i| i.config.module.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let mut ctx = Ctx::headless(&name, &module_file, badge);
        ctx.checkpoints = self.watch.checkpoints.clone();
        ctx.fork_name = self.fork_name();
        ctx.pending = self.pending.as_ref().map(|p| p.1.to_string());
        if let Some((text, tok, at)) = &self.toast {
            let age = at.elapsed().as_secs_f32();
            if age < TOAST_SECS {
                ctx.toast = Some((text.clone(), *tok, 1.0 - age / TOAST_SECS));
            }
        }
        if !self.opts.no_measure {
            ctx.elapsed_secs = self.watch.elapsed_secs();
            ctx.bps = self.watch.bps();
            let times = self.frame_ms.borrow();
            ctx.frame_ms = times.back().copied();
            ctx.fresh = self.arrivals.iter().filter(|(_, at)| at.elapsed() < FLASH).map(|(seq, _)| *seq).collect();
        }
        ctx
    }

    /// Poll the run directory; returns whether the frame must re-render.
    fn tick(&mut self) -> bool {
        let animating = self.toast.is_some() || !self.arrivals.is_empty();
        let mut changed = self.watch.poll();
        if changed && !self.opts.no_measure && self.watch.new_seqs.len() <= 50 {
            let now = Instant::now();
            for seq in &self.watch.new_seqs {
                self.arrivals.insert(*seq, now);
            }
        }
        self.arrivals.retain(|_, at| at.elapsed() < FLASH);
        if self.last_refresh.elapsed() >= Duration::from_secs(1) {
            let before = (self.watch.badge(), self.watch.checkpoints.clone());
            self.watch.refresh();
            self.last_refresh = Instant::now();
            changed |= before != (self.watch.badge(), self.watch.checkpoints.clone());
            self.slow_frames_hint();
        }
        if let Some((id, action)) = self.pending.clone() {
            if let Some(ack) = self.watch.acks.iter().find(|a| a.id == id).cloned() {
                changed = true;
                self.pending = None;
                if ack.outcome == "applied" {
                    self.say(format!("{action} applied at b{}", ack.batch), Tok::User);
                } else {
                    self.say(format!("{action} refused · {}", ack.code.unwrap_or_default()), Tok::Warn);
                }
            }
        }
        if self.toast.as_ref().is_some_and(|t| t.2.elapsed().as_secs_f32() >= TOAST_SECS) {
            self.toast = None;
        }
        changed || animating
    }

    fn slow_frames_hint(&mut self) {
        let mut times: Vec<f64> = self.frame_ms.borrow().iter().copied().collect();
        if times.len() < 60 || self.slow_hint.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            return;
        }
        times.sort_by(f64::total_cmp);
        if times[times.len() * 95 / 100] > 16.0 {
            self.slow_hint = Some(Instant::now());
            self.say("rendering is slow · press v to hide heavy panels", Tok::Warn);
        }
    }

    fn act(&mut self, action: Action) -> Cmd<Msg> {
        match action {
            Action::None => {}
            Action::Detach => return Cmd::quit(),
            Action::Toast(text, tok) => self.say(text, tok),
            Action::Request(action, arg) => match send_request(&self.watch.run, action, arg) {
                Ok(id) => {
                    self.pending = Some((id, action));
                    self.say(format!("{action} requested · applies at the next batch boundary"), Tok::Info);
                }
                Err(fault) => self.say(format!("{}: {}", fault.code, fault.message), Tok::Bad),
            },
            Action::Fork => {
                let name = self.fork_name();
                let forked = fork_run(&self.fork_root(), &self.watch.run, None, Some(&name), None, None);
                match (forked, self.opts.exe.clone()) {
                    (Ok(new_run), Some(exe)) => match spawn_driver(&exe, &new_run) {
                        Ok(_) => {
                            let name = new_run.name();
                            self.say(format!("forked → {name} · emath dream watch {name}"), Tok::User);
                        }
                        Err(fault) => self.say(format!("{}: {}", fault.code, fault.message), Tok::Bad),
                    },
                    (Ok(new_run), None) => self.say(format!("forked → {} · start it with emath dream resume", new_run.name()), Tok::User),
                    (Err(fault), _) => self.say(format!("{}: {}", fault.code, fault.message), Tok::Warn),
                }
            }
        }
        Cmd::none()
    }

    fn with_frame<R>(&mut self, f: impl FnOnce(&mut Self, &Rendered) -> R) -> Option<R> {
        let frame = self.rendered.borrow_mut().take()?;
        let out = f(self, &frame);
        *self.rendered.borrow_mut() = Some(frame);
        Some(out)
    }
}

fn key_of(code: KeyCode, modifiers: Modifiers) -> Option<Key> {
    let ctrl = modifiers.contains(Modifiers::CTRL);
    Some(match code {
        KeyCode::Char('c') if ctrl => Key::CtrlC,
        KeyCode::Char('t') if ctrl => Key::CtrlT,
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Esc,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Backspace => Key::Backspace,
        _ => return None,
    })
}

fn rgb(c: (u8, u8, u8)) -> PackedRgba {
    PackedRgba::rgb(c.0, c.1, c.2)
}

/// Paint a canvas into a FrankenTUI frame with theme colors.
fn paint(canvas: &Canvas, theme: usize, glyphs: &Glyphs, frame: &mut Frame) {
    let bg = color(theme, Tok::Bg);
    let (w, h) = (canvas.w.min(frame.width()), canvas.h.min(frame.height()));
    for y in 0..h {
        for x in 0..w {
            let cell = canvas.cell(x, y);
            let (mut fg, mut back) = (color(theme, cell.fg), color(theme, cell.bg));
            if cell.veiled {
                fg = blend(fg, bg, 0.62);
                back = blend(back, bg, 0.62);
            }
            let flags = if cell.bold { StyleFlags::BOLD } else { StyleFlags::empty() };
            let out = FtCell::from_char(glyphs.map(cell.ch)).with_fg(rgb(fg)).with_bg(rgb(back)).with_attrs(CellAttrs::new(flags, 0));
            frame.buffer.set(x, y, out);
        }
    }
}

impl Model for Dashboard {
    type Message = Msg;

    fn update(&mut self, msg: Msg) -> Cmd<Msg> {
        match msg {
            Msg::Tick => {
                if self.tick() {
                    self.stale.set(true);
                }
                Cmd::none()
            }
            Msg::Input(Event::Key(key)) => {
                self.stale.set(true);
                if key.kind == KeyEventKind::Release {
                    return Cmd::none();
                }
                let Some(key) = key_of(key.code, key.modifiers) else { return Cmd::none() };
                let action = self
                    .with_frame(|me, frame| {
                        let ctx = me.ctx();
                        on_key(&mut me.view, &me.watch.model, &ctx, frame, key)
                    })
                    .unwrap_or(Action::None);
                self.act(action)
            }
            Msg::Input(Event::Mouse(mouse)) => {
                self.stale.set(true);
                let (x, y) = (mouse.x, mouse.y);
                let action = self.with_frame(|me, frame| match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let double = me.last_click.is_some_and(|(px, py, at)| px == x && py == y && at.elapsed() < Duration::from_millis(400));
                        me.last_click = Some((x, y, Instant::now()));
                        let ctx = me.ctx();
                        on_click(&mut me.view, &me.watch.model, &ctx, frame, x, y, double)
                    }
                    MouseEventKind::ScrollUp => {
                        on_wheel(&mut me.view, &me.watch.model, frame, x, y, true);
                        Action::None
                    }
                    MouseEventKind::ScrollDown => {
                        on_wheel(&mut me.view, &me.watch.model, frame, x, y, false);
                        Action::None
                    }
                    _ => Action::None,
                });
                self.act(action.unwrap_or(Action::None))
            }
            Msg::Input(_) => {
                self.stale.set(true);
                Cmd::none()
            }
        }
    }

    fn view(&self, frame: &mut Frame) {
        let (w, h) = (frame.width(), frame.height());
        if !self.stale.get() {
            if let Some(last) = self.rendered.borrow().as_ref().filter(|r| r.canvas.w == w && r.canvas.h == h) {
                paint(&last.canvas, self.view.theme, &self.glyphs, frame);
                return;
            }
        }
        self.stale.set(false);
        let started = Instant::now();
        let ctx = self.ctx();
        let rendered = render(&self.watch.model, &self.view, &ctx, frame.width(), frame.height());
        paint(&rendered.canvas, self.view.theme, &self.glyphs, frame);
        *self.rendered.borrow_mut() = Some(rendered);
        let mut times = self.frame_ms.borrow_mut();
        times.push_back(started.elapsed().as_secs_f64() * 1000.0);
        while times.len() > 60 {
            times.pop_front();
        }
    }

    fn subscriptions(&self) -> Vec<Box<dyn ftui::Subscription<Msg>>> {
        let animating = self.toast.is_some() || !self.arrivals.is_empty();
        let interval = if animating { Duration::from_millis(33) } else { Duration::from_millis(100) };
        vec![tick_every(interval, || Msg::Tick)]
    }
}

/// Attach the dashboard to a run directory until `q` / Ctrl+C.
pub fn watch(run: RunDir, opts: WatchOptions) -> std::io::Result<()> {
    // The dashboard paints one whole frame of its own; a render slower
    // than FrankenTUI's 16 ms default budget must still be presented
    // (a skipped frame leaves the screen blank), so the budget is strict
    // and generous. Slow frames surface through the adaptive hint.
    let budget = ftui::render::budget::FrameBudgetConfig::strict(Duration::from_secs(2));
    App::fullscreen(Dashboard::new(run, opts)).with_mouse().with_budget(budget).run()
}
