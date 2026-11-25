//! VLESS: a VPN client for VLESS + XTLS Vision + REALITY servers. The UI is DrawnUI for Rust; on
//! Android the tunnel is the system VPN (ConnectionService, Java) running Xray-core.

mod link;
mod texts;
mod update;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
use android as platform;
#[cfg(any(target_os = "ios", target_os = "macos"))]
mod apple;
#[cfg(any(target_os = "ios", target_os = "macos"))]
use apple as platform;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as platform;
/// Windows: the elevated tunnel helper (`--tunnel ...`), started by the app itself on Connect.
#[cfg(windows)]
pub fn tunnel_helper(args: &[String]) -> i32 {
    win::tunnel_helper(args)
}
#[cfg(not(any(target_os = "android", target_os = "ios", target_os = "macos", windows)))]
mod platform {
    //! Desktop: the UI runs, there is no tunnel.
    use super::Event;
    pub fn take_events() -> Vec<Event> {
        Vec::new()
    }
    pub fn set_waker(_waker: drawnui::Waker, _id: drawnui::prelude::ControlId) {}
    pub fn connect(_config: String, _notification: &str) {}
    pub fn disconnect() {}
    pub fn test() {}
    pub fn copy_text(_text: &str) {}
    pub fn fetch(_url: &str) {}
    pub fn open_url(_url: &str) {}
    pub fn clipboard_text() -> String {
        String::new()
    }
    pub fn data_dir() -> Option<std::path::PathBuf> {
        None
    }
    pub fn system_language() -> String {
        String::new()
    }
    pub fn core_version() -> String {
        String::new()
    }
}

use drawnui::prelude::*;
use link::Server;
use texts::{Lang, Texts};

// Colors: a dark night-blue theme with one accent per VPN state.
const BG: u32 = 0x0B1220;
const SURFACE: u32 = 0x141D2E;
const SURFACE_HI: u32 = 0x1C2840;
const LINE: u32 = 0x26324A;
const TEXT: u32 = 0xE8EEF7;
const MUTED: u32 = 0x8A97AD;
const ACCENT: u32 = 0x3B82F6;
const GREEN: u32 = 0x22C55E;
const AMBER: u32 = 0xF59E0B;
const RED: u32 = 0xEF4444;

// Material Icons (assets/fonts/icons.ttf, a subset).
const ICON_POWER: &str = "\u{e8ac}";
const ICON_ADD: &str = "\u{e145}";
const ICON_REFRESH: &str = "\u{e5d5}";
const ICON_EXPAND: &str = "\u{e5cf}";
const ICON_TEST: &str = "\u{e640}";
const ICON_SHARE: &str = "\u{e80d}";
const ICON_DELETE: &str = "\u{e872}";
const ICON_SHIELD: &str = "\u{e32a}";
const ICON_SETTINGS: &str = "\u{e8b8}";
const ICON_BACK: &str = "\u{e5c4}";
const ICON_CHECK: &str = "\u{e5ca}";
const ICON_OPEN: &str = "\u{e89e}";
const ICON_CODE: &str = "\u{e86f}";
const ICON_BUG: &str = "\u{e868}";

const REPO: &str = "https://github.com/taublast/DrawnVpnClient";

const ROUTE_SETTINGS: &str = "settings";

/// This repository's releases (all platforms), asked at start and at most once a day.
const RELEASES: &str = "https://api.github.com/repos/taublast/DrawnVpnClient/releases?per_page=20";
/// The release file for this build's phones, short to type and share: netconnect-010-arm64.apk and
/// netconnect-010-armv7.apk for 0.1.0.
#[cfg(all(not(target_arch = "arm"), not(target_os = "macos"), not(windows)))]
const RELEASE_ASSET: &str = "-arm64.apk";
#[cfg(target_arch = "arm")]
const RELEASE_ASSET: &str = "-armv7.apk";
/// macOS: one universal app, netconnect-013-macos.dmg for 0.1.3.
#[cfg(target_os = "macos")]
const RELEASE_ASSET: &str = "-macos.dmg";
/// Windows: netconnect-013-windows-x64.zip / -windows-arm64.zip for 0.1.3.
#[cfg(all(windows, not(target_arch = "aarch64")))]
const RELEASE_ASSET: &str = "-windows-x64.zip";
#[cfg(all(windows, target_arch = "aarch64"))]
const RELEASE_ASSET: &str = "-windows-arm64.zip";
/// iOS installs come through TestFlight or Ad Hoc: no update check there, and no Settings section.
const UPDATES: bool = !cfg!(target_os = "ios");
const UPDATE_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// The height of every control (buttons, fields, switch rows, menu lines), points: one size
/// everywhere, big enough for a finger.
const CONTROL_HEIGHT: f32 = 50.0;

fn rgb(v: u32) -> Color {
    Color::new(0xFF00_0000 | v)
}

/// A clickable control's color: its state's color `v`, 10 % lighter (toward white) while the mouse
/// is over it, so the hover shows on top of the state.
fn shade(v: u32, hovered: bool) -> Color {
    if !hovered {
        return rgb(v);
    }
    let light = |c: u32| c + (255 - c) / 10;
    rgb(light(v >> 16 & 0xFF) << 16 | light(v >> 8 & 0xFF) << 8 | light(v & 0xFF))
}

/// A clickable control with no background of its own: none, or a light wash of the page color
/// while the mouse is over it.
fn hover_wash(hovered: bool) -> Color {
    hover_wash_on(BG, hovered)
}

/// The same over another background (a menu panel's).
fn hover_wash_on(background: u32, hovered: bool) -> Color {
    if hovered { shade(background, true) } else { Color::TRANSPARENT }
}

/// Mouse hover (desktop): the control under the pointer goes into the app state, where its
/// observer reads it to shade its color. Touch never hovers, so phones look as before.
fn hoverable<T: Control>(b: Build<T>) -> Build<T> {
    b.on_hovered(|me, app: &mut App, _cx, on| {
        let id = me.id();
        if on {
            app.hovered = Some(id);
        } else if app.hovered == Some(id) {
            app.hovered = None;
        }
    })
}

