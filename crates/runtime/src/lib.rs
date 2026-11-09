#[cfg(any(target_os = "android", target_os = "ios", test))]
#[cfg_attr(test, allow(dead_code))]
mod actor;
pub mod app;
#[cfg(any(target_os = "android", target_os = "ios", test))]
#[cfg_attr(test, allow(dead_code))]
mod bridge;
pub mod demo;
pub mod diff;
pub mod element;
#[cfg(target_os = "android")]
mod ffi;
#[cfg(any(target_os = "android", target_os = "ios", test))]
#[cfg_attr(test, allow(dead_code))]
mod gateway;
#[cfg(target_os = "ios")]
mod ios;
pub mod layout;
pub mod model;
pub mod nightshade;
pub mod store;
pub mod tree;

pub use app::{Action, App, Event, HostInfo, Runtime, ViewContext, Viewport};
pub use diff::Op;
pub use element::{
    button, column, gap_w, glass, input, list, map, row, scroll, spacer, text, Element, Kind, Props,
};
pub use layout::Frame;
pub use tree::{build, flatten, FlatTree, UiNode};
