//! The iOS side (ios/app/glue.m): the connection extension through NetworkExtension, the clipboard,
//! links, the language; and the states it sends back (`nc_event`), queued for the UI thread. The same
//! functions as src/android.rs.

use std::ffi::{CStr, CString, c_char, c_int};
use std::sync::Mutex;

use drawnui::prelude::{ControlId, Dirty};

use crate::Event;

static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
static WAKE: Mutex<Option<(drawnui::Waker, ControlId)>> = Mutex::new(None);

unsafe extern "C" {
    fn nc_start();
    fn nc_connect(config: *const c_char);
    fn nc_disconnect();
    fn nc_test();
    fn nc_copy_text(text: *const c_char);
    fn nc_clipboard_text() -> *mut c_char;
    fn nc_open_url(url: *const c_char);
    fn nc_system_language() -> *mut c_char;
    fn nc_core_version() -> *mut c_char;
    fn nc_free(text: *mut c_char);
}

fn c(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

/// A string glue.m made (strdup), freed here.
fn taken(text: *mut c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    let s = unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned();
    unsafe { nc_free(text) };
    s
}

/// Events from before the app was built are delivered now; the state of a connection that is
/// already up is asked for.
pub fn set_waker(waker: drawnui::Waker, id: ControlId) {
    let pending = !EVENTS.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
    if pending {
        waker.invalidate(id, Dirty::APPLY);
    }
    *WAKE.lock().unwrap_or_else(|e| e.into_inner()) = Some((waker, id));
    unsafe { nc_start() };
}

pub fn take_events() -> Vec<Event> {
    std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The app's own Application Support folder.
pub fn data_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// `_notification`: Android's notification text; iOS shows its own status icon.
pub fn connect(config: String, _notification: &str) {
    let config = c(&config);
    unsafe { nc_connect(config.as_ptr()) };
}

pub fn disconnect() {
    unsafe { nc_disconnect() };
}

/// A test request through the running tunnel; the result comes back as a state event.
pub fn test() {
    unsafe { nc_test() };
}

/// No update check on iOS (installs come through TestFlight or Ad Hoc): never asked.
pub fn fetch(_url: &str) {}

pub fn open_url(url: &str) {
    let url = c(url);
    unsafe { nc_open_url(url.as_ptr()) };
}

pub fn copy_text(text: &str) {
    let text = c(text);
    unsafe { nc_copy_text(text.as_ptr()) };
}

pub fn clipboard_text() -> String {
    taken(unsafe { nc_clipboard_text() })
}

/// The phone's language code: "ru", "en", ...
pub fn system_language() -> String {
    taken(unsafe { nc_system_language() })
}

/// "Xray-core v26.11.9", from the app's Info.plist.
pub fn core_version() -> String {
    taken(unsafe { nc_core_version() })
}

/// From glue.m, on any thread.
#[unsafe(no_mangle)]
pub extern "C" fn nc_event(kind: c_int, text: *const c_char) {
    let text = if text.is_null() { String::new() } else { unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned() };
    EVENTS.lock().unwrap_or_else(|e| e.into_inner()).push(Event::State(kind, text));
    if let Some((waker, id)) = WAKE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        waker.invalidate(*id, Dirty::APPLY);
    }
}