/// What the Java side reports (MainActivity.nativeEvent).
pub enum Event {
    State(i32, String),
    /// The answer of `platform::fetch`, "" when it failed.
    Fetched(String),
    /// macOS: Connect / Disconnect from the menu bar icon.
    #[cfg(target_os = "macos")]
    Toggle,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Vpn {
    #[default]
    Off,
    Connecting,
    Connected,
    Error,
    NoResponse,
}

impl Vpn {
    fn from_code(code: i32) -> Vpn {
        match code {
            1 => Vpn::Connecting,
            2 => Vpn::Connected,
            3 => Vpn::Error,
            4 => Vpn::NoResponse,
            _ => Vpn::Off,
        }
    }
    fn color(self) -> u32 {
        match self {
            Vpn::Off => TEXT,
            Vpn::Connecting => AMBER,
            Vpn::Connected => GREEN,
            Vpn::Error | Vpn::NoResponse => RED,
        }
    }
    fn title(self, t: &Texts) -> &'static str {
        match self {
            Vpn::Off => t.not_connected,
            Vpn::Connecting => t.connecting,
            Vpn::Connected => t.connected,
            Vpn::Error => t.could_not_connect,
            Vpn::NoResponse => t.no_answer,
        }
    }
    /// The energy core's state weights: idle, connecting, connected, error.
    fn core_weights(self) -> [f32; 4] {
        match self {
            Vpn::Off => [1.0, 0.0, 0.0, 0.0],
            Vpn::Connecting => [0.0, 1.0, 0.0, 0.0],
            Vpn::Connected => [0.0, 0.0, 1.0, 0.0],
            Vpn::Error | Vpn::NoResponse => [0.0, 0.0, 0.0, 1.0],
        }
    }
    /// The power icon on the energy core.
    fn icon_color(self) -> u32 {
        match self {
            Vpn::Off => MUTED,
            Vpn::Connecting => 0xFCD34D,
            Vpn::Connected => 0xF0FDF4,
            Vpn::Error | Vpn::NoResponse => 0xFCA5A5,
        }
    }
    /// The tunnel is up or coming up: the big button disconnects.
    fn is_on(self) -> bool {
        matches!(self, Vpn::Connecting | Vpn::Connected | Vpn::NoResponse)
    }
}

#[derive(Default)]
struct App {
    servers: Vec<Server>,
    selected: usize,
    vpn: Vpn,
    /// From the service: the delay through the server in ms, or the error.
    detail: String,
    /// The last import or delete message.
    notice: String,
    /// A connection test is running.
    testing: bool,
    /// The open server menu or delete question.
    popup: Option<PopupId>,
    /// The link field of the "Add server" dialog, and whether its text is no vless:// link.
    entry: Handle<SkiaEditor>,
    entry_text: String,
    entry_error: bool,
    /// The server the menu was opened for, and its card: the delete question opens there too.
    menu_for: Option<(usize, ControlId)>,
    /// The clickable control under the mouse pointer (desktop; touch never hovers).
    hovered: Option<ControlId>,
    lang: Lang,
    /// A newer release on GitHub, the version the user put off with Later, the setting, and when
    /// GitHub was last asked.
    update: Option<update::Update>,
    skipped_update: String,
    check_updates: bool,
    last_update_check: Option<std::time::Instant>,
    /// "Check now" was tapped: its answer is told in a toast.
    manual_check: bool,
    /// The phone's language code.
    system_lang: String,
    core_version: String,
    shell: Handle<SkiaShell>,
    mailbox: Handle<Mailbox>,
    list: Handle<SkiaLayout>,
    /// The connect button's energy core: the shader layer, the state weights it shows now
    /// (idle, connecting, connected, error) and its running animations. It draws only while
    /// something changes: a tween between states, the connecting spin, the error shockwave.
    core: Handle<SkiaLayout>,
    core_weights: std::rc::Rc<std::cell::Cell<[f32; 4]>>,
    core_tween: Option<AnimationId>,
    core_spin: Option<AnimationId>,
    core_shock: Option<AnimationId>,
}

