//! Key and mouse handling (SPEC §10.1). Handlers only change the
//! viewer's own state or return an `Action` for the host loop: control
//! requests reach the driver through `control/requests.jsonl`, never
//! by touching run state here.

use super::canvas::Tok;
use super::model::RunModel;
use super::render::{selected_key, Frame, HitAct, Mode};
use super::view::{Ctx, Overlay, PanelId, RunBadge, View, ALL_PANELS, TAB_NAMES};

/// A decoded key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Tab,
    BackTab,
    Enter,
    Esc,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Backspace,
    CtrlC,
    CtrlT,
}

/// What the host loop must do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Detach,
    Request(&'static str, Option<i128>),
    Fork,
    Toast(String, Tok),
}

fn set_tab(view: &mut View, tab: usize) {
    view.tab = tab % TAB_NAMES.len();
    view.expanded = None;
    if view.focus.is_some_and(|p| !view.visible().contains(&p)) {
        view.focus = None;
    }
}

fn cycle_focus(view: &mut View, frame: &Frame, forward: bool) {
    let order: Vec<PanelId> = if frame.layout.mode == Mode::Single || view.expanded.is_some() {
        view.visible()
    } else {
        frame.layout.panels.iter().map(|(p, _)| *p).collect()
    };
    if order.is_empty() {
        return;
    }
    let at = view.focus.and_then(|f| order.iter().position(|p| *p == f));
    let next = match (at, forward) {
        (None, true) => 0,
        (None, false) => order.len() - 1,
        (Some(i), true) => (i + 1) % order.len(),
        (Some(i), false) => (i + order.len() - 1) % order.len(),
    };
    view.focus = Some(order[next]);
    if view.expanded.is_some() {
        view.expanded = view.focus;
    }
}

fn move_selection(view: &mut View, model: &RunModel, delta: isize) {
    let list = model.record_list(usize::MAX, true);
    if list.is_empty() {
        return;
    }
    let current = selected_key(view, model);
    let at = current.and_then(|k| list.iter().position(|r| r.key == k)).unwrap_or(0) as isize;
    let next = (at + delta).clamp(0, list.len() as isize - 1) as usize;
    view.selected = Some(list[next].key);
    view.follow = false;
}

