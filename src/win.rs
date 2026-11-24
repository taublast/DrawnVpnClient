//! Windows: the tunnel is Xray-core's own TUN (the Wintun driver), run by a second copy of this
//! program started as administrator (`--tunnel`, the Windows prompt on every Connect). The app
//! itself stays a normal user process: it stops the tunnel through a named event, and the tunnel
//! also stops when the app exits.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use drawnui::prelude::{ControlId, Dirty};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetExitCodeProcess, INFINITE, OpenEventW, OpenProcess, SetEvent, WaitForMultipleObjects,
    WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};

use crate::Event;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const EVENT_MODIFY_STATE: u32 = 0x0002;
const ERROR_CANCELLED: u32 = 1223;
/// Answers 204 with no body: a cheap check that traffic really passes the server.
const PROBE_URL: &str = "https://www.gstatic.com/generate_204";

static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
static WAKE: Mutex<Option<(drawnui::Waker, ControlId)>> = Mutex::new(None);
/// The running tunnel: its helper process and the event that stops it.
static TUNNEL: Mutex<Option<Tunnel>> = Mutex::new(None);
/// Counts tunnels, so a late report of an old one never overwrites the new one's state.
static GENERATION: AtomicU32 = AtomicU32::new(0);

struct Tunnel {
    generation: u32,
    process: usize,
    stop: usize,
}

fn push(event: Event) {
    EVENTS.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    if let Some((waker, id)) = WAKE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        waker.invalidate(*id, Dirty::APPLY);
    }
}

pub fn set_waker(waker: drawnui::Waker, id: ControlId) {
    let pending = !EVENTS.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
    if pending {
        waker.invalidate(id, Dirty::APPLY);
    }
    *WAKE.lock().unwrap_or_else(|e| e.into_inner()) = Some((waker, id));
    // Nothing runs at start: the page shows "Not connected" (and the update check starts).
    push(Event::State(0, String::new()));
}

pub fn take_events() -> Vec<Event> {
    std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn data_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("Net Connect");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// xray.exe and wintun.dll, in "engine" next to the program.
fn engine_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("engine"))).unwrap_or_default()
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

fn hidden(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Starts the tunnel with an Xray config (a running one is stopped first: switching servers).
pub fn connect(config: String, _notification: &str) {
    std::thread::spawn(move || {
        stop_tunnel(true);
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        if let Err(text) = start_tunnel(&config, generation) {
            push(Event::State(3, text));
        }
    });
}

pub fn disconnect() {
    std::thread::spawn(|| stop_tunnel(false));
}

/// A test request through the running tunnel: connected with its delay, or no answer.
pub fn test() {
    std::thread::spawn(|| {
        let generation = GENERATION.load(Ordering::SeqCst);
        report_probe(generation, probe());
    });
}

fn report_probe(generation: u32, ms: Option<u64>) {
    if GENERATION.load(Ordering::SeqCst) != generation || TUNNEL.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
        return;
    }
    match ms {
        Some(ms) => push(Event::State(2, ms.to_string())),
        None => push(Event::State(4, String::new())),
    }
}

fn start_tunnel(config: &str, generation: u32) -> Result<(), String> {
    let dir = data_dir().ok_or("No data folder")?;
    let config_path = dir.join("run.json");
    std::fs::write(&config_path, config).map_err(|e| e.to_string())?;
    let pid = std::process::id();
    let stop_name = format!("Local\\NetConnectStop-{pid}-{generation}");
    let ready_name = format!("Local\\NetConnectReady-{pid}-{generation}");
    // SAFETY: plain Win32 calls; the handles are owned here and closed in stop_tunnel / below.
    unsafe {
        let stop = CreateEventW(std::ptr::null(), 1, 0, wide(&stop_name).as_ptr());
        let ready = CreateEventW(std::ptr::null(), 1, 0, wide(&ready_name).as_ptr());
        if stop.is_null() || ready.is_null() {
            return Err("Could not create the stop event".into());
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let parameters = format!(
            "--tunnel \"{}\" --parent {pid} --stop {stop_name} --ready {ready_name}",
            config_path.display()
        );
        let (verb, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&parameters));
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = 0; // SW_HIDE
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            let error = GetLastError();
            CloseHandle(stop);
            CloseHandle(ready);
            // The user chose No in the Windows prompt: the app shows its own text.
            return Err(if error == ERROR_CANCELLED { "permission".into() } else { format!("The tunnel did not start ({error})") });
        }
        let process = info.hProcess;
        *TUNNEL.lock().unwrap_or_else(|e| e.into_inner()) = Some(Tunnel { generation, process: process as usize, stop: stop as usize });
        push(Event::State(1, String::new()));
        // Ready (Xray's tunnel is up), else the helper ended: then the watcher reports it.
        let handles = [ready, process];
        let woke = WaitForMultipleObjects(2, handles.as_ptr(), 0, 30_000);
        CloseHandle(ready);
        let watch = process as usize;
        std::thread::spawn(move || watch_helper(generation, watch));
        if woke == WAIT_OBJECT_0 {
            report_probe(generation, probe());
        } else if woke != WAIT_OBJECT_0 + 1 {
            stop_tunnel(false);
            return Err("The tunnel did not start in time".into());
        }
    }
    Ok(())
}

