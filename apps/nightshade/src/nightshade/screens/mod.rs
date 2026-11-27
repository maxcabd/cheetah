//! Screens: the map with its search sheet (the root, `/`), and the pages
//! the router opens over it (`views/pages/`, e.g. `/restaurant/[id]`, `/saved`).

mod map;
mod place_ask;
mod restaurant;
mod saved;
mod settings;

pub(crate) use place_ask::PlaceAsk;