impl App {
    fn t(&self) -> &'static Texts {
        self.lang.texts(&self.system_lang)
    }

    fn load() -> App {
        let mut app = App { system_lang: platform::system_language(), check_updates: UPDATES, ..App::default() };
        let Some(dir) = platform::data_dir() else { return app };
        if let Ok(text) = std::fs::read_to_string(dir.join("servers.txt")) {
            let mut lines = text.lines();
            let selected = lines.next().and_then(|l| l.trim().parse().ok()).unwrap_or(0);
            app.servers = lines.filter_map(|l| Server::parse(l).ok()).collect();
            app.selected = selected.min(app.servers.len().saturating_sub(1));
        }
        // settings.txt: key=value lines.
        for line in std::fs::read_to_string(dir.join("settings.txt")).unwrap_or_default().lines() {
            match line.split_once('=') {
                Some(("lang", v)) => app.lang = Lang::from_code(v.trim()),
                Some(("updates", v)) => app.check_updates = UPDATES && v.trim() != "0",
                Some(("skip", v)) => app.skipped_update = v.trim().to_owned(),
                _ => {}
            }
        }
        app
    }

    fn save(&self) {
        let Some(dir) = platform::data_dir() else { return };
        let mut text = format!("{}\n", self.selected);
        for s in &self.servers {
            text.push_str(&s.link);
            text.push('\n');
        }
        let settings = format!("lang={}\nupdates={}\nskip={}\n", self.lang.code(), u8::from(self.check_updates), self.skipped_update);
        let saved = std::fs::write(dir.join("servers.txt"), text).and_then(|_| std::fs::write(dir.join("settings.txt"), settings));
        if let Err(e) = saved {
            eprintln!("save failed: {e}");
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut Cx) {
        match event {
            Event::State(code, text) => {
                let was_on = self.vpn.is_on();
                self.set_vpn(Vpn::from_code(code), cx);
                if self.vpn.is_on() != was_on {
                    // The card of the server in use shows (or stops showing) as selected.
                    cx.items_reset(self.list);
                }
                self.detail = text;
                self.testing = false;
                // Sent at start and whenever the app comes back to the screen.
                self.check_for_updates();
            }
            Event::Fetched(json) => {
                self.update = update::newer(&json, env!("CARGO_PKG_VERSION"), RELEASE_ASSET);
                if std::mem::take(&mut self.manual_check) {
                    let t = self.t();
                    let text = match &self.update {
                        _ if json.is_empty() => t.update_check_failed.to_owned(),
                        Some(update) => {
                            // Asked for by hand: a version put off with Later is offered again.
                            if self.skipped_update == update.version {
                                self.skipped_update.clear();
                                self.save();
                            }
                            Texts::fill(t.update_available, &update.version)
                        }
                        None => t.up_to_date.to_owned(),
                    };
                    cx.show_toast(self.shell, &text, 2500.0);
                }
            }
            #[cfg(target_os = "macos")]
            Event::Toggle => self.toggle(cx),
        }
    }

    /// Under the state: the server's name, its delay, or what went wrong.
    fn status_detail(&self) -> String {
        let t = self.t();
        match self.vpn {
            _ if self.testing && self.vpn.is_on() => t.testing.to_owned(),
            Vpn::Off | Vpn::Connecting => self.servers.get(self.selected).map_or(t.no_server.to_owned(), |s| s.name.clone()),
            Vpn::Connected => Texts::fill(t.answers_in, &self.detail),
            Vpn::NoResponse => t.no_answer_detail.to_owned(),
            Vpn::Error if self.detail == "permission" => t.permission_denied.to_owned(),
            Vpn::Error if self.detail == "extension" => t.allow_extension.to_owned(),
            Vpn::Error => self.detail.clone(),
        }
    }

    /// Adds every vless:// link in `text` (one per line), selects the last one added.
    fn import(&mut self, text: &str, cx: &mut Cx) {
        let t = self.t();
        let (mut added, mut updated) = (Vec::new(), Vec::new());
        let mut bad = false;
        // While the tunnel runs, the selected server is the connected one: it stays selected.
        let select = |app: &mut App, at: usize| {
            if !app.vpn.is_on() {
                app.selected = at;
            }
        };
        for line in text.split_whitespace() {
            let Ok(server) = Server::parse(line) else {
                bad = true;
                continue;
            };
            // The same server (user, address, port): a new link replaces its name and settings.
            match self.servers.iter().position(|s| s.is_same_server(&server)) {
                Some(at) => {
                    if self.servers[at].link != server.link {
                        updated.push(server.name.clone());
                        self.servers[at] = server;
                    }
                    select(self, at);
                }
                None => {
                    added.push(server.name.clone());
                    self.servers.push(server);
                    select(self, self.servers.len() - 1);
                }
            }
        }
        self.notice = match (added.len(), updated.len()) {
            (0, 0) if text.trim().is_empty() => t.clipboard_empty.to_owned(),
            (0, 0) if bad => t.not_a_link.to_owned(),
            (0, 0) => t.already_added.to_owned(),
            (1, 0) => Texts::fill(t.added, &added[0]),
            (0, 1) => Texts::fill(t.updated, &updated[0]),
            (a, u) => Texts::fill(t.added_many, a + u),
        };
        cx.items_reset(self.list);
        self.save();
    }

    /// The server at `index` is the one the tunnel runs through.
    fn is_connected_to(&self, index: usize) -> bool {
        self.vpn.is_on() && index == self.selected
    }

    /// A card shows as selected only for the server in use (connecting or connected) and while
    /// its menu is open; with nothing connected no card is marked.
    fn highlighted(&self, index: usize) -> bool {
        self.is_connected_to(index) || (self.popup.is_some() && self.menu_for.map(|(menu, _)| menu) == Some(index))
    }

    fn delete(&mut self, index: usize, cx: &mut Cx) {
        if index >= self.servers.len() || self.is_connected_to(index) {
            return;
        }
        let removed = self.servers.remove(index);
        if self.selected > index || self.selected >= self.servers.len() {
            self.selected = self.selected.saturating_sub(1);
        }
        self.notice = Texts::fill(self.t().removed, &removed.name);
        cx.items_reset(self.list);
        self.save();
    }

    /// The big button: connects the selected server, or disconnects.
    /// The connection state, and the energy core following it.
    fn set_vpn(&mut self, vpn: Vpn, cx: &mut Cx) {
        if vpn == self.vpn {
            return;
        }
        self.vpn = vpn;
        let core = self.core;
        // Fade the state weights over 400 ms from what shows now.
        let from = self.core_weights.get();
        let to = vpn.core_weights();
        let weights = self.core_weights.clone();
        if let Some(a) = self.core_tween.take() {
            cx.stop_animation(a);
        }
        let tween = cx.animate(core, 400.0, easing::cubic_out, move |v, cx| {
            let w: [f32; 4] = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * v);
            weights.set(w);
            if let Some(mut layer) = cx.get_mut(core)
                && let Some(effect) = layer.effect_mut::<SkiaShaderEffect>()
            {
                for (name, value) in ["uIdle", "uBusy", "uOn", "uErr"].into_iter().zip(w) {
                    effect.set_uniform(name, &[value]);
                }
            }
        });
        self.core_tween = Some(tween);
        // The spin runs while connecting and through the fade out of it, then the clock stops.
        if vpn == Vpn::Connecting {
            if self.core_spin.is_none() {
                self.core_spin = Some(cx.animate_shaders(core));
            }
        } else if self.core_spin.is_some() {
            cx.on_finished(tween, |app: &mut App, cx| {
                if app.vpn != Vpn::Connecting
                    && let Some(spin) = app.core_spin.take()
                {
                    cx.stop_animation(spin);
                }
            });
        }
        // One shockwave on an error, then a still red rim.
        if let Some(a) = self.core_shock.take() {
            cx.stop_animation(a);
        }
        if vpn.core_weights()[3] > 0.0 {
            let shock = cx.animate(core, 1900.0, easing::linear, move |v, cx| {
                if let Some(mut layer) = cx.get_mut(core)
                    && let Some(effect) = layer.effect_mut::<SkiaShaderEffect>()
                {
                    effect.set_uniform("uErrorAge", &[v * 1.9]);
                }
            });
            self.core_shock = Some(shock);
        }
    }

    fn toggle(&mut self, cx: &mut Cx) {
        if self.vpn.is_on() {
            self.notice.clear();
            platform::disconnect();
        } else if self.servers.is_empty() {
            self.notice = self.t().add_server_first.to_owned();
        } else {
            self.connect(self.selected, cx);
        }
    }

    /// Connects through the server at `index` (from another one: the service switches).
    /// The server moves to the top: the list is ordered by last connection.
    fn connect(&mut self, index: usize, cx: &mut Cx) {
        if index >= self.servers.len() {
            return;
        }
        let server = self.servers.remove(index);
        let config = server.xray_config();
        self.servers.insert(0, server);
        self.selected = 0;
        self.notice.clear();
        cx.items_reset(self.list);
        self.save();
        self.set_vpn(Vpn::Connecting, cx);
        self.detail.clear();
        platform::connect(config, self.t().vpn_is_on);
    }

    /// Copies the server's link to the clipboard and says so.
    fn share(&mut self, index: usize, cx: &mut Cx) {
        let Some(server) = self.servers.get(index) else { return };
        platform::copy_text(&server.link);
        cx.show_toast(self.shell, self.t().link_copied, 2000.0);
    }

    fn test(&mut self) {
        self.testing = true;
        platform::test();
    }

    fn set_language(&mut self, lang: Lang, cx: &mut Cx) {
        self.lang = lang;
        #[cfg(target_os = "macos")]
        platform::menu_texts(self.t());
        // Messages were written in the old language.
        self.notice.clear();
        cx.items_reset(self.list);
        self.save();
    }

    /// Import: the clipboard's links when it has new ones, else the dialog to paste or type one.
    fn import_tapped(&mut self, cx: &mut Cx) {
        let clipboard = platform::clipboard_text();
        let new = clipboard
            .split_whitespace()
            .filter_map(|l| Server::parse(l).ok())
            .any(|s| !self.servers.iter().any(|known| known.link == s.link));
        if new {
            self.import(&clipboard, cx);
            return;
        }
        self.notice.clear();
        self.entry_error = false;
        self.entry_text.clear();
        let options = PopupOptions { animated: true, close_when_background_tapped: true, show_overlay: true, background_color: None };
        cx.open_popup(self.shell, entry_dialog(self), options);
        // The field exists from the next frame on: the keyboard comes up there.
        cx.postpone_before_draw::<App>(|app, cx| cx.focus(Some(app.entry.id())));
    }

    /// The dialog's Add: imports its link, or says it is none.
    fn entry_add(&mut self, text: &str, cx: &mut Cx) {
        if !text.split_whitespace().any(|l| Server::parse(l).is_ok()) {
            self.entry_error = true;
            return;
        }
        cx.focus(None::<ControlId>);
        cx.close_popup(self.shell, true);
        self.import(text, cx);
    }

    /// Asks GitHub for the latest release, at most once a day while the setting is on.
    fn check_for_updates(&mut self) {
        if self.check_updates && self.last_update_check.is_none_or(|t| t.elapsed() >= UPDATE_CHECK_EVERY) {
            self.last_update_check = Some(std::time::Instant::now());
            platform::fetch(RELEASES);
        }
    }

    /// Settings' "Check now": asks GitHub at once, whatever the daily timer says.
    fn check_now(&mut self) {
        if self.manual_check {
            return;
        }
        self.manual_check = true;
        self.last_update_check = Some(std::time::Instant::now());
        platform::fetch(RELEASES);
    }

    /// The update to show: not the version put off with Later.
    fn offered(&self) -> Option<&update::Update> {
        self.update.as_ref().filter(|u| self.check_updates && u.version != self.skipped_update)
    }

    fn close_popup(&mut self, cx: &mut Cx) {
        if let Some(popup) = self.popup.take() {
            cx.close_popup_window(popup);
            cx.items_reset(self.list);
        }
    }
}