/// Reports the end of a helper: off, or the engine's own failure.
fn watch_helper(generation: u32, process: usize) {
    let process = process as HANDLE;
    // SAFETY: the handle stays open until stop_tunnel closes it after this wait returns.
    let code = unsafe {
        WaitForSingleObject(process, INFINITE);
        let mut code = 0u32;
        GetExitCodeProcess(process, &mut code);
        code
    };
    let mut tunnel = TUNNEL.lock().unwrap_or_else(|e| e.into_inner());
    if tunnel.as_ref().is_some_and(|t| t.generation == generation) {
        if let Some(t) = tunnel.take() {
            // SAFETY: handles from start_tunnel, closed once.
            unsafe {
                CloseHandle(t.stop as HANDLE);
                CloseHandle(t.process as HANDLE);
            }
        }
        drop(tunnel);
        push(if code == 0 { Event::State(0, String::new()) } else { Event::State(3, "The engine stopped".into()) });
    }
}

/// Signals the running tunnel to stop and waits for it (up to 10 s). `switching`: no "off" report.
fn stop_tunnel(switching: bool) {
    let tunnel = TUNNEL.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(t) = tunnel else {
        if !switching {
            push(Event::State(0, String::new()));
        }
        return;
    };
    // SAFETY: handles from start_tunnel, closed once here (the watcher sees the tunnel gone).
    unsafe {
        SetEvent(t.stop as HANDLE);
        WaitForSingleObject(t.process as HANDLE, 10_000);
        CloseHandle(t.stop as HANDLE);
        CloseHandle(t.process as HANDLE);
    }
    if !switching {
        push(Event::State(0, String::new()));
    }
}

/// Two requests on one connection through the system (so through the tunnel), the faster one in
/// ms, as Android's measure does.
fn probe() -> Option<u64> {
    for _ in 0..3 {
        let out = hidden("curl.exe")
            .args(["-s", "-o", "NUL", "-o", "NUL", "--max-time", "10", "-w", "%{http_code} %{time_total}\\n", PROBE_URL, PROBE_URL])
            .output()
            .ok()?;
        let best = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let (code, time) = line.trim().split_once(' ')?;
                matches!(code, "200" | "204").then(|| time.parse::<f64>().ok()).flatten()
            })
            .fold(None, |best: Option<f64>, t| Some(best.map_or(t, |b| b.min(t))));
        if let Some(seconds) = best {
            return Some((seconds * 1000.0).round() as u64);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    None
}

pub fn fetch(url: &str) {
    let url = url.to_owned();
    std::thread::spawn(move || {
        let body = hidden("curl.exe")
            .args(["-sfL", "--max-time", "20", "-H", "User-Agent: NetConnect", "-H", "Accept: application/vnd.github+json", &url])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        push(Event::Fetched(body));
    });
}

pub fn open_url(url: &str) {
    let _ = hidden("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).spawn();
}

pub fn clipboard_text() -> String {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;
    // SAFETY: the clipboard is opened and closed here; the locked memory is read up to its NUL.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return String::new();
        }
        let mut text = String::new();
        let data = GetClipboardData(CF_UNICODETEXT);
        if !data.is_null() {
            let ptr = GlobalLock(data) as *const u16;
            if !ptr.is_null() {
                let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
                text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                GlobalUnlock(data);
            }
        }
        CloseClipboard();
        text
    }
}

