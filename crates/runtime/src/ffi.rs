use crate::actor::{spawn, ActorHandle, Command};
use crate::app::Viewport;
use crate::bridge::Host;
use crate::layout::TextRequest;
use jni::objects::{GlobalRef, JFloatArray, JObject, JString, JValue};
use jni::sys::{jboolean, jbyte, jfloat, jint, jlong};
use jni::{JNIEnv, JavaVM};
use std::sync::Arc;

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

    pub(crate) fn measure_text_batch(&self, requests: &[TextRequest]) -> Vec<(f32, f32)> {
        if requests.is_empty() {
            return Vec::new();
        }
        let mut env = match self.vm.get_env() {
            Ok(env) => env,
            Err(_) => return vec![(0.0, 0.0); requests.len()],
        };
        let string_class = match env.find_class("java/lang/String") {
            Ok(class) => class,
            Err(_) => return vec![(0.0, 0.0); requests.len()],
        };
        let texts =
            match env.new_object_array(requests.len() as i32, &string_class, JObject::null()) {
                Ok(array) => array,
                Err(_) => return vec![(0.0, 0.0); requests.len()],
            };
        let mut sizes = Vec::with_capacity(requests.len());
        let mut bolds = Vec::with_capacity(requests.len());
        let mut widths = Vec::with_capacity(requests.len());
        for (index, request) in requests.iter().enumerate() {
            let text = match env.new_string(&request.text) {
                Ok(text) => text,
                Err(_) => return vec![(0.0, 0.0); requests.len()],
            };
            if env
                .set_object_array_element(&texts, index as i32, &text)
                .is_err()
            {
                return vec![(0.0, 0.0); requests.len()];
            }
            sizes.push(request.font_size);
            bolds.push(request.bold as jboolean);
            widths.push(request.max_width);
        }
        let size_array = match env.new_float_array(requests.len() as i32) {
            Ok(array) => array,
            Err(_) => return vec![(0.0, 0.0); requests.len()],
        };
        let width_array = match env.new_float_array(requests.len() as i32) {
            Ok(array) => array,
            Err(_) => return vec![(0.0, 0.0); requests.len()],
        };
        let bold_array = match env.new_boolean_array(requests.len() as i32) {
            Ok(array) => array,
            Err(_) => return vec![(0.0, 0.0); requests.len()],
        };
        if env.set_float_array_region(&size_array, 0, &sizes).is_err()
            || env
                .set_float_array_region(&width_array, 0, &widths)
                .is_err()
            || env
                .set_boolean_array_region(&bold_array, 0, &bolds)
                .is_err()
        {
            return vec![(0.0, 0.0); requests.len()];
        }

        let result = env.call_method(
            self.object.as_obj(),
            "measureTextBatch",
            "([Ljava/lang/String;[F[Z[F)[F",
            &[
                JValue::Object(&texts),
                JValue::Object(&size_array),
                JValue::Object(&bold_array),
                JValue::Object(&width_array),
            ],
        );
        let array = match result.and_then(|value| value.l()) {
            Ok(object) => JFloatArray::from(object),
            Err(_) => return vec![(0.0, 0.0); requests.len()],
        };
        let mut flat = vec![0.0f32; requests.len() * 2];
        if env.get_float_array_region(&array, 0, &mut flat).is_err() {
            return vec![(0.0, 0.0); requests.len()];
        }
        flat.chunks_exact(2)
            .map(|pair| (pair[0], pair[1]))
            .collect()
    }

    pub(crate) fn emit_ops(&self, bytes: &[u8]) {
        let mut env = match self.vm.get_env() {
            Ok(env) => env,
            Err(_) => return,
        };
        let array = match env.new_byte_array(bytes.len() as i32) {
            Ok(array) => array,
            Err(_) => return,
        };
        let mut signed = Vec::with_capacity(bytes.len());
        for byte in bytes {
            signed.push(*byte as jbyte);
        }
        if env.set_byte_array_region(&array, 0, &signed).is_err() {
            return;
        }
        let array = JObject::from(array);
        let _ = env.call_method(
            self.object.as_obj(),
            "onOps",
            "([B)V",
            &[JValue::Object(&array)],
        );
    }
}

impl Host for JniHost {
    fn attach(&self) {
        self.attach_current_thread();
    }

    fn measure_text(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        JniHost::measure_text(self, text, font_size, bold, max_width)
    }

    fn measure_text_batch(&self, requests: &[TextRequest]) -> Vec<(f32, f32)> {
        JniHost::measure_text_batch(self, requests)
    }

    fn emit_ops(&self, bytes: &[u8]) {
        JniHost::emit_ops(self, bytes);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeNew<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    host: JObject<'local>,
    store_path: JString<'local>,
) -> jlong {
    let store_path: String = match env.get_string(&store_path) {
        Ok(path) => path.into(),
        Err(_) => return 0,
    };
    new_actor(&mut env, host, store_path, "chat".to_string())
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeNewApp<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    host: JObject<'local>,
    store_path: JString<'local>,
    app: JString<'local>,
) -> jlong {
    let store_path: String = match env.get_string(&store_path) {
        Ok(path) => path.into(),
        Err(_) => return 0,
    };
    let app: String = match env.get_string(&app) {
        Ok(value) => value.into(),
        Err(_) => return 0,
    };
    new_actor(&mut env, host, store_path, app)
}

fn new_actor(env: &mut JNIEnv, host: JObject, store_path: String, app: String) -> jlong {
    let host: Arc<dyn Host> = match JniHost::new(env, host) {
        Ok(host) => Arc::new(host),
        Err(_) => return 0,
    };

    // `adb reverse tcp:9001 tcp:9001` maps this to the local gateway.
    let actor = spawn(host, store_path, "ws://127.0.0.1:9001".to_string(), app);
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
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeEventCode<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: jint,
    kind_code: jint,
    payload: JString<'local>,
) {
    let Some(kind) = crate::app::event_kind_from_code(kind_code as u32) else {
        return;
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
        kind: kind.to_string(),
        payload,
    });
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
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeLifecycle<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    active: jboolean,
) {
    if handle == 0 {
        return;
    }

    let actor = unsafe { &*(handle as *const ActorHandle) };
    let _ = actor.sender.send(Command::Lifecycle(active != 0));
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
