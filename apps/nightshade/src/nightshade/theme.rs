//! Sizes the Rust layout math needs. Colors and view sizes live in
//! `views/theme.view`; keep SEARCH_HEIGHT and SEARCH_BOTTOM_GAP in step.

pub(crate) const SEARCH_HEIGHT: f32 = 52.0;
/// Grabber, search bar, and the gap under it: the sheet with results.
pub(crate) const RESULT_SHEET_HEIGHT: f32 = 100.0;
pub(crate) const SEARCH_BOTTOM_GAP: f32 = 12.0;
/// Before a search: grabber, heading, suggestion chips, search bar.
pub(crate) const IDLE_SHEET_HEIGHT: f32 = 328.0;
