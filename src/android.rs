//! The Java side (MainActivity, XrayVpnService) through JNI: calls into the activity, and the
//! events it sends back (`nativeEvent`), queued for the UI thread.

use std::sync::Mutex;

use drawnui::AndroidApp;
use drawnui::prelude::{ControlId, Dirty};
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::jint;
use jni::{JNIEnv, JavaVM};

use crate::Event;

static APP: Mutex<Option<AndroidApp>> = Mutex::new(None);
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
static WAKE: Mutex<Option<(drawnui::Waker, ControlId)>> = Mutex::new(None);

/// MainActivity.EVENT_LINK; 0..4 are VPN states.
const EVENT_LINK: jint = 10;

pub fn set_app(app: AndroidApp) {
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
}

/// Events from before the app was built are delivered now.
pub fn set_waker(waker: drawnui::Waker, id: ControlId) {
    let pending = !EVENTS.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
    if pending {
        waker.invalidate(id, Dirty::APPLY);
    }
    *WAKE.lock().unwrap_or_else(|e| e.into_inner()) = Some((waker, id));
}

pub fn take_events() -> Vec<Event> {
    std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn data_dir() -> Option<std::path::PathBuf> {
    APP.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.internal_data_path()
}

/// Runs `f` with the activity on this thread (attached to the VM once, for good).
fn with_activity<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
    let app = APP.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    // SAFETY: both pointers come from android-activity and live as long as the process / activity.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.ok()?;
    let mut env = vm.attach_current_thread_permanently().ok()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
    let result = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|e| eprintln!("jni: {e}")).ok()
}

/// `notification`: the text of the VPN's notification, in the app's language.
pub fn connect(config: String, notification: &str) {
    with_activity(|env, activity| {
        let config = env.new_string(config)?;
        let notification = env.new_string(notification)?;
        env.call_method(
            activity,
            "vpnConnect",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&config), JValue::Object(&notification)],
        )?;
        Ok(())
    });
}

pub fn disconnect() {
    with_activity(|env, activity| env.call_method(activity, "vpnDisconnect", "()V", &[]).map(|_| ()));
}

/// A test request through the running tunnel; the result comes back as a state event.
pub fn test() {
    with_activity(|env, activity| env.call_method(activity, "vpnTest", "()V", &[]).map(|_| ()));
}

/// Calls a `()String` method of the activity.
fn activity_string(method: &str) -> String {
    with_activity(|env, activity| {
        let text = env.call_method(activity, method, "()Ljava/lang/String;", &[])?.l()?;
        Ok(env.get_string(&JString::from(text))?.into())
    })
    .unwrap_or_default()
}

pub fn clipboard_text() -> String {
    activity_string("clipboardText")
}

/// The phone's language code: "ru", "en", ...
pub fn system_language() -> String {
    activity_string("systemLanguage")
}

/// "Lib v40, Xray-core v26.x" from the AAR.
pub fn core_version() -> String {
    activity_string("coreVersion")
}

/// MainActivity.nativeEvent: any thread (the service's, the activity's).
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_appomobi_drawnvless_MainActivity_nativeEvent<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    kind: jint,
    text: JString<'l>,
) {
    let text: String = env.get_string(&text).map(Into::into).unwrap_or_default();
    let event = if kind == EVENT_LINK { Event::Link(text) } else { Event::State(kind, text) };
    EVENTS.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    if let Some((waker, id)) = WAKE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        waker.invalidate(*id, Dirty::APPLY);
    }
}