/// Wakes on events from the Java side and hands them to the app state on the UI thread.
#[derive(Default)]
struct Mailbox;

impl Control for Mailbox {
    fn on_props_changed(&mut self, cx: &mut Cx) {
        for event in platform::take_events() {
            cx.postpone_before_draw::<App>(move |app, cx| app.on_event(event, cx));
        }
    }
}

/// The shell: the main screen as its root, Settings pushed over it. Its own nav bar is hidden:
/// each page has a header in the app's language.
fn build(app: &mut App) -> Build<SkiaLayout> {
    SkiaLayer::new().fill().background_color(rgb(BG)).children((
        SkiaShell::new()
            .fill()
            .nav_bar_height(0.0)
            .page_background_color(rgb(BG))
            .assign(&mut app.shell)
            .root(main_page(app))
            .route(ROUTE_SETTINGS, |app: &mut App, _arguments: &ShellArguments| settings_page(app)),
        Build::new(Mailbox).assign(&mut app.mailbox),
    ))
}

// ---------------------------------------------------------------- main page

fn main_page(app: &mut App) -> Build<SkiaLayout> {
    SkiaGrid::new()
        .fill()
        .row_definitions("Auto,Auto,Auto,Auto,*,Auto")
        .column_definitions("*")
        .padding((20, 12, 20, 16))
        .row_spacing(0)
        .children((header().row(0), update_banner().row(1), power_button(app).row(2), status().row(3), servers(app).row(4), actions().row(5)))
}

/// A round icon button for headers (settings, back).
fn icon_button(icon: &'static str) -> Build<SkiaShape> {
    hoverable(SkiaShape::new())
        .shape_type(ShapeType::Circle)
        .width_request(44)
        .lock_ratio(1)
        .vertical_options(LayoutOptions::Center)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .observe(|me, app: &App| me.set_background_color(hover_wash(app.hovered == Some(me.id()))))
        .children(SkiaLabel::new(icon).font_family("FontIcons").font_size(24).text_color(rgb(TEXT)).center().accessibility_role(Aria::PRESENTATION))
}

fn header() -> Build<SkiaLayout> {
    SkiaLayer::new().fill_x().padding((0, 8, 0, 0)).children((
        SkiaRow::new().spacing(8).vertical_options(LayoutOptions::Center).children((
            SkiaLabel::new(ICON_SHIELD).font_family("FontIcons").font_size(22).text_color(rgb(ACCENT)).vertical_options(LayoutOptions::Center),
            SkiaLabel::new("Net Connect")
                .font_family("FontTextBold")
                .font_size(20)
                .text_color(rgb(TEXT))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::HEADING),
        )),
        icon_button(ICON_SETTINGS)
            .horizontal_options(LayoutOptions::End)
            .margin((0, 0, -10, 0))
            .observe(|me, app: &App| me.set_accessibility_label(app.t().settings))
            .on_tapped(|_me, app: &mut App, cx| cx.go_to(app.shell, ROUTE_SETTINGS, true)),
    ))
}

/// A newer release on GitHub: Download opens its APK in the browser, Later hides this version.
fn update_banner() -> Build<SkiaShape> {
    let button = |text: fn(&Texts) -> &'static str, color: u32, download: bool| {
        hoverable(SkiaShape::new())
            .corner_radius(10)
            .height_request(CONTROL_HEIGHT)
            .fill_x()
            .background_color(rgb(color))
            .animation_tapped(SkiaTouchAnimation::Ripple)
            .accessibility_role(Aria::BUTTON)
            .observe(move |me, app: &App| {
                me.set_accessibility_label(text(app.t()));
                me.set_background_color(shade(color, app.hovered == Some(me.id())));
            })
            .on_tapped(move |_me, app: &mut App, _cx| {
                let Some(update) = app.offered().cloned() else { return };
                if download {
                    platform::open_url(&update.url);
                } else {
                    app.skipped_update = update.version;
                    app.save();
                }
            })
            .children(
                SkiaLabel::new("")
                    .font_family("FontTextBold")
                    .font_size(14)
                    .text_color(rgb(TEXT))
                    .center()
                    .accessibility_role(Aria::PRESENTATION)
                    .observe(move |me, app: &App| me.set_text(text(app.t()))),
            )
    };
    SkiaShape::new()
        .corner_radius(14)
        .fill_x()
        .margin((0, 16, 0, 0))
        .background_color(rgb(SURFACE_HI))
        .stroke_color(rgb(ACCENT))
        .stroke_width(1)
        .accessibility_role(Aria::REGION)
        .observe(|me, app: &App| me.set_is_visible(app.offered().is_some()))
        .children(SkiaStack::new().fill_x().padding((16, 12)).spacing(10).children((
            SkiaLabel::new("")
                .font_size(15)
                .text_color(rgb(TEXT))
                .fill_x()
                .accessibility_live("polite")
                .observe(|me, app: &App| {
                    if let Some(update) = app.offered() {
                        me.set_text(Texts::fill(app.t().update_available, &update.version));
                    }
                }),
            SkiaGrid::new()
                .column_definitions("*,*")
                .column_spacing(10)
                .children((button(|t| t.download, ACCENT, true).column(0), button(|t| t.later, SURFACE, false).column(1))),
        )))
}

/// The energy core's disc and the square it glows in, points.
const CORE_DISC: f32 = 184.0;
const CORE_SIZE: f32 = 240.0;

