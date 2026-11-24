//! The app's texts in every language. A new language: one more `Texts` and a line in `Lang`.

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Lang {
    /// The phone's language when the app has it, else English.
    #[default]
    System,
    En,
    Ru,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::System => "system",
            Lang::En => "en",
            Lang::Ru => "ru",
        }
    }

    pub fn from_code(code: &str) -> Lang {
        match code {
            "en" => Lang::En,
            "ru" => Lang::Ru,
            _ => Lang::System,
        }
    }

    /// The texts for this choice; `system` is the phone's language code ("ru", "en", ...).
    pub fn texts(self, system: &str) -> &'static Texts {
        match self {
            Lang::En => &EN,
            Lang::Ru => &RU,
            Lang::System if system == "ru" => &RU,
            Lang::System => &EN,
        }
    }
}

pub struct Texts {
    pub not_connected: &'static str,
    pub connecting: &'static str,
    pub connected: &'static str,
    pub could_not_connect: &'static str,
    pub no_answer: &'static str,
    pub no_answer_detail: &'static str,
    pub answers_in: &'static str,
    pub permission_denied: &'static str,
    /// macOS: the tunnel (a system extension) waits for the user to turn it on.
    pub allow_extension: &'static str,
    /// macOS menu bar: brings the app's window up; ends the app.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub open_app: &'static str,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub quit: &'static str,
    pub no_server: &'static str,
    pub servers: &'static str,
    pub empty: &'static str,
    pub import: &'static str,
    pub test_connection: &'static str,
    pub testing: &'static str,
    pub delete: &'static str,
    pub delete_question: &'static str,
    pub cancel: &'static str,
    pub add_server: &'static str,
    pub link_placeholder: &'static str,
    pub add: &'static str,
    pub share: &'static str,
    pub link_copied: &'static str,
    pub connect: &'static str,
    pub disconnect: &'static str,
    pub selected: &'static str,
    pub clipboard_empty: &'static str,
    pub not_a_link: &'static str,
    pub already_added: &'static str,
    pub added: &'static str,
    pub added_many: &'static str,
    pub updated: &'static str,
    pub removed: &'static str,
    pub add_server_first: &'static str,
    pub no_encryption: &'static str,
    pub vpn_is_on: &'static str,
    pub settings: &'static str,
    pub back: &'static str,
    pub language: &'static str,
    pub system_language: &'static str,
    pub about: &'static str,
    pub updates: &'static str,
    pub check_updates: &'static str,
    pub update_available: &'static str,
    pub download: &'static str,
    pub later: &'static str,
    pub version: &'static str,
}

impl Texts {
    /// `template` with `{}` replaced by `value`.
    pub fn fill(template: &str, value: impl std::fmt::Display) -> String {
        template.replacen("{}", &value.to_string(), 1)
    }
}

pub static EN: Texts = Texts {
    not_connected: "Not connected",
    connecting: "Connecting…",
    connected: "Connected",
    could_not_connect: "Could not connect",
    no_answer: "Connected, no answer",
    no_answer_detail: "The server does not answer",
    answers_in: "The server answers in {} ms",
    permission_denied: "The VPN permission was not given",
    allow_extension: "Turn on Net Connect in System Settings › General › Login Items & Extensions › Network Extensions",
    open_app: "Open Net Connect",
    quit: "Quit",
    no_server: "No server",
    servers: "SERVERS",
    empty: "No servers yet.\nCopy a vless:// link and tap Import.",
    import: "Import",
    test_connection: "Test connection",
    testing: "Testing…",
    delete: "Delete",
    delete_question: "Delete {}?",
    cancel: "Cancel",
    add_server: "Add server",
    link_placeholder: "Paste or type a vless:// link",
    add: "Add",
    share: "Share",
    link_copied: "Link copied to the clipboard",
    connect: "Connect",
    disconnect: "Disconnect",
    selected: "selected",
    clipboard_empty: "The clipboard is empty: copy a vless:// link first",
    not_a_link: "This is not a vless:// link",
    already_added: "This server is already in the list",
    added: "Added {}",
    added_many: "Added or updated {} servers",
    updated: "Updated {}",
    removed: "Removed {}",
    add_server_first: "Add a server first: copy its vless:// link, then tap Import",
    no_encryption: "No encryption",
    vpn_is_on: "VPN is on",
    settings: "Settings",
    back: "Back",
    language: "LANGUAGE",
    system_language: "Phone language",
    about: "ABOUT",
    updates: "UPDATES",
    check_updates: "Check for updates",
    update_available: "Net Connect {} is available",
    download: "Download",
    later: "Later",
    version: "Version",
};

pub static RU: Texts = Texts {
    not_connected: "Не подключено",
    connecting: "Подключение…",
    connected: "Подключено",
    could_not_connect: "Не удалось подключиться",
    no_answer: "Подключено, нет ответа",
    no_answer_detail: "Сервер не отвечает",
    answers_in: "Сервер отвечает за {} мс",
    permission_denied: "Разрешение на VPN не получено",
    allow_extension: "Включите Net Connect: Системные настройки › Основные › Объекты входа и расширения › Сетевые расширения",
    open_app: "Открыть Net Connect",
    quit: "Выйти",
    no_server: "Нет сервера",
    servers: "СЕРВЕРЫ",
    empty: "Серверов пока нет.\nСкопируйте ссылку vless:// и нажмите «Импорт».",
    import: "Импорт",
    test_connection: "Проверить связь",
    testing: "Проверка…",
    delete: "Удалить",
    delete_question: "Удалить {}?",
    cancel: "Отмена",
    add_server: "Добавить сервер",
    link_placeholder: "Вставьте или введите ссылку vless://",
    add: "Добавить",
    share: "Поделиться",
    link_copied: "Ссылка скопирована в буфер обмена",
    connect: "Подключить",
    disconnect: "Отключить",
    selected: "выбран",
    clipboard_empty: "Буфер обмена пуст: сначала скопируйте ссылку vless://",
    not_a_link: "Это не ссылка vless://",
    already_added: "Этот сервер уже в списке",
    added: "Добавлен {}",
    added_many: "Добавлено или обновлено серверов: {}",
    updated: "Обновлён {}",
    removed: "Удалён {}",
    add_server_first: "Сначала добавьте сервер: скопируйте его ссылку vless:// и нажмите «Импорт»",
    no_encryption: "Без шифрования",
    vpn_is_on: "VPN включён",
    settings: "Настройки",
    back: "Назад",
    language: "ЯЗЫК",
    system_language: "Язык телефона",
    about: "О ПРИЛОЖЕНИИ",
    updates: "ОБНОВЛЕНИЯ",
    check_updates: "Проверять обновления",
    update_available: "Доступна версия Net Connect {}",
    download: "Скачать",
    later: "Позже",
    version: "Версия",
};
