use crate::actor::{spawn, ActorHandle, Command};
use crate::app::Viewport;
use crate::bridge::Host;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Arc;

type OpsCallback = extern "C" fn(*const u8, usize, *mut c_void);
type MeasureCallback =
    extern "C" fn(*const c_char, f32, c_int, f32, *mut f32, *mut f32, *mut c_void);
type MeasureBatchCallback = extern "C" fn(*const u8, usize, *mut f32, usize, *mut c_void);

struct CffiHost {
    ops: OpsCallback,
    measure: MeasureCallback,
    measure_batch: MeasureBatchCallback,
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

    fn measure_text_batch(&self, requests: &[crate::layout::TextRequest]) -> Vec<(f32, f32)> {
        if requests.is_empty() {
            return Vec::new();
        }
        let mut bytes = Vec::with_capacity(requests.len() * 32);
        bytes.extend_from_slice(&(requests.len() as u32).to_le_bytes());
        for request in requests {
            let text = request.text.as_bytes();
            bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
            bytes.extend_from_slice(text);
            bytes.extend_from_slice(&request.font_size.to_bits().to_le_bytes());
            bytes.push(if request.bold { 1 } else { 0 });
            bytes.extend_from_slice(&request.max_width.to_bits().to_le_bytes());
        }
        let mut out = vec![0.0f32; requests.len() * 2];
        (self.measure_batch)(
            bytes.as_ptr(),
            bytes.len(),
            out.as_mut_ptr(),
            out.len(),
            self.context,
        );
        out.chunks_exact(2).map(|pair| (pair[0], pair[1])).collect()
    }

    fn emit_ops(&self, bytes: &[u8]) {
        (self.ops)(bytes.as_ptr(), bytes.len(), self.context);
    }
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_new(
    store_path: *const c_char,
    gateway_url: *const c_char,
    ops: OpsCallback,
    measure: MeasureCallback,
    measure_batch: MeasureBatchCallback,
    context: *mut c_void,
) -> i64 {
    new_actor(
        store_path,
        gateway_url,
        "chat",
        ops,
        measure,
        measure_batch,
        context,
    )
}

#[no_mangle]
pub unsafe extern "C" fn cheetah_new_app(
    store_path: *const c_char,
    gateway_url: *const c_char,
    app: *const c_char,
    ops: OpsCallback,
    measure: MeasureCallback,
    measure_batch: MeasureBatchCallback,
    context: *mut c_void,
) -> i64 {
    if app.is_null() {
        return 0;
    }
    let app = CStr::from_ptr(app).to_string_lossy().into_owned();
    new_actor(
        store_path,
        gateway_url,
        &app,
        ops,
        measure,
        measure_batch,
        context,
    )
}

unsafe fn new_actor(
    store_path: *const c_char,
    gateway_url: *const c_char,
    app: &str,
    ops: OpsCallback,
    measure: MeasureCallback,
    measure_batch: MeasureBatchCallback,
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
        measure_batch,
        context,
    });
    let actor = spawn(host, store_path, gateway_url, app.to_string());
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
pub unsafe extern "C" fn cheetah_event_code(
    handle: i64,
    id: u32,
    kind_code: u32,
    payload: *const c_char,
) {
    if handle == 0 || payload.is_null() {
        return;
    }
    let Some(kind) = crate::app::event_kind_from_code(kind_code) else {
        return;
    };
    let payload = CStr::from_ptr(payload).to_string_lossy().into_owned();
    let actor = &*(handle as *const ActorHandle);
    let _ = actor.sender.send(Command::Event {
        id,
        kind: kind.to_string(),
        payload,
    });
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
