//! `emath dream` viewer (bead emath-3vium, SPEC
//! `internal/design/dream-dashboard/SPEC.md`). The viewer folds the
//! run's journal into a [`model::RunModel`] and renders it into a
//! theme-free [`canvas::Canvas`]; the FrankenTUI presenter
//! (`app`, feature `dashboard`) maps tokens to theme colors and owns the
//! terminal. The viewer never computes mathematics and never writes run
//! state: steering goes through the run's control channel.

pub mod canvas;
pub mod input;
pub mod model;
pub mod panels;
pub mod render;
pub mod view;
pub mod watch;

#[cfg(feature = "dashboard")]
pub mod app;