/// The big round button: the energy core shader (glass when off, a spinning plasma ring while
/// connecting, a green core when connected, a red rim and one shockwave on an error), with the
/// power icon and the tap target on top. The shader layer draws only when its uniforms or its
/// clock change, so a steady state costs nothing.
fn power_button(app: &mut App) -> Build<SkiaLayout> {
    let weights = Vpn::Off.core_weights();
    app.core_weights.set(weights);
    SkiaLayer::new()
        .width_request(CORE_SIZE)
        .height_request(CORE_SIZE)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 8, 0, 0))
        .children((
            SkiaLayer::new()
                .fill()
                .visual_effect(
                    SkiaShaderEffect::new()
                        .shader_code(include_str!("../assets/shaders/energy_core.sksl"))
                        .use_background(UseBackground::Never)
                        .uniform("uIdle", &[weights[0]])
                        .uniform("uBusy", &[weights[1]])
                        .uniform("uOn", &[weights[2]])
                        .uniform("uErr", &[weights[3]])
                        .uniform("uErrorAge", &[2.0])
                        .uniform("uDisc", &[CORE_DISC / CORE_SIZE]),
                )
                .accessibility_role(Aria::PRESENTATION)
                .assign(&mut app.core),
            hoverable(SkiaShape::new())
                .shape_type(ShapeType::Circle)
                .width_request(CORE_DISC)
                .height_request(CORE_DISC)
                .center()
                .animation_tapped(SkiaTouchAnimation::Ripple)
                .accessibility_role(Aria::BUTTON)
                .observe(|me, app: &App| {
                    // Hover: a light wash over the disc.
                    me.set_background_color(if app.hovered == Some(me.id()) { Color::new(0x14FF_FFFF) } else { Color::TRANSPARENT });
                    me.set_accessibility_label(if app.vpn.is_on() { app.t().disconnect } else { app.t().connect });
                })
                .on_tapped(|_me, app: &mut App, cx| app.toggle(cx))
                .children(
                    SkiaLabel::new(ICON_POWER)
                        .font_family("FontIcons")
                        .font_size(84)
                        .center()
                        .accessibility_role(Aria::PRESENTATION)
                        .observe(|me, app: &App| me.set_text_color(rgb(app.vpn.icon_color()))),
                ),
        ))
}

fn status() -> Build<SkiaLayout> {
    SkiaStack::new().spacing(4).margin((0, 0, 0, 20)).children((
        SkiaLabel::new("")
            .font_family("FontTextBold")
            .font_size(22)
            .horizontal_options(LayoutOptions::Center)
            .accessibility_live("polite")
            .observe(|me, app: &App| {
                me.set_text(app.vpn.title(app.t()));
                me.set_text_color(rgb(app.vpn.color()));
            }),
        SkiaLabel::new("")
            .font_size(14)
            .text_color(rgb(MUTED))
            .horizontal_options(LayoutOptions::Center)
            .horizontal_text_alignment(TextAlignment::Center)
            .observe(|me, app: &App| me.set_text(app.status_detail())),
    ))
}

fn section_title() -> Build<SkiaLabel> {
    SkiaLabel::new("").font_size(12).font_family("FontTextBold").text_color(rgb(MUTED)).accessibility_role(Aria::HEADING)
}

fn servers(app: &mut App) -> Build<SkiaLayout> {
    SkiaStack::new().spacing(10).fill().children((
        section_title().observe(|me, app: &App| me.set_text(app.t().servers)),
        SkiaLabel::new("").font_size(14).text_color(rgb(MUTED)).fill_x().observe(|me, app: &App| {
            me.set_text(app.t().empty);
            me.set_is_visible(app.servers.is_empty());
        }),
        SkiaScroll::new().fill().content(
            SkiaLayout::column()
                .spacing(8)
                .fill_x()
                .accessibility_role(Aria::LIST)
                .assign(&mut app.list)
                .observe(|me, app: &App| me.set_accessibility_label(app.t().servers))
                .items(|app: &App| app.servers.len(), server_cell, bind_server),
        ),
    ))
}

struct CellHandles {
    card: Handle<SkiaShape>,
    name: Handle<SkiaLabel>,
    info: Handle<SkiaLabel>,
}

/// A card: the background and border show which one is selected.
fn card() -> Build<SkiaShape> {
    hoverable(SkiaShape::new()).corner_radius(14).fill_x().stroke_width(2).animation_tapped(SkiaTouchAnimation::Ripple).accessibility_role(Aria::BUTTON)
}

fn style_card(card: &mut Mut<'_, SkiaShape>, selected: bool, hovered: bool) {
    card.set_background_color(shade(if selected { SURFACE_HI } else { SURFACE }, hovered));
    card.set_stroke_color(rgb(if selected { ACCENT } else { LINE }));
}

fn server_cell() -> (Build<SkiaShape>, CellHandles) {
    let mut h = CellHandles { card: Handle::default(), name: Handle::default(), info: Handle::default() };
    let cell = card()
        .assign(&mut h.card)
        // A recycled cell is bound only when the list changes: hover restyles it here.
        .on_hovered(|me, app: &mut App, _cx, on| {
            let id = me.id();
            if on {
                app.hovered = Some(id);
            } else if app.hovered == Some(id) {
                app.hovered = None;
            }
            let selected = me.base().context_index.is_some_and(|index| app.highlighted(index));
            style_card(me, selected, on);
        })
        .on_tapped(|me, app: &mut App, cx| {
            let Some(index) = me.base().context_index else { return };
            app.close_popup(cx);
            app.menu_for = Some((index, me.id()));
            app.popup = Some(cx.open_popup_window(server_menu(app, index), me.id(), PopupWindowOptions::default()));
            // The card whose menu is open shows as selected.
            cx.items_reset(app.list);
        })
        .children(SkiaStack::new().spacing(3).padding((16, 12)).children((
            SkiaLabel::new("").font_family("FontTextBold").font_size(16).text_color(rgb(TEXT)).max_lines(1).assign(&mut h.name),
            SkiaLabel::new("").font_size(12).text_color(rgb(MUTED)).max_lines(1).assign(&mut h.info),
        )));
    (cell, h)
}

fn bind_server(h: &CellHandles, app: &App, index: usize, cx: &mut Cx) {
    let Some(server) = app.servers.get(index) else { return };
    let t = app.t();
    let selected = index == app.selected;
    if let Some(mut card) = cx.get_mut(h.card) {
        let hovered = app.hovered == Some(h.card.id());
        style_card(&mut card, app.highlighted(index), hovered);
        card.set_accessibility_label(if selected { format!("{}, {}", server.name, t.selected) } else { server.name.clone() });
    }
    if let Some(mut name) = cx.get_mut(h.name) {
        name.set_text(server.name.clone());
    }
    if let Some(mut info) = cx.get_mut(h.info) {
        info.set_text(format!("{}:{}  ·  {}", server.host, server.port, server.kind(t.no_encryption)));
    }
}