fn scroll(slot: &mut Option<usize>, delta: isize, len: usize) {
    let now = slot.unwrap_or(0) as isize;
    let next = (now + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
    *slot = if next == 0 { None } else { Some(next) };
}

fn scroll_focused(view: &mut View, model: &RunModel, up_by: isize) -> bool {
    match view.focus.or(view.expanded) {
        Some(PanelId::Events) => {
            scroll(&mut view.events_scroll, up_by, model.log.len());
            true
        }
        Some(PanelId::Journal) => {
            scroll(&mut view.journal_scroll, up_by, model.raw.len());
            true
        }
        _ => false,
    }
}

fn request(ctx: &Ctx, action: &'static str, arg: Option<i128>) -> Action {
    if ctx.badge.ended() {
        return Action::Toast("the run has ended · controls are off".into(), Tok::Warn);
    }
    if ctx.badge == RunBadge::Stalled {
        return Action::Toast(format!("driver not running · emath dream resume {}", ctx.run_name), Tok::Bad);
    }
    Action::Request(action, arg)
}

/// Handle one key.
pub fn on_key(view: &mut View, model: &RunModel, ctx: &Ctx, frame: &Frame, key: Key) -> Action {
    match view.overlay {
        Overlay::None => {}
        Overlay::Keys => {
            view.overlay = Overlay::None;
            return Action::None;
        }
        Overlay::Panels => {
            match key {
                Key::Esc | Key::Char('v') => view.overlay = Overlay::None,
                Key::Char(c @ 'a'..='k') => view.toggle_hidden(ALL_PANELS[usize::from(c as u8 - b'a')]),
                _ => {}
            }
            return Action::None;
        }
        Overlay::Grow => {
            match key {
                Key::Esc => view.overlay = Overlay::None,
                Key::Backspace => {
                    view.grow_input.pop();
                }
                Key::Char(c) if c.is_ascii_digit() && view.grow_input.len() < 12 => view.grow_input.push(c),
                Key::Enter => {
                    view.overlay = Overlay::None;
                    return match view.grow_input.parse::<i128>() {
                        Ok(id) => request(ctx, "grow_case", Some(id)),
                        Err(_) => Action::Toast("enter a case ordinal".into(), Tok::Warn),
                    };
                }
                _ => {}
            }
            return Action::None;
        }
        Overlay::Stop => {
            view.overlay = Overlay::None;
            return if key == Key::Char('y') { request(ctx, "stop", None) } else { Action::None };
        }
        Overlay::Fork => {
            view.overlay = Overlay::None;
            return if key == Key::Char('y') && !ctx.checkpoints.is_empty() { Action::Fork } else { Action::None };
        }
    }
    match key {
        Key::Char(c @ '1'..='4') => set_tab(view, usize::from(c as u8 - b'1')),
        Key::Left => set_tab(view, view.tab + TAB_NAMES.len() - 1),
        Key::Right => set_tab(view, view.tab + 1),
        Key::Tab => cycle_focus(view, frame, true),
        Key::BackTab => cycle_focus(view, frame, false),
        Key::Enter => {
            if view.expanded.is_some() {
                view.expanded = None;
            } else {
                let focus = view.focus.or(frame.layout.panels.first().map(|(p, _)| *p));
                view.focus = focus;
                view.expanded = focus;
            }
        }
        Key::Esc => view.expanded = None,
        Key::Up => {
            if !scroll_focused(view, model, 1) {
                move_selection(view, model, -1);
            }
        }
        Key::Down => {
            if !scroll_focused(view, model, -1) {
                move_selection(view, model, 1);
            }
        }
        Key::PageUp => {
            if !scroll_focused(view, model, 10) {
                move_selection(view, model, -10);
            }
        }
        Key::PageDown => {
            if !scroll_focused(view, model, -10) {
                move_selection(view, model, 10);
            }
        }
        Key::Home => {
            view.follow = true;
            view.selected = None;
        }
        Key::End => {
            view.events_scroll = None;
            view.journal_scroll = None;
        }
        Key::Char(' ') => {
            return match ctx.badge {
                RunBadge::Paused(_) => request(ctx, "resume", None),
                _ => request(ctx, "pause", None),
            };
        }
        Key::Char('s') => {
            return if matches!(ctx.badge, RunBadge::Paused(_)) {
                request(ctx, "step", None)
            } else {
                Action::Toast("pause first (space); s steps one batch".into(), Tok::Warn)
            };
        }
        Key::Char('g') => {
            view.grow_input = model.case_ids.iter().max().map_or(0, |m| m + 1).to_string();
            view.overlay = Overlay::Grow;
        }
        Key::Char('X') => view.overlay = Overlay::Stop,
        Key::Char('c') => return request(ctx, "checkpoint", None),
        Key::Char('f') => view.overlay = Overlay::Fork,
        Key::Char('p') => return Action::Toast("family pinning needs the steering seam (D5)".into(), Tok::Info),
        Key::Char('v') => view.overlay = Overlay::Panels,
        Key::Char('t') | Key::CtrlT => view.theme = (view.theme + 1) % super::canvas::THEMES.len(),
        Key::Char('?') => view.overlay = Overlay::Keys,
        Key::Char('q') | Key::CtrlC => return Action::Detach,
        _ => {}
    }
    Action::None
}

/// Handle a click (`double` = second click within the double-click window).
pub fn on_click(view: &mut View, model: &RunModel, ctx: &Ctx, frame: &Frame, x: u16, y: u16, double: bool) -> Action {
    let Some(hit) = frame.hits.iter().rev().find(|h| h.rect.contains(x, y)) else { return Action::None };
    match hit.act {
        HitAct::Tab(tab) => set_tab(view, tab),
        HitAct::Focus(panel) => {
            view.focus = Some(panel);
            if double {
                view.expanded = if view.expanded == Some(panel) { None } else { Some(panel) };
            }
        }
        HitAct::Toggle(panel) => {
            view.focus = Some(panel);
            view.expanded = if view.expanded == Some(panel) { None } else { Some(panel) };
        }
        HitAct::Row(panel, key) => {
            view.focus = Some(panel);
            view.selected = Some(key);
            view.follow = false;
            if double {
                view.expanded = if view.expanded == Some(panel) { None } else { Some(panel) };
            }
        }
        HitAct::Key(c) => {
            let key = match c {
                '\t' => Key::Tab,
                '\n' => Key::Enter,
                other => Key::Char(other),
            };
            return on_key(view, model, ctx, frame, key);
        }
        HitAct::PanelSwitch(panel) => view.toggle_hidden(panel),
        HitAct::Outside => view.overlay = Overlay::None,
    }
    Action::None
}

/// Wheel over a panel: scroll Events/Journal, else move the selection.
pub fn on_wheel(view: &mut View, model: &RunModel, frame: &Frame, x: u16, y: u16, up: bool) {
    let panel = frame.layout.panels.iter().find(|(_, r)| r.contains(x, y)).map(|(p, _)| *p);
    let delta: isize = if up { 3 } else { -3 };
    match panel {
        Some(PanelId::Events) => scroll(&mut view.events_scroll, delta, model.log.len()),
        Some(PanelId::Journal) => scroll(&mut view.journal_scroll, delta, model.raw.len()),
        Some(PanelId::Frontier | PanelId::Heat | PanelId::Lineage) => move_selection(view, model, if up { -1 } else { 1 }),
        _ => {}
    }
}

