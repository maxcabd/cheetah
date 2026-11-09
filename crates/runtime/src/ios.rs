use crate::actor::{spawn, ActorHandle, Command};
use crate::app::Viewport;
use crate::bridge::Host;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Arc;

type OpsCallback = extern "C" fn(*const c_char, *mut c_void);
type MeasureCallback =
    extern "C" fn(*const c_char, f32, c_int, f32, *mut f32, *mut f32, *mut c_void);

struct CffiHost {
    ops: OpsCallback,
    measure: MeasureCallback,
    context: *mut c_void,
}

unsafe impl Send for CffiHost {}
unsafe impl Sync for CffiHost {}

impl Host for CffiHost {
    fn measure_text(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        let text = CString::new(text).unwrap_or_default();
        let mut width = 0.0f32;
        let mut height = 0.0f32;

        (self.measure)(
            text.as_ptr(),
            font_size,
            bold as c_int,
            max_width,
            &mut width,
            &mut height,
            self.context,
        );

        (width, height)
    }

    fn emit_ops(&self, json: &str) {
        let json = CString::new(json).unwrap_or_default();
        (self.ops)(json.as_ptr(), self.context);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_new(
    store_path: *const c_char,
    gateway_url: *const c_char,
    ops: OpsCallback,
    measure: MeasureCallback,
    context: *mut c_void,
) -> i64 {
    if store_path.is_null() || gateway_url.is_null() {
        return 0;
    }

    let store_path = CStr::from_ptr(store_path).to_string_lossy().into_owned();
    let gateway_url = CStr::from_ptr(gateway_url).to_string_lossy().into_owned();
    let host: Arc<dyn Host> = Arc::new(CffiHost {
        ops,
        measure,
        context,
    });
    let actor = spawn(host, store_path, gateway_url);
    Box::into_raw(Box::new(actor)) as i64
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_resize(handle: i64, width: f32, height: f32) {
    if handle == 0 {
        return;
    }
    let actor = &*(handle as *const ActorHandle);
    let _ = actor
        .sender
        .send(Command::Resize(Viewport { width, height }));
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_event(
    handle: i64,
    id: u32,
    kind: *const c_char,
    payload: *const c_char,
) {
    if handle == 0 || kind.is_null() || payload.is_null() {
        return;
    }

    let kind = CStr::from_ptr(kind).to_string_lossy().into_owned();
    let payload = CStr::from_ptr(payload).to_string_lossy().into_owned();
    let actor = &*(handle as *const ActorHandle);
    let _ = actor.sender.send(Command::Event { id, kind, payload });
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_lifecycle(handle: i64, active: c_int) {
    if handle == 0 {
        return;
    }
    let actor = &*(handle as *const ActorHandle);
    let _ = actor.sender.send(Command::Lifecycle(active != 0));
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_free(handle: i64) {
    if handle == 0 {
        return;
    }

    let mut actor = Box::from_raw(handle as *mut ActorHandle);
    let _ = actor.sender.send(Command::Shutdown);
    if let Some(join) = actor.join.take() {
        let _ = join.join();
    }
}
