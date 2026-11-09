use crate::actor::{spawn, ActorHandle, Command};
use crate::app::Viewport;
use crate::layout::TextMeasurer;
use jni::objects::{GlobalRef, JFloatArray, JObject, JString, JValue};
use jni::sys::{jfloat, jint, jlong};
use jni::{JNIEnv, JavaVM};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

type MeasurementKey = (String, u32, bool, u32);
type Measurement = (f32, f32);

pub(crate) struct JniHost {
    vm: JavaVM,
    object: GlobalRef,
}

impl JniHost {
    pub(crate) fn new(env: &mut JNIEnv, object: JObject) -> jni::errors::Result<Self> {
        Ok(Self {
            vm: env.get_java_vm()?,
            object: env.new_global_ref(object)?,
        })
    }

    pub(crate) fn attach_current_thread(&self) {
        if self.vm.get_env().is_err() {
            if let Ok(guard) = self.vm.attach_current_thread() {
                std::mem::forget(guard);
            }
        }
    }

    pub(crate) fn measure_text(
        &self,
        text: &str,
        font_size: f32,
        bold: bool,
        max_width: f32,
    ) -> (f32, f32) {
        let mut env = match self.vm.get_env() {
            Ok(env) => env,
            Err(_) => return (0.0, 0.0),
        };
        let text = match env.new_string(text) {
            Ok(text) => text,
            Err(_) => return (0.0, 0.0),
        };

        let result = match env.call_method(
            self.object.as_obj(),
            "measureText",
            "(Ljava/lang/String;FZF)[F",
            &[
                JValue::Object(&text),
                JValue::Float(font_size),
                JValue::Bool(bold as u8),
                JValue::Float(max_width),
            ],
        ) {
            Ok(result) => result,
            Err(_) => return (0.0, 0.0),
        };

        let array = match result.l() {
            Ok(object) => JFloatArray::from(object),
            Err(_) => return (0.0, 0.0),
        };

        let mut buffer = [0.0f32; 2];
        if env.get_float_array_region(&array, 0, &mut buffer).is_err() {
            return (0.0, 0.0);
        }
        (buffer[0], buffer[1])
    }

    pub(crate) fn emit_ops(&self, json: &str) {
        let mut env = match self.vm.get_env() {
            Ok(env) => env,
            Err(_) => return,
        };
        let json = match env.new_string(json) {
            Ok(json) => json,
            Err(_) => return,
        };
        let _ = env.call_method(
            self.object.as_obj(),
            "onOps",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&json)],
        );
    }
}

pub(crate) struct CachedTextMeasurer {
    host: Arc<JniHost>,
    cache: RefCell<HashMap<MeasurementKey, Measurement>>,
}

impl CachedTextMeasurer {
    pub(crate) fn new(host: Arc<JniHost>) -> Self {
        Self {
            host,
            cache: RefCell::new(HashMap::new()),
        }
    }
}

impl TextMeasurer for CachedTextMeasurer {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        let key = (
            text.to_string(),
            font_size.to_bits(),
            bold,
            max_width.to_bits(),
        );

        if let Some(value) = self.cache.borrow().get(&key) {
            return *value;
        }

        let value = self.host.measure_text(text, font_size, bold, max_width);
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, value);
        value
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeNew<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    host: JObject<'local>,
) -> jlong {
    let host = match JniHost::new(&mut env, host) {
        Ok(host) => Arc::new(host),
        Err(_) => return 0,
    };

    let actor = spawn(host);
    Box::into_raw(Box::new(actor)) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeResize<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    width: jfloat,
    height: jfloat,
) {
    if handle == 0 {
        return;
    }

    let actor = unsafe { &*(handle as *const ActorHandle) };
    let _ = actor
        .sender
        .send(Command::Resize(Viewport { width, height }));
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeEvent<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: jint,
    kind: JString<'local>,
    payload: JString<'local>,
) {
    let kind: String = match env.get_string(&kind) {
        Ok(value) => value.into(),
        Err(_) => return,
    };
    let payload: String = match env.get_string(&payload) {
        Ok(value) => value.into(),
        Err(_) => return,
    };

    if handle == 0 {
        return;
    }

    let actor = unsafe { &*(handle as *const ActorHandle) };
    let _ = actor.sender.send(Command::Event {
        id: id as u32,
        kind,
        payload,
    });
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeFree<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }

    let mut actor = unsafe { Box::from_raw(handle as *mut ActorHandle) };
    let _ = actor.sender.send(Command::Shutdown);
    if let Some(join) = actor.join.take() {
        let _ = join.join();
    }
}
