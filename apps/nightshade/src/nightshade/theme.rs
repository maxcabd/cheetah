//! Sizes the Rust layout math needs. Colors and view sizes live in
//! `views/theme.view`; keep SEARCH_HEIGHT and SEARCH_BOTTOM_GAP in step.

pub(crate) const SEARCH_HEIGHT: f32 = 52.0;
/// Grabber, search bar, and room around it: the sheet with results.
pub(crate) const RESULT_SHEET_HEIGHT: f32 = 120.0;
/// Result cards: their bottom edge above the screen bottom, and height
/// (CAROUSEL_BOTTOM and CARD_HEIGHT in `views/theme.view`).
pub(crate) const CAROUSEL_BOTTOM: f32 = 224.0;
pub(crate) const CARD_HEIGHT: f32 = 320.0;
pub(crate) const SEARCH_BOTTOM_GAP: f32 = 4.0;
/// Space between the search bar and an open keyboard.
pub(crate) const KEYBOARD_GAP: f32 = 8.0;
/// Before a search: grabber, heading, suggestion chips, search bar.
pub(crate) const IDLE_SHEET_HEIGHT: f32 = 320.0;
