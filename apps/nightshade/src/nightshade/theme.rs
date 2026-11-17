//! Sizes the Rust layout math needs. Colors and view sizes live in
//! `views/theme.view`; keep SEARCH_HEIGHT and SEARCH_BOTTOM_GAP in step.

pub(crate) const SEARCH_HEIGHT: f32 = 72.0;
/// Grabber, search bar, and the gap under it: the sheet with results.
pub(crate) const RESULT_SHEET_HEIGHT: f32 = 136.0;
pub(crate) const SEARCH_BOTTOM_GAP: f32 = 28.0;
/// Before a search: grabber, heading, suggestion chips, search bar.
pub(crate) const IDLE_SHEET_HEIGHT: f32 = 364.0;