fn actions() -> Build<SkiaLayout> {
    SkiaStack::new().spacing(10).padding((0, 12, 0, 0)).children((
        SkiaLabel::new("")
            .font_size(13)
            .text_color(rgb(MUTED))
            .fill_x()
            .horizontal_text_alignment(TextAlignment::Center)
            .accessibility_live("polite")
            .observe(|me, app: &App| {
                me.set_text(app.notice.clone());
                me.set_is_visible(!app.notice.is_empty());
            }),
        import_button(),
    ))
}

/// A small pill in the middle: adds the vless:// links on the clipboard.
fn import_button() -> Build<SkiaShape> {
    pill_button(ICON_ADD, |app| app.t().import).on_tapped(|_me, app: &mut App, cx| app.import_tapped(cx))
}

/// The app's secondary button (Import, Check now): a centered outline pill with an icon and a
/// caption. Outline only: these are used rarely, the connection is what the screens are about.
fn pill_button(icon: &'static str, caption: fn(&App) -> &'static str) -> Build<SkiaShape> {
    hoverable(SkiaShape::new())
        .corner_radius(CONTROL_HEIGHT / 2.0)
        .height_request(CONTROL_HEIGHT)
        .horizontal_options(LayoutOptions::Center)
        .stroke_color(rgb(MUTED))
        .stroke_width(1.5)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .observe(move |me, app: &App| {
            me.set_accessibility_label(caption(app));
            me.set_background_color(hover_wash(app.hovered == Some(me.id())));
        })
        .children(SkiaRow::new().spacing(6).padding((16, 0, 20, 0)).vertical_options(LayoutOptions::Center).children((
            SkiaLabel::new(icon)
                .font_family("FontIcons")
                .font_size(20)
                .text_color(rgb(MUTED))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION),
            SkiaLabel::new("")
                .font_family("FontTextBold")
                .font_size(15)
                .text_color(rgb(MUTED))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(move |me, app: &App| me.set_text(caption(app))),
        )))
}

/// "Add server": a field for a vless:// link, Cancel and Add. Near the top: the keyboard takes the bottom.
fn entry_dialog(app: &mut App) -> Build<SkiaLayout> {
    let t = app.t();
    let button = |text: &'static str, color: u32, add: bool| {
        hoverable(SkiaShape::new())
            .corner_radius(10)
            .height_request(CONTROL_HEIGHT)
            .fill_x()
            .background_color(rgb(color))
            .animation_tapped(SkiaTouchAnimation::Ripple)
            .accessibility_role(Aria::BUTTON)
            .accessibility_label(text)
            .observe(move |me, app: &App| me.set_background_color(shade(color, app.hovered == Some(me.id()))))
            .on_tapped(move |_me, app: &mut App, cx| {
                if add {
                    let text = app.entry_text.clone();
                    app.entry_add(&text, cx);
                } else {
                    cx.focus(None::<ControlId>);
                    cx.close_popup(app.shell, true);
                }
            })
            .children(
                SkiaLabel::new(text)
                    .font_family("FontTextBold")
                    .font_size(15)
                    .text_color(rgb(TEXT))
                    .center()
                    .accessibility_role(Aria::PRESENTATION),
            )
    };
    // A full-page layer: the panel sits near the top (the keyboard takes the bottom), a tap
    // beside it closes the dialog.
    let close = |_me: &mut Mut<'_, SkiaLayout>, app: &mut App, cx: &mut Cx| {
        cx.focus(None::<ControlId>);
        cx.close_popup(app.shell, true);
    };
    SkiaLayer::new().fill().children((
        SkiaLayer::new().fill().on_tapped(close),
        SkiaShape::new()
        .corner_radius(16)
        .fill_x()
        .margin((20, 64, 20, 0))
        .vertical_options(LayoutOptions::Start)
        .background_color(rgb(SURFACE_HI))
        .stroke_color(rgb(LINE))
        .stroke_width(1)
        .accessibility_modal(true)
        .accessibility_label(t.add_server)
        .children(SkiaStack::new().fill_x().padding(20).spacing(14).children((
            SkiaLabel::new(t.add_server).font_family("FontTextBold").font_size(18).text_color(rgb(TEXT)).accessibility_role(Aria::HEADING),
            SkiaEditor::new()
                .fill_x()
                .font_size(14)
                .text_color(rgb(TEXT))
                .placeholder_text(t.link_placeholder)
                .placeholder_color(rgb(MUTED))
                .max_lines(4)
                .auto_height(true)
                .return_type(ReturnType::Done)
                .corner_radius(10)
                .stroke_color(rgb(LINE))
                .stroke_width(1)
                .background_color(rgb(SURFACE))
                .padding((12, 10))
                .accessibility_label(t.link_placeholder)
                .assign(&mut app.entry)
                .on_text_changed(|_me, app: &mut App, _cx, text| {
                    app.entry_text = text.to_owned();
                    app.entry_error = false;
                })
                .on_text_submitted(|_me, app: &mut App, cx, text| app.entry_add(text, cx)),
            SkiaLabel::new("")
                .font_size(13)
                .text_color(rgb(RED))
                .fill_x()
                .accessibility_live("polite")
                .observe(|me, app: &App| {
                    me.set_text(app.t().not_a_link);
                    me.set_is_visible(app.entry_error);
                }),
            SkiaGrid::new()
                .column_definitions("*,*")
                .column_spacing(10)
                .children((button(t.cancel, SURFACE, false).column(0), button(t.add, ACCENT, true).column(1))),
        )))
        // Taps on the panel stay in the panel.
        .on_tapped(|_me, _app: &mut App, _cx| {}),
    ))
}

// ---------------------------------------------------------------- server menu

/// The panel of a menu or question over a server card.
fn popup_panel() -> Build<SkiaShape> {
    SkiaShape::new()
        .corner_radius(14)
        .width_request(250)
        .background_color(rgb(SURFACE_HI))
        .stroke_color(rgb(LINE))
        .stroke_width(1)
        .padding((0, 6))
}

/// What can be done with server `index`: the main action first, removing it last.
fn server_menu(app: &App, index: usize) -> Build<SkiaShape> {
    let t = app.t();
    let mut items = Vec::new();
    if app.is_connected_to(index) {
        items.push(menu_item(ICON_POWER, t.disconnect, TEXT, |app, _index, cx| app.toggle(cx)));
        items.push(menu_item(ICON_TEST, t.test_connection, TEXT, |app, _index, _cx| app.test()));
        items.push(menu_item(ICON_SHARE, t.share, TEXT, |app, index, cx| app.share(index, cx)));
    } else {
        items.push(menu_item(ICON_POWER, t.connect, TEXT, |app, index, cx| app.connect(index, cx)));
        items.push(menu_item(ICON_SHARE, t.share, TEXT, |app, index, cx| app.share(index, cx)));
        items.push(menu_item(ICON_DELETE, t.delete, RED, |app, index, cx| {
            if let Some((_, card)) = app.menu_for {
                app.menu_for = Some((index, card));
                app.popup = Some(cx.open_popup_window(delete_question(app, index), card, PopupWindowOptions::default()));
            }
        }));
    }
    popup_panel()
        .accessibility_role(Aria::MENU)
        .accessibility_label(app.servers.get(index).map_or(String::new(), |s| s.name.clone()))
        .children(SkiaStack::new().fill_x().children(items))
}

