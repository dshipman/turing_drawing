//! Compatibility facade over `atelier_ui` (replaces the former hand-rolled chrome).

pub use atelier_ui::style::*;
pub use atelier_ui::widget::*;

/// Fixed label column width (graphite default metrics).
pub const LABEL_WIDTH: f32 = 72.0;
/// Left machine-list panel width.
pub const LEFT_PANEL: f32 = 248.0;
/// Right inspector panel width.
pub const RIGHT_PANEL: f32 = 288.0;
/// Corner radius used by legacy call sites.
pub const RADIUS: f32 = 2.0;
pub const FONT_BODY: f32 = 12.0;
pub const FONT_SMALL: f32 = 11.0;
