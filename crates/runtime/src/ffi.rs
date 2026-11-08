use crate::app::{Runtime, Viewport};
use crate::demo::ChatApp;
use crate::layout::TextMeasurer;
use jni::objects::{GlobalRef, JFloatArray, JObject, JString, JValue};
use jni::sys::{jfloat, jint, jlong, jstring};
use jni::{JNIEnv, JavaVM};
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

const EMPTY_OPS: &str = "[]";

type MeasurementKey = (String, u32, bool, u32);
type Measurement = (f32, f32);

struct JniMeasurer {
    vm: JavaVM,
    object: GlobalRef,
    cache: RefCell<HashMap<MeasurementKey, Measurement>>,
}

impl JniMeasurer {
    fn new(env: &mut JNIEnv, object: JObject) -> jni::errors::Result<Self> {
        Ok(Self {
            vm: env.get_java_vm()?,
            object: env.new_global_ref(object)?,
            cache: RefCell::new(HashMap::new()),
        })
    }

    fn measure_cached(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        let key = (
            text.to_string(),
            font_size.to_bits(),
            bold,
            max_width.to_bits(),
        );

        if let Some(value) = self.cache.borrow().get(&key) {
            return *value;
        }

        let value = self
            .measure_jni(text, font_size, bold, max_width)
            .unwrap_or((0.0, 0.0));

        let mut cache = self.cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, value);
        value
    }

    fn measure_jni(
        &self,
        text: &str,
        font_size: f32,
        bold: bool,
        max_width: f32,
    ) -> jni::errors::Result<(f32, f32)> {
        let mut env = self.vm.attach_current_thread()?;
        let text = env.new_string(text)?;

        let result = env.call_method(
            self.object.as_obj(),
            "measureText",
            "(Ljava/lang/String;FZF)[F",
            &[
                JValue::Object(&text),
                JValue::Float(font_size),
                JValue::Bool(bold as u8),
                JValue::Float(max_width),
            ],
        )?;

        let array = JFloatArray::from(result.l()?);
        let mut buffer = [0.0f32; 2];
        env.get_float_array_region(&array, 0, &mut buffer)?;
        Ok((buffer[0], buffer[1]))
    }
}

impl TextMeasurer for JniMeasurer {
    fn measure(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        self.measure_cached(text, font_size, bold, max_width)
    }
}

fn to_java_string(env: &mut JNIEnv, value: String) -> jstring {
    match env.new_string(value) {
        Ok(string) => string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeNew<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    measurer: JObject<'local>,
) -> jlong {
    let mut runtime = Runtime::new(ChatApp::new());
    if let Ok(measurer) = JniMeasurer::new(&mut env, measurer) {
        runtime.set_text_measurer(Rc::new(measurer));
    }
    Box::into_raw(Box::new(runtime)) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeRender<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    width: jfloat,
    height: jfloat,
) -> jstring {
    if handle == 0 {
        return to_java_string(&mut env, EMPTY_OPS.to_string());
    }

    let json = catch_unwind(AssertUnwindSafe(|| {
        let runtime = unsafe { &mut *(handle as *mut Runtime) };
        let ops = runtime.render(Viewport { width, height });
        serde_json::to_string(&ops).unwrap_or_else(|_| EMPTY_OPS.to_string())
    }))
    .unwrap_or_else(|_| EMPTY_OPS.to_string());

    to_java_string(&mut env, json)
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeEvent<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
    id: jint,
    kind: JString<'local>,
    payload: JString<'local>,
) -> jstring {
    let kind: String = match env.get_string(&kind) {
        Ok(value) => value.into(),
        Err(_) => String::new(),
    };
    let payload: String = match env.get_string(&payload) {
        Ok(value) => value.into(),
        Err(_) => String::new(),
    };

    if handle == 0 {
        return to_java_string(&mut env, EMPTY_OPS.to_string());
    }

    let json = catch_unwind(AssertUnwindSafe(|| {
        let runtime = unsafe { &mut *(handle as *mut Runtime) };
        let ops = runtime.event(id as u32, &kind, &payload);
        serde_json::to_string(&ops).unwrap_or_else(|_| EMPTY_OPS.to_string())
    }))
    .unwrap_or_else(|_| EMPTY_OPS.to_string());

    to_java_string(&mut env, json)
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeFree<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            drop(Box::from_raw(handle as *mut Runtime));
        }
    }
}