/// A menu line: `action(app, server index, cx)` runs after the menu closed.
fn menu_item(icon: &'static str, text: &'static str, color: u32, action: fn(&mut App, usize, &mut Cx)) -> Build<SkiaLayout> {
    hoverable(SkiaRow::new())
        .fill_x()
        .height_request(CONTROL_HEIGHT)
        .padding((16, 0))
        .spacing(14)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::MENU_ITEM)
        .accessibility_label(text)
        .observe(|me, app: &App| {
            let hovered = app.hovered == Some(me.id());
            me.set_background_color(if hovered { shade(SURFACE_HI, true) } else { Color::TRANSPARENT });
        })
        .on_tapped(move |_me, app: &mut App, cx| {
            let index = app.menu_for.map(|(index, _)| index);
            app.close_popup(cx);
            if let Some(index) = index {
                action(app, index, cx);
            }
        })
        .children((
            SkiaLabel::new(icon)
                .font_family("FontIcons")
                .font_size(22)
                .text_color(rgb(color))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION),
            SkiaLabel::new(text)
                .font_size(16)
                .text_color(rgb(color))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION),
        ))
}

/// "Delete Serbia?" with Cancel and Delete.
fn delete_question(app: &App, index: usize) -> Build<SkiaShape> {
    let t = app.t();
    let question = Texts::fill(t.delete_question, app.servers.get(index).map_or("", |s| s.name.as_str()));
    let button = |text: &'static str, color: u32, delete: bool| {
        hoverable(SkiaShape::new())
            .corner_radius(10)
            .height_request(CONTROL_HEIGHT)
            .fill_x()
            .background_color(rgb(color))
            .animation_tapped(SkiaTouchAnimation::Ripple)
            .accessibility_role(Aria::BUTTON)
            .accessibility_label(text)
            .observe(move |me, app: &App| me.set_background_color(shade(color, app.hovered == Some(me.id()))))
            .on_tapped(move |_me, app: &mut App, cx| {
                let index = app.menu_for.map(|(index, _)| index);
                app.close_popup(cx);
                if delete && let Some(index) = index {
                    app.delete(index, cx);
                }
            })
            .children(
                SkiaLabel::new(text)
                    .font_family("FontTextBold")
                    .font_size(15)
                    .text_color(rgb(TEXT))
                    .center()
                    .accessibility_role(Aria::PRESENTATION),
            )
    };
    popup_panel().accessibility_modal(true).accessibility_label(question.clone()).children(
        SkiaStack::new().fill_x().padding((16, 10, 16, 10)).spacing(14).children((
            SkiaLabel::new(question).font_family("FontTextBold").font_size(16).text_color(rgb(TEXT)).fill_x(),
            SkiaGrid::new()
                .column_definitions("*,*")
                .column_spacing(10)
                .children((button(t.cancel, SURFACE, false).column(0), button(t.delete, RED, true).column(1))),
        )),
    )
}

// ---------------------------------------------------------------- settings page

/// Languages in the picker: the phone's language first, then each by its own name.
const LANGUAGES: [(Lang, &str); 3] = [(Lang::System, ""), (Lang::En, "English"), (Lang::Ru, "Русский")];

fn settings_page(app: &mut App) -> Build<SkiaLayout> {
    // Asked here, not at start: it starts the Go runtime of Xray-core.
    if app.core_version.is_empty() {
        app.core_version = platform::core_version();
    }
    SkiaStack::new().fill().padding((20, 12, 20, 16)).spacing(10).children((
        SkiaRow::new().spacing(4).margin((-10, 8, 0, 16)).children((
            icon_button(ICON_BACK)
                .observe(|me, app: &App| me.set_accessibility_label(app.t().back))
                .on_tapped(|_me, app: &mut App, cx| cx.go_back(app.shell, true)),
            SkiaLabel::new("")
                .font_family("FontTextBold")
                .font_size(20)
                .text_color(rgb(TEXT))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::HEADING)
                .observe(|me, app: &App| me.set_text(app.t().settings)),
        )),
        section_title().observe(|me, app: &App| me.set_text(app.t().language)),
        language_field(),
        section_title().margin((0, 16, 0, 0)).observe(|me, app: &App| {
            me.set_text(app.t().updates);
            me.set_is_visible(UPDATES);
        }),
        SkiaShape::new()
            .corner_radius(14)
            .fill_x()
            .background_color(rgb(SURFACE))
            .stroke_color(rgb(LINE))
            .stroke_width(2)
            .height_request(CONTROL_HEIGHT)
            .observe(|me, _app: &App| me.set_is_visible(UPDATES))
            .children(SkiaLayer::new().fill().padding((16, 0)).children((
                SkiaLabel::new("")
                    .font_size(16)
                    .text_color(rgb(TEXT))
                    .vertical_options(LayoutOptions::Center)
                    .margin((0, 0, 64, 0))
                    .accessibility_role(Aria::PRESENTATION)
                    .observe(|me, app: &App| me.set_text(app.t().check_updates)),
                SkiaSwitch::new()
                    .horizontal_options(LayoutOptions::End)
                    .vertical_options(LayoutOptions::Center)
                    .color_frame_on(rgb(ACCENT))
                    .observe(|me, app: &App| {
                        me.set_default_value(app.check_updates);
                        me.set_accessibility_label(app.t().check_updates);
                    })
                    .on_toggled(|_me, app: &mut App, _cx, on| {
                        app.check_updates = on;
                        app.save();
                        app.check_for_updates();
                    }),
            ))),
        pill_button(ICON_REFRESH, |app| if app.manual_check { app.t().checking } else { app.t().check_now })
            .margin((0, 4, 0, 0))
            .observe(|me, _app: &App| me.set_is_visible(UPDATES))
            .on_tapped(|_me, app: &mut App, _cx| app.check_now()),
        section_title().margin((0, 16, 0, 0)).observe(|me, app: &App| me.set_text(app.t().about)),
        SkiaLabel::new("")
            .font_size(14)
            .text_color(rgb(MUTED))
            .fill_x()
            .observe(|me, app: &App| me.set_text(format!("{} {}", app.t().version, env!("CARGO_PKG_VERSION")))),
        // "Lib v40, Xray-core v26.11.9": the engine's version is what matters.
        link(|app| format!("{}", app.core_version.rsplit(", ").next().unwrap_or(&app.core_version).replace(" v", " ")), "https://github.com/XTLS/Xray-core")
            .observe(|me, app: &App| me.set_is_visible(!app.core_version.is_empty())),
        link(|app| format!("{} DrawnUI for Rust", app.t().built_with), "https://github.com/DrawnUi/DrawnUi.Rust"),
        SkiaRow::new().spacing(10).horizontal_options(LayoutOptions::Center).margin((0, 12, 0, 0)).children((
            pill_button(ICON_CODE, |_| "GitHub").on_tapped(|_me, _app: &mut App, _cx| platform::open_url(REPO)),
            pill_button(ICON_BUG, |app| app.t().report_issue).on_tapped(|_me, _app: &mut App, _cx| platform::open_url(&format!("{REPO}/issues/new"))),
        )),
    ))
}