pub fn copy_text(text: &str) {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;
    let data = wide(text);
    // SAFETY: the clipboard owns the memory after SetClipboardData succeeds.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        if !memory.is_null() {
            let ptr = GlobalLock(memory) as *mut u16;
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
                GlobalUnlock(memory);
                SetClipboardData(CF_UNICODETEXT, memory);
            }
        }
        CloseClipboard();
    }
}

/// "ru", "en"...
pub fn system_language() -> String {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    // SAFETY: the buffer is LOCALE_NAME_MAX_LENGTH long.
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    let name = String::from_utf16_lossy(&buffer[..len.max(1) as usize - 1]);
    name.split('-').next().unwrap_or_default().to_owned()
}

/// "Xray-core v26.11.9", from the bundled engine.
pub fn core_version() -> String {
    hidden(engine_dir().join("xray.exe"))
        .arg("version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().nth(1).map(|v| format!("Xray-core v{v}")))
        .unwrap_or_default()
}

// ------------------------------------------------------------------ the elevated helper

/// `--tunnel <config> --parent <pid> --stop <event> --ready <event>`: runs Xray with the config
/// until the app sets the stop event or exits; sets the ready event once Xray has started.
/// Exit code 0 when stopped, 2 when Xray ended by itself.
pub fn tunnel_helper(args: &[String]) -> i32 {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
    let (config, parent, stop, ready) = (arg("--tunnel"), arg("--parent"), arg("--stop"), arg("--ready"));
    let log_path = Path::new(&config).with_file_name("engine.log");
    let mut log = std::fs::File::create(&log_path).ok();
    let Ok(mut child) = hidden(engine_dir().join("xray.exe"))
        .args(["run", "-c", &config])
        .current_dir(engine_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return 2;
    };
    kill_with_this_process(child.as_raw_handle());
    // SAFETY: plain Win32 calls on handles owned by this function.
    let (parent_handle, stop_handle, ready_handle) = unsafe {
        (
            OpenProcess(SYNCHRONIZE, 0, parent.parse().unwrap_or(0)),
            OpenEventW(SYNCHRONIZE | EVENT_MODIFY_STATE, 0, wide(&stop).as_ptr()),
            OpenEventW(SYNCHRONIZE | EVENT_MODIFY_STATE, 0, wide(&ready).as_ptr()),
        )
    };
    // Xray's log: "Xray ... started" means the tunnel is up.
    let ready_handle = ready_handle as usize;
    let lines = |stream: Box<dyn std::io::Read + Send>, mut log: Option<std::fs::File>| {
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if let Some(log) = log.as_mut() {
                    let _ = writeln!(log, "{line}");
                }
                if line.contains(" started") && ready_handle != 0 {
                    // SAFETY: the event handle stays open while the helper runs.
                    unsafe { SetEvent(ready_handle as HANDLE) };
                }
            }
        })
    };
    let err_log = log.as_ref().and_then(|l| l.try_clone().ok());
    if let Some(out) = child.stdout.take() {
        lines(Box::new(out), log.take());
    }
    if let Some(err) = child.stderr.take() {
        lines(Box::new(err), err_log);
    }
    let handles = [child.as_raw_handle() as HANDLE, stop_handle, parent_handle];
    let count = if parent_handle.is_null() { 2 } else { 3 };
    // SAFETY: the handles are valid (a null stop handle only makes the wait return at once).
    let woke = unsafe { WaitForMultipleObjects(count, handles.as_ptr(), 0, INFINITE) };
    let engine_ended = woke == WAIT_OBJECT_0;
    let _ = child.kill();
    let _ = child.wait();
    if engine_ended { 2 } else { 0 }
}

/// Xray dies with the helper, also when the helper is killed (a job object).
fn kill_with_this_process(process: std::os::windows::io::RawHandle) {
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    // SAFETY: the job handle is kept open for the helper's lifetime on purpose (leaked).
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        AssignProcessToJobObject(job, process as HANDLE);
    }
}
