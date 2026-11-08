use crate::app::{Runtime, Viewport};
use crate::demo::ChatApp;
use jni::objects::{JObject, JString};
use jni::sys::{jfloat, jint, jlong, jstring};
use jni::JNIEnv;
use std::panic::{catch_unwind, AssertUnwindSafe};

const EMPTY_OPS: &str = "[]";

fn to_java_string(env: &mut JNIEnv, value: String) -> jstring {
    match env.new_string(value) {
        Ok(string) => string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_example_rustui_RustBridge_nativeNew<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
) -> jlong {
    let runtime = Runtime::new(ChatApp::new());
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
