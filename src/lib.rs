//! VLESS: a VPN client for VLESS + XTLS Vision + REALITY servers. The UI is DrawnUI for Rust; on
//! Android the tunnel is the system VPN (XrayVpnService, Java) running Xray-core.

mod link;
mod texts;
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
use android as platform;
#[cfg(not(target_os = "android"))]
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
const ICON_TEST: &str = "\u{e640}";
const ICON_DELETE: &str = "\u{e872}";
const ICON_SHIELD: &str = "\u{e32a}";
const ICON_SETTINGS: &str = "\u{e8b8}";
const ICON_BACK: &str = "\u{e5c4}";
const ICON_CHECK: &str = "\u{e5ca}";

const ROUTE_SETTINGS: &str = "settings";

fn rgb(v: u32) -> Color {
    Color::new(0xFF00_0000 | v)
}

/// What the Java side reports (MainActivity.nativeEvent).
pub enum Event {
    State(i32, String),
    Link(String),
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
            Vpn::Connecting | Vpn::NoResponse => AMBER,
            Vpn::Connected => GREEN,
            Vpn::Error => RED,
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
    /// The server the menu was opened for, and its card: the delete question opens there too.
    menu_for: Option<(usize, ControlId)>,
    lang: Lang,
    /// The phone's language code.
    system_lang: String,
    core_version: String,
    shell: Handle<SkiaShell>,
    mailbox: Handle<Mailbox>,
    list: Handle<SkiaLayout>,
}