/// A line of the About section that opens a web page: accent colored with an open-link icon,
/// lighter under the mouse.
fn link(text: fn(&App) -> String, url: &'static str) -> Build<SkiaLayout> {
    // The labels color themselves from the row's hover: the row's id is known once it is built.
    let row = std::rc::Rc::new(std::cell::Cell::new(None));
    let (row_text, row_icon) = (row.clone(), row.clone());
    let color = move |app: &App, row: &std::cell::Cell<Option<ControlId>>| shade(ACCENT, row.get().is_some() && app.hovered == row.get());
    hoverable(SkiaRow::new())
        .spacing(4)
        .padding((0, 4))
        .accessibility_role(Aria::LINK)
        .observe(move |me, app: &App| {
            row.set(Some(me.id()));
            me.set_accessibility_label(text(app));
        })
        .on_tapped(move |_me, _app: &mut App, _cx| platform::open_url(url))
        .children((
            SkiaLabel::new("")
                .font_size(14)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(move |me, app: &App| {
                    me.set_text(text(app));
                    me.set_text_color(color(app, &row_text));
                }),
            SkiaLabel::new(ICON_OPEN)
                .font_family("FontIcons")
                .font_size(15)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(move |me, app: &App| me.set_text_color(color(app, &row_icon))),
        ))
}

/// A language's own name ("Phone language" in the app's language).
fn language_name(app: &App, lang: Lang) -> &'static str {
    LANGUAGES.iter().find(|(l, _)| *l == lang).map_or("", |(l, name)| if *l == Lang::System { app.t().system_language } else { name })
}

/// Settings' language: one field showing the choice, as the DrawnUI Designer's choices dropdown;
/// a tap opens the list under it. It stays one row however many languages there are.
fn language_field() -> Build<SkiaShape> {
    card()
        .height_request(CONTROL_HEIGHT)
        .observe(|me, app: &App| {
            style_card(me, false, app.hovered == Some(me.id()));
            me.set_accessibility_label(format!("{}: {}", app.t().language_label, language_name(app, app.lang)));
        })
        .on_tapped(|me, app: &mut App, cx| {
            app.close_popup(cx);
            let options = PopupWindowOptions { cpu: true, ..PopupWindowOptions::default() };
            app.popup = Some(cx.open_popup_window(language_menu(app), me.id(), options));
        })
        .children(SkiaLayer::new().fill().padding((16, 0)).children((
            SkiaLabel::new("")
                .font_size(16)
                .text_color(rgb(TEXT))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(|me, app: &App| me.set_text(language_name(app, app.lang))),
            SkiaLabel::new(ICON_EXPAND)
                .font_family("FontIcons")
                .font_size(24)
                .text_color(rgb(MUTED))
                .horizontal_options(LayoutOptions::End)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION),
        )))
}

/// The language list under the field: the phone's language first, then each by its own name; the
/// current one carries a check. A long list scrolls.
fn language_menu(_app: &App) -> Build<SkiaShape> {
    let rows: Vec<_> = LANGUAGES.into_iter().map(|(lang, _)| language_row(lang)).collect();
    popup_panel()
        .width_request(300)
        .accessibility_role(Aria::MENU)
        .children(SkiaScroll::new().maximum_height_request(420).content(SkiaStack::new().fill_x().children(rows)))
}

fn language_row(lang: Lang) -> Build<SkiaLayout> {
    hoverable(SkiaRow::new())
        .fill_x()
        .height_request(CONTROL_HEIGHT)
        .padding((16, 0))
        .spacing(14)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::MENU_ITEM_RADIO)
        .observe(move |me, app: &App| {
            me.set_background_color(hover_wash_on(SURFACE_HI, app.hovered == Some(me.id())));
            me.set_accessibility_label(language_name(app, lang));
            me.set_accessibility_is_pressed(Some(app.lang == lang));
        })
        .on_tapped(move |_me, app: &mut App, cx| {
            app.close_popup(cx);
            app.set_language(lang, cx);
        })
        .children((
            SkiaLabel::new(ICON_CHECK)
                .font_family("FontIcons")
                .font_size(22)
                .text_color(rgb(ACCENT))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(move |me, app: &App| me.set_opacity(if app.lang == lang { 1.0 } else { 0.0 })),
            SkiaLabel::new("")
                .font_size(16)
                .text_color(rgb(TEXT))
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(move |me, app: &App| me.set_text(language_name(app, lang))),
        ))
}

pub fn run() {
    // macOS: the app's files are in the bundle's Resources (only code may sit next to the
    // executable in a notarized app); DrawnUI reads them from the working folder then.
    #[cfg(target_os = "macos")]
    if let Some(resources) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.join("Resources")))
        && resources.join("assets").is_dir()
    {
        let _ = std::env::set_current_dir(resources);
    }
    // macOS: a phone-like window (the layout is the phones'); elsewhere the platform's own size.
    #[cfg(any(target_os = "macos", windows))]
    drawnui::run_sized("Net Connect", 420.0, 860.0, make_ui);
    #[cfg(not(any(target_os = "macos", windows)))]
    drawnui::run("Net Connect", make_ui);
}

fn make_ui() -> Box<dyn drawnui::App> {
    {
        // Toasts on the app's dark theme: the default near-black one would not show.
        SkiaShell::set_defaults(ShellDefaults {
            toast_background_color: rgb(SURFACE_HI),
            toast_text_color: rgb(TEXT),
            toast_text_font: "FontText".into(),
            ..SkiaShell::defaults()
        });
        let app = App::load();
        #[cfg(target_os = "macos")]
        platform::menu_texts(app.t());
        let ui = Ui::new(app, build)
            // A menu closed by a press outside or Escape: its card stops showing as selected.
            .on_popup_window_closed(|app: &mut App, popup, cx| {
                if app.popup == Some(popup) {
                    app.popup = None;
                    cx.items_reset(app.list);
                }
            })
            .font("FontText", "assets/fonts/OpenSans-Regular.ttf")
            .font_weight("FontText", "assets/fonts/OpenSans-Semibold.ttf", 600)
            .font("FontTextBold", "assets/fonts/OpenSans-Semibold.ttf")
            .font("FontIcons", "assets/fonts/icons.ttf")
            .background(rgb(BG));
        platform::set_waker(ui.waker(), ui.state.mailbox.id());
        Box::new(ui)
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    android::set_app(app.clone());
    drawnui::set_android_app(app);
    run();
}
