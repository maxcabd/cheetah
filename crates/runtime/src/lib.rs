#[cfg(any(target_os = "android", target_os = "ios", test, feature = "devserver"))]
#[cfg_attr(test, allow(dead_code))]
mod actor;
pub mod app;
#[cfg(any(target_os = "android", target_os = "ios", test, feature = "devserver"))]
#[cfg_attr(test, allow(dead_code))]
mod bridge;
pub mod demo;
#[cfg(feature = "devserver")]
pub mod dev;
pub mod diff;
pub mod element;
#[cfg(target_os = "android")]
mod ffi;
#[cfg(any(target_os = "android", target_os = "ios", test, feature = "devserver"))]
#[cfg_attr(test, allow(dead_code))]
mod gateway;
pub mod http;
#[cfg(target_os = "ios")]
mod ios;
pub mod kv;
pub mod layout;
pub mod model;
pub mod native_api;
pub mod nav;
// The Nightshade app lives in its own crate (apps/nightshade) so the dev
// server can hot-patch it; device builds compile the same file in here.
#[path = "../../../apps/nightshade/src/nightshade.rs"]
pub mod nightshade;
pub mod resource;
pub mod secure_store;
pub mod store;
pub mod tree;
mod wire;

pub fn encode_ops(ops: &[Op]) -> Vec<u8> {
    wire::encode(ops)
}

pub use app::{Action, App, Event, HostInfo, Runtime, RuntimeMetrics, ViewContext, Viewport};
pub use diff::Op;
pub use element::{
    button, column, gap_w, glass, hscroll, image, input, list, map, native, row, scroll, sheet,
    spacer, text, Element, Kind, Props,
};
pub use layout::{Frame, TextRequest};
pub use native_api::{NativeCall, NativeEvent};
pub use tree::{build, flatten, FlatTree, UiNode};