impl App {
    fn t(&self) -> &'static Texts {
        self.lang.texts(&self.system_lang)
    }

    fn load() -> App {
        let mut app = App { system_lang: platform::system_language(), ..App::default() };
        let Some(dir) = platform::data_dir() else { return app };
        if let Ok(text) = std::fs::read_to_string(dir.join("servers.txt")) {
            let mut lines = text.lines();
            let selected = lines.next().and_then(|l| l.trim().parse().ok()).unwrap_or(0);
            app.servers = lines.filter_map(|l| Server::parse(l).ok()).collect();
            app.selected = selected.min(app.servers.len().saturating_sub(1));
        }
        if let Ok(code) = std::fs::read_to_string(dir.join("language.txt")) {
            app.lang = Lang::from_code(code.trim());
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
        let saved = std::fs::write(dir.join("servers.txt"), text).and_then(|_| std::fs::write(dir.join("language.txt"), self.lang.code()));
        if let Err(e) = saved {
            eprintln!("save failed: {e}");
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut Cx) {
        match event {
            Event::State(code, text) => {
                self.vpn = Vpn::from_code(code);
                self.detail = text;
                self.testing = false;
            }
            Event::Link(text) => self.import(&text, cx),
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
            Vpn::Error => self.detail.clone(),
        }
    }

    /// Adds every vless:// link in `text` (one per line), selects the last one added.
    fn import(&mut self, text: &str, cx: &mut Cx) {
        let t = self.t();
        let mut added = Vec::new();
        let mut bad = false;
        for line in text.split_whitespace() {
            match Server::parse(line) {
                Ok(server) => match self.servers.iter().position(|s| s.link == server.link) {
                    Some(at) => self.selected = at,
                    None => {
                        added.push(server.name.clone());
                        self.servers.push(server);
                        self.selected = self.servers.len() - 1;
                    }
                },
                Err(_) => bad = true,
            }
        }
        self.notice = match added.len() {
            0 if text.trim().is_empty() => t.clipboard_empty.to_owned(),
            0 if bad => t.not_a_link.to_owned(),
            0 => t.already_added.to_owned(),
            1 => Texts::fill(t.added, &added[0]),
            n => Texts::fill(t.added_many, n),
        };
        cx.items_reset(self.list);
        self.save();
    }

    /// The server at `index` is the one the tunnel runs through.
    fn is_connected_to(&self, index: usize) -> bool {
        self.vpn.is_on() && index == self.selected
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
    fn connect(&mut self, index: usize, cx: &mut Cx) {
        let Some(server) = self.servers.get(index) else { return };
        let config = server.xray_config();
        self.notice.clear();
        if self.selected != index {
            self.selected = index;
            cx.items_reset(self.list);
            self.save();
        }
        self.vpn = Vpn::Connecting;
        self.detail.clear();
        platform::connect(config, self.t().vpn_is_on);
    }

    fn test(&mut self) {
        self.testing = true;
        platform::test();
    }

    fn set_language(&mut self, lang: Lang, cx: &mut Cx) {
        self.lang = lang;
        // Messages were written in the old language.
        self.notice.clear();
        cx.items_reset(self.list);
        self.save();
    }

    fn close_popup(&mut self, cx: &mut Cx) {
        if let Some(popup) = self.popup.take() {
            cx.close_popup_window(popup);
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
        .row_definitions("Auto,Auto,Auto,*,Auto")
        .column_definitions("*")
        .padding((20, 12, 20, 16))
        .row_spacing(0)
        .children((header().row(0), power_button().row(1), status().row(2), servers(app).row(3), actions().row(4)))
}

/// A round icon button for headers (settings, back).
fn icon_button(icon: &'static str) -> Build<SkiaShape> {
    SkiaShape::new()
        .shape_type(ShapeType::Circle)
        .width_request(44)
        .lock_ratio(1)
        .vertical_options(LayoutOptions::Center)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .children(SkiaLabel::new(icon).font_family("FontIcons").font_size(24).text_color(rgb(TEXT)).center().accessibility_role(Aria::PRESENTATION))
}

fn header() -> Build<SkiaLayout> {
    SkiaLayer::new().fill_x().padding((0, 8, 0, 0)).children((
        SkiaRow::new().spacing(8).vertical_options(LayoutOptions::Center).children((
            SkiaLabel::new(ICON_SHIELD).font_family("FontIcons").font_size(22).text_color(rgb(ACCENT)).vertical_options(LayoutOptions::Center),
            SkiaLabel::new("VLESS")
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

/// The big round button: gray off, amber connecting, green on.
fn power_button() -> Build<SkiaShape> {
    SkiaShape::new()
        .shape_type(ShapeType::Circle)
        .width_request(184)
        .lock_ratio(1)
        .horizontal_options(LayoutOptions::Center)
        .margin((0, 36, 0, 20))
        .background_color(rgb(SURFACE))
        .stroke_width(6)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .observe(|me, app: &App| {
            me.set_stroke_color(rgb(if app.vpn == Vpn::Off { MUTED } else { app.vpn.color() }));
            me.set_accessibility_label(if app.vpn.is_on() { app.t().disconnect } else { app.t().connect });
        })
        .on_tapped(|_me, app: &mut App, cx| app.toggle(cx))
        .children(
            SkiaLabel::new(ICON_POWER)
                .font_family("FontIcons")
                .font_size(84)
                .center()
                .accessibility_role(Aria::PRESENTATION)
                .observe(|me, app: &App| me.set_text_color(rgb(app.vpn.color()))),
        )
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
    SkiaShape::new().corner_radius(14).fill_x().stroke_width(2).animation_tapped(SkiaTouchAnimation::Ripple).accessibility_role(Aria::BUTTON)
}

fn style_card(card: &mut Mut<'_, SkiaShape>, selected: bool) {
    card.set_background_color(rgb(if selected { SURFACE_HI } else { SURFACE }));
    card.set_stroke_color(rgb(if selected { ACCENT } else { LINE }));
}

fn server_cell() -> (Build<SkiaShape>, CellHandles) {
    let mut h = CellHandles { card: Handle::default(), name: Handle::default(), info: Handle::default() };
    let cell = card()
        .assign(&mut h.card)
        .on_tapped(|me, app: &mut App, cx| {
            let Some(index) = me.base().context_index else { return };
            app.close_popup(cx);
            app.menu_for = Some((index, me.id()));
            app.popup = Some(cx.open_popup_window(server_menu(app, index), me.id(), PopupWindowOptions::default()));
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
        style_card(&mut card, selected);
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
    SkiaShape::new()
        .corner_radius(22)
        .height_request(44)
        .horizontal_options(LayoutOptions::Center)
        // Outline only: it is used rarely, the connection is what the screen is about.
        .stroke_color(rgb(MUTED))
        .stroke_width(1.5)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::BUTTON)
        .observe(|me, app: &App| me.set_accessibility_label(app.t().import))
        .on_tapped(|_me, app: &mut App, cx| app.import(&platform::clipboard_text(), cx))
        .children(SkiaRow::new().spacing(6).padding((16, 0, 20, 0)).vertical_options(LayoutOptions::Center).children((
            SkiaLabel::new(ICON_ADD)
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
                .observe(|me, app: &App| me.set_text(app.t().import)),
        )))
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
    } else {
        items.push(menu_item(ICON_POWER, t.connect, TEXT, |app, index, cx| app.connect(index, cx)));
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
    SkiaRow::new()
        .fill_x()
        .height_request(48)
        .padding((16, 0))
        .spacing(14)
        .animation_tapped(SkiaTouchAnimation::Ripple)
        .accessibility_role(Aria::MENU_ITEM)
        .accessibility_label(text)
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
        SkiaShape::new()
            .corner_radius(10)
            .height_request(40)
            .fill_x()
            .background_color(rgb(color))
            .animation_tapped(SkiaTouchAnimation::Ripple)
            .accessibility_role(Aria::BUTTON)
            .accessibility_label(text)
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
        SkiaStack::new()
            .spacing(8)
            .accessibility_role(Aria::RADIO_GROUP)
            .children(LANGUAGES.into_iter().map(|(lang, name)| language_option(lang, name)).collect::<Vec<_>>()),
        section_title().margin((0, 16, 0, 0)).observe(|me, app: &App| me.set_text(app.t().about)),
        SkiaLabel::new("").font_size(14).text_color(rgb(MUTED)).fill_x().observe(|me, app: &App| {
            let mut text = format!("{} {}", app.t().version, env!("CARGO_PKG_VERSION"));
            if !app.core_version.is_empty() {
                // "Lib v40, Xray-core v26.11.9": the engine's version is what matters.
                let core = app.core_version.rsplit(", ").next().unwrap_or(&app.core_version).replace(" v", " ");
                text.push_str(&format!("\n{core}"));
            }
            me.set_text(text);
        }),
    ))
}

fn language_option(lang: Lang, name: &'static str) -> Build<SkiaShape> {
    card()
        .accessibility_role(Aria::RADIO)
        .observe(move |me, app: &App| {
            let selected = app.lang == lang;
            style_card(me, selected);
            me.set_accessibility_is_pressed(Some(selected));
            me.set_accessibility_label(if lang == Lang::System { app.t().system_language } else { name });
        })
        .on_tapped(move |_me, app: &mut App, cx| app.set_language(lang, cx))
        .children(SkiaLayer::new().fill_x().padding((16, 14)).children((
            SkiaLabel::new(name)
                .font_size(16)
                .text_color(rgb(TEXT))
                .vertical_options(LayoutOptions::Center)
                .observe(move |me, app: &App| {
                    if lang == Lang::System {
                        me.set_text(app.t().system_language);
                    }
                }),
            SkiaLabel::new(ICON_CHECK)
                .font_family("FontIcons")
                .font_size(22)
                .text_color(rgb(ACCENT))
                .horizontal_options(LayoutOptions::End)
                .vertical_options(LayoutOptions::Center)
                .accessibility_role(Aria::PRESENTATION)
                .observe(move |me, app: &App| me.set_is_visible(app.lang == lang)),
        )))
}

pub fn run() {
    drawnui::run("VLESS", || {
        let ui = Ui::new(App::load(), build)
            .font("FontText", "assets/fonts/OpenSans-Regular.ttf")
            .font_weight("FontText", "assets/fonts/OpenSans-Semibold.ttf", 600)
            .font("FontTextBold", "assets/fonts/OpenSans-Semibold.ttf")
            .font("FontIcons", "assets/fonts/icons.ttf")
            .background(rgb(BG));
        platform::set_waker(ui.waker(), ui.state.mailbox.id());
        Box::new(ui)
    });
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: drawnui::AndroidApp) {
    android::set_app(app.clone());
    drawnui::set_android_app(app);
    run();
}
