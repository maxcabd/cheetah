//! Sizes the Rust layout math needs. Colors and view sizes live in
//! `views/theme.view`; keep SEARCH_HEIGHT and SEARCH_BOTTOM_GAP in step.

pub(crate) const SEARCH_HEIGHT: f32 = 52.0;
/// Grabber, search bar, and room around it: the sheet with results.
pub(crate) const RESULT_SHEET_HEIGHT: f32 = 120.0;
/// Result cards: their bottom edge above the screen bottom, and height
/// (CAROUSEL_BOTTOM and CARD_HEIGHT in `views/theme.view`).
pub(crate) const CAROUSEL_BOTTOM: f32 = 224.0;
pub(crate) const CARD_HEIGHT: f32 = 280.0;
/// Card width as a fraction of the screen; the rest shows the neighbouring
/// cards peeking in.
pub(crate) const CARD_WIDTH_FRACTION: f32 = 0.78;
/// Space between cards in the carousel.
pub(crate) const CARD_GAP: f32 = 10.0;
/// Cards built on either side of the selected one, so a swipe or two never
/// outruns them.
pub(crate) const CARDS_AHEAD: usize = 2;
/// A minimized card (MINI_CARD_HEIGHT in `views/theme.view`).
pub(crate) const MINI_CARD_HEIGHT: f32 = 72.0;
pub(crate) const SEARCH_BOTTOM_GAP: f32 = 4.0;
/// Before a search: grabber, heading, suggestion chips, search bar.
pub(crate) const IDLE_SHEET_HEIGHT: f32 = 320.0;

/// Smallest span, in degrees, the map fits pins into: a focused card's single
/// pin (~130 m, street level), and several pins (~440 m).
pub(crate) const FOCUS_MIN_SPAN: f32 = 0.0012;
pub(crate) const OVERVIEW_MIN_SPAN: f32 = 0.004;
