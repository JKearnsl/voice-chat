//! The mod's own small translation table, Simple Voice Chat-style: every
//! on-screen string is a key, looked up in the player's language (set from the
//! game's `settings.language`), falling back to English.
//!
//! Horizon doesn't hand mods a translation service, so the menu and HUD draw
//! their own text; this is where that text is localised. Templates use `{}` for
//! each argument, filled in order by [`fmt`].
use std::cell::RefCell;

/// The languages, in the order the table's columns are written. Column 0
/// (`en_us`) is the fallback for a missing language or an empty cell.
const LANGS: &[&str] = &[
    "en_us", "ru_ru", "uk_ua", "de_de", "es_es", "fr_fr", "pt_br", "zh_cn", "pl_pl", "it_it",
];

thread_local! {
    /// The current language code, lowercased (`ru_ru`). Thread-local because
    /// the menu, HUD and chat are all drawn on the game thread that sets it;
    /// this also keeps the tests from racing over one global.
    static LANG: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Sets the player's language from the game's `settings.language`
/// (`en_us`, `ru_ru`, …). Unknown languages fall back to English.
pub fn set_language(code: &str) {
    LANG.with(|lang| {
        let code = code.to_lowercase();
        if *lang.borrow() != code {
            *lang.borrow_mut() = code;
        }
    });
}

/// The column of the current language, or English's (0) when it isn't listed.
/// A regional variant that isn't listed (`de_at`) falls back to the first
/// language sharing its prefix (`de_de`), then English.
fn lang_index() -> usize {
    LANG.with(|lang| {
        let lang = lang.borrow();
        if let Some(i) = LANGS.iter().position(|l| *l == lang.as_str()) {
            return i;
        }
        if let Some(prefix) = lang.split('_').next().filter(|p| !p.is_empty())
            && let Some(i) = LANGS.iter().position(|l| l.starts_with(prefix))
        {
            return i;
        }
        0
    })
}

/// The template for `key` in the current language, English if that cell is
/// empty, or `None` if the key isn't in the table.
fn template(key: &str) -> Option<&'static str> {
    TABLE.iter().find(|(k, _)| *k == key).map(|(_, cols)| {
        let i = lang_index();
        let s = cols.get(i).copied().unwrap_or("");
        if s.is_empty() { cols[0] } else { s }
    })
}

/// `key`'s text, no arguments. A missing key shows itself (a development
/// slip, not a crash).
pub fn tr(key: &str) -> String {
    template(key).unwrap_or(key).to_string()
}

/// `key`'s text with each `{}` replaced by the next argument. Arguments may be
/// `&str`, `String` or `&String` (anything `AsRef<str>`).
pub fn fmt<S: AsRef<str>>(key: &str, args: &[S]) -> String {
    let Some(tmpl) = template(key) else {
        return key.to_string();
    };
    let mut out = String::with_capacity(tmpl.len());
    let mut rest = tmpl;
    let mut args = args.iter();
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        if let Some(a) = args.next() {
            out.push_str(a.as_ref());
        } else {
            out.push_str("{}");
        }
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

// The translations. Each row is a key and one string per language in `LANGS`'
// order; an empty string falls back to English. Keep columns aligned with
// `LANGS`. CJK draws with the game's font.
#[rustfmt::skip]
const TABLE: &[(&str, [&str; 10])] = &[
    //  key                 en_us                 ru_ru                      uk_ua                      de_de                      es_es                        fr_fr                          pt_br                        zh_cn         pl_pl                         it_it
    ("voice_chat",        ["Voice Chat",          "Голосовой чат",           "Голосовий чат",           "Sprachchat",              "Chat de voz",               "Chat vocal",                  "Chat de voz",               "语音聊天",     "Czat głosowy",               "Chat vocale"]),
    ("settings",          ["Settings",            "Настройки",               "Налаштування",            "Einstellungen",           "Ajustes",                   "Paramètres",                  "Configurações",             "设置",         "Ustawienia",                 "Impostazioni"]),
    ("groups",            ["Groups",              "Группы",                  "Групи",                   "Gruppen",                 "Grupos",                    "Groupes",                     "Grupos",                    "群组",         "Grupy",                      "Gruppi"]),
    ("group",             ["Group",               "Группа",                  "Група",                   "Gruppe",                  "Grupo",                     "Groupe",                      "Grupo",                     "群组",         "Grupa",                      "Gruppo"]),
    ("back",              ["Back",                "Назад",                   "Назад",                   "Zurück",                  "Atrás",                     "Retour",                      "Voltar",                    "返回",         "Wstecz",                     "Indietro"]),
    ("on",                ["On",                  "Вкл",                     "Увімк",                   "An",                      "Act.",                      "Oui",                         "Lig.",                      "开",           "Wł.",                        "Sì"]),
    ("off",               ["Off",                 "Выкл",                    "Вимк",                    "Aus",                     "Des.",                      "Non",                         "Des.",                      "关",           "Wył.",                       "No"]),
    ("muted",             ["Muted",               "Выкл. микр.",             "Вимк. мікр.",             "Stumm",                   "Silenciado",                "Muet",                        "Mudo",                      "静音",         "Wyciszony",                  "Muto"]),

    ("tip_mic_on",        ["Microphone on — click to mute",   "Микрофон включён — выключить", "Мікрофон увімкнено — вимкнути", "Mikrofon an — stummschalten", "Micrófono activo — silenciar", "Micro actif — couper", "Microfone ligado — mudo", "麦克风开启 — 点击静音", "Mikrofon wł. — wycisz", "Microfono attivo — muta"]),
    ("tip_mic_off",       ["Microphone muted — click to unmute", "Микрофон выключен — включить", "Мікрофон вимкнено — увімкнути", "Mikrofon stumm — aktivieren", "Micrófono en silencio — activar", "Micro coupé — activer", "Microfone mudo — ligar", "麦克风静音 — 点击开启", "Mikrofon wyciszony — włącz", "Microfono muto — attiva"]),
    ("tip_voice_on",      ["Voice chat on — click to turn off", "Голосовой чат вкл — выключить", "Голосовий чат увімк — вимкнути", "Sprachchat an — ausschalten", "Chat de voz activo — desactivar", "Chat vocal actif — désactiver", "Chat de voz ligado — desligar", "语音聊天开启 — 点击关闭", "Czat głosowy wł. — wyłącz", "Chat vocale attiva — disattiva"]),
    ("tip_voice_off",     ["Voice chat off — click to turn on", "Голосовой чат выкл — включить", "Голосовий чат вимк — увімкнути", "Sprachchat aus — einschalten", "Chat de voz inactivo — activar", "Chat vocal inactif — activer", "Chat de voz desligado — ligar", "语音聊天关闭 — 点击开启", "Czat głosowy wył. — włącz", "Chat vocale disattiva — attiva"]),
    ("tip_mode_ptt",      ["Push to talk — click for voice activation", "Рация — переключить на активацию голосом", "Рація — перемкнути на активацію голосом", "Push-to-Talk — zu Sprachaktivierung", "Pulsar para hablar — activación por voz", "Appuyer pour parler — activation vocale", "Aperte para falar — ativação por voz", "按键通话 — 切换为声控", "Naciśnij, by mówić — aktywacja głosem", "Premi per parlare — attivazione vocale"]),
    ("tip_mode_voice",    ["Voice activation — click for push to talk", "Активация голосом — переключить на рацию", "Активація голосом — перемкнути на рацію", "Sprachaktivierung — zu Push-to-Talk", "Activación por voz — pulsar para hablar", "Activation vocale — appuyer pour parler", "Ativação por voz — aperte para falar", "声控 — 切换为按键通话", "Aktywacja głosem — naciśnij, by mówić", "Attivazione vocale — premi per parlare"]),

    ("activation",        ["Activation",          "Активация",               "Активація",               "Aktivierung",             "Activación",                "Activation",                  "Ativação",                  "激活",         "Aktywacja",                  "Attivazione"]),
    ("voice_activation",  ["Voice activation",    "Активация голосом",       "Активація голосом",       "Sprachaktivierung",       "Activación por voz",        "Activation vocale",           "Ativação por voz",          "声控",         "Aktywacja głosem",           "Attivazione vocale"]),
    ("push_to_talk",      ["Push to talk",        "Рация (зажать)",          "Рація (затиснути)",       "Push-to-Talk",            "Pulsar para hablar",        "Appuyer pour parler",         "Aperte para falar",         "按键通话",     "Naciśnij, by mówić",         "Premi per parlare"]),
    ("microphone",        ["Microphone",          "Микрофон",                "Мікрофон",                "Mikrofon",                "Micrófono",                 "Microphone",                  "Microfone",                 "麦克风",       "Mikrofon",                   "Microfono"]),
    ("voice_volume",      ["Voice volume",        "Громкость голосов",       "Гучність голосів",        "Lautstärke",              "Volumen de voz",            "Volume des voix",             "Volume das vozes",          "语音音量",     "Głośność głosów",            "Volume voci"]),
    ("mic_gain",          ["Microphone gain",     "Усиление микрофона",      "Підсилення мікрофона",    "Mikrofonverstärkung",     "Ganancia de micrófono",     "Gain du micro",               "Ganho do microfone",        "麦克风增益",   "Wzmocnienie mikrofonu",      "Guadagno microfono"]),
    ("devices",           ["Devices",             "Устройства",              "Пристрої",                "Geräte",                  "Dispositivos",              "Appareils",                   "Dispositivos",              "设备",         "Urządzenia",                 "Dispositivi"]),
    ("levels",            ["Levels",              "Уровни",                  "Рівні",                   "Pegel",                   "Niveles",                   "Niveaux",                     "Níveis",                    "音量与阈值",   "Poziomy",                    "Livelli"]),
    ("player_volumes",    ["Player volumes",       "Громкость игроков",        "Гучність гравців",         "Lautstärke der Spieler",  "Volúmenes de jugadores",    "Volume des joueurs",          "Volume dos jogadores",      "玩家音量",     "Głośność graczy",            "Volume dei giocatori"]),
    ("volumes_hint",      ["Each source has its own volume.", "У каждого источника своя громкость.", "Кожне джерело має свою гучність.", "Jede Quelle hat ihre eigene Lautstärke.", "Cada fuente tiene su propio volumen.", "Chaque source a son propre volume.", "Cada fonte tem seu próprio volume.", "每个音源可单独调整音量。", "Każde źródło ma własną głośność.", "Ogni sorgente ha il proprio volume."]),
    ("no_audio_sources",  ["No players or audio sources yet", "Пока нет игроков и аудиоисточников", "Поки немає гравців і аудіоджерел", "Noch keine Spieler oder Audioquellen", "Aún no hay jugadores ni fuentes de audio", "Aucun joueur ou source audio", "Nenhum jogador ou fonte de áudio ainda", "暂无玩家或音源", "Brak graczy i źródeł dźwięku", "Nessun giocatore o sorgente audio"]),
    ("audio_player",      ["Audio player · {}",    "Аудиоплеер · {}",         "Аудіоплеєр · {}",          "Audioplayer · {}",         "Reproductor · {}",           "Lecteur audio · {}",           "Player de áudio · {}",       "音频播放器 · {}", "Odtwarzacz audio · {}",       "Lettore audio · {}"]),
    ("speakers",          ["Speakers",            "Динамики",                "Динаміки",                "Lautsprecher",            "Altavoces",                 "Haut-parleurs",               "Alto-falantes",             "扬声器",       "Głośniki",                   "Altoparlanti"]),
    ("default_device",    ["Default",             "По умолчанию",            "За замовчуванням",        "Standard",                "Predeterminado",            "Par défaut",                  "Padrão",                    "默认",         "Domyślne",                   "Predefinito"]),
    ("activation_level",  ["Activation level",    "Порог активации",         "Поріг активації",         "Aktivierungspegel",       "Nivel de activación",       "Seuil d'activation",          "Nível de ativação",         "激活阈值",     "Próg aktywacji",             "Soglia attivazione"]),
    ("hotkeys",           ["Hotkeys",             "Горячие клавиши",         "Гарячі клавіші",          "Tastenkürzel",            "Atajos",                    "Raccourcis",                  "Atalhos",                   "快捷键",       "Skróty klawiszowe",          "Scorciatoie"]),
    ("change_keys",       ["Change keys in Options \u{203A} Mods \u{203A} Voice Chat", "Назначить клавиши: Опции \u{203A} Моды \u{203A} Voice Chat", "Призначити клавіші: Опції \u{203A} Моди \u{203A} Voice Chat", "Tasten in Optionen \u{203A} Mods \u{203A} Voice Chat", "Cambia teclas en Opciones \u{203A} Mods \u{203A} Voice Chat", "Changer les touches : Options \u{203A} Mods \u{203A} Voice Chat", "Alterar teclas em Opções \u{203A} Mods \u{203A} Voice Chat", "在 选项 \u{203A} 模组 \u{203A} Voice Chat 中改键", "Zmień klawisze w Opcje \u{203A} Mody \u{203A} Voice Chat", "Cambia tasti in Opzioni \u{203A} Mod \u{203A} Voice Chat"]),
    ("protocol",          ["Protocol",            "Протокол",                "Протокол",                "Protokoll",               "Protocolo",                 "Protocole",                   "Protocolo",                 "协议",         "Protokół",                   "Protocollo"]),
    ("automatic",         ["Automatic",           "Автоматически",           "Автоматично",             "Automatisch",             "Automático",                "Automatique",                 "Automático",                "自动",         "Automatycznie",              "Automatico"]),
    ("status_on_screen",  ["Status on screen",    "Статус на экране",        "Статус на екрані",        "Status im Bild",          "Estado en pantalla",        "État à l'écran",              "Status na tela",            "屏幕状态",     "Status na ekranie",          "Stato a schermo"]),
    ("not_bound",         ["not bound",           "не задана",               "не задано",               "nicht belegt",            "sin asignar",               "non assignée",                "não definida",              "未绑定",       "nieprzypisane",              "non assegnato"]),

    ("in_group",          ["In {}",               "В группе {}",             "У групі {}",              "In {}",                   "En {}",                     "Dans {}",                     "Em {}",                     "在 {}",        "W {}",                       "In {}"]),
    ("leave",             ["Leave",               "Выйти",                   "Вийти",                   "Verlassen",               "Salir",                     "Quitter",                     "Sair",                      "离开",         "Opuść",                      "Esci"]),
    ("groups_off_server", ["Groups are off on this server", "Группы выключены на этом сервере", "Групи вимкнені на цьому сервері", "Gruppen auf diesem Server deaktiviert", "Los grupos están desactivados en este servidor", "Les groupes sont désactivés sur ce serveur", "Grupos desativados neste servidor", "此服务器已禁用群组", "Grupy wyłączone na tym serwerze", "Gruppi disattivati su questo server"]),
    ("not_connected",     ["Voice chat isn't connected", "Голосовой чат не подключён", "Голосовий чат не підключено", "Sprachchat nicht verbunden", "El chat de voz no está conectado", "Le chat vocal n'est pas connecté", "Chat de voz não conectado", "语音聊天未连接", "Czat głosowy nie jest połączony", "Chat vocale non connessa"]),
    ("no_groups",         ["No groups yet — make one below", "Групп пока нет — создайте ниже", "Груп поки немає — створіть нижче", "Noch keine Gruppen — unten erstellen", "Aún no hay grupos — crea uno abajo", "Aucun groupe — créez-en un ci-dessous", "Nenhum grupo ainda — crie um abaixo", "暂无群组 — 在下方创建", "Brak grup — utwórz poniżej", "Nessun gruppo — creane uno sotto"]),
    ("password_tag",      ["(password)",          "(пароль)",                "(пароль)",                "(Passwort)",              "(contraseña)",              "(mot de passe)",              "(senha)",                   "（密码）",     "(hasło)",                    "(password)"]),
    ("nobody",            ["nobody",              "никого",                  "нікого",                  "niemand",                 "nadie",                     "personne",                    "ninguém",                   "无人",         "nikt",                       "nessuno"]),
    ("join",              ["Join",                "Войти",                   "Увійти",                  "Beitreten",               "Unirse",                    "Rejoindre",                   "Entrar",                    "加入",         "Dołącz",                     "Entra"]),
    ("password_for",      ["Password for {}",     "Пароль для {}",           "Пароль для {}",           "Passwort für {}",         "Contraseña de {}",          "Mot de passe pour {}",        "Senha de {}",               "{} 的密码",    "Hasło do {}",                "Password per {}"]),
    ("password",          ["password",            "пароль",                  "пароль",                  "Passwort",                "contraseña",                "mot de passe",                "senha",                     "密码",         "hasło",                      "password"]),
    ("cancel",            ["Cancel",              "Отмена",                  "Скасувати",               "Abbrechen",               "Cancelar",                  "Annuler",                     "Cancelar",                  "取消",         "Anuluj",                     "Annulla"]),
    ("available_groups",  ["Available groups",     "Доступные группы",        "Доступні групи",           "Verfügbare Gruppen",       "Grupos disponibles",          "Groupes disponibles",         "Grupos disponíveis",          "可用群组",     "Dostępne grupy",             "Gruppi disponibili"]),
    ("group_type",        ["Group type",           "Тип группы",               "Тип групи",                "Gruppentyp",               "Tipo de grupo",               "Type de groupe",              "Tipo de grupo",               "群组类型",     "Typ grupy",                  "Tipo di gruppo"]),
    ("new_group",         ["New group",           "Новая группа",            "Нова група",              "Neue Gruppe",             "Nuevo grupo",               "Nouveau groupe",              "Novo grupo",                "新建群组",     "Nowa grupa",                 "Nuovo gruppo"]),
    ("name",              ["name",                "название",                "назва",                   "Name",                    "nombre",                    "nom",                         "nome",                      "名称",         "nazwa",                      "nome"]),
    ("password_optional", ["password (optional)", "пароль (необязательно)",  "пароль (необов'язково)",  "Passwort (optional)",     "contraseña (opcional)",     "mot de passe (optionnel)",    "senha (opcional)",          "密码（可选）", "hasło (opcjonalne)",         "password (opzionale)"]),
    ("group_name_hint",   ["e.g. Builders",        "Например, Строители",      "Наприклад, Будівельники",  "z. B. Baumeister",        "p. ej., Constructores",     "ex. Bâtisseurs",              "ex.: Construtores",         "例如：建造者", "np. Budowniczowie",           "es. Costruttori"]),
    ("group_password_hint", ["No password",        "Без пароля",               "Без пароля",               "Kein Passwort",           "Sin contraseña",            "Sans mot de passe",           "Sem senha",                 "无密码",       "Bez hasła",                  "Nessuna password"]),
    ("type_label",        ["Type: {}",            "Тип: {}",                 "Тип: {}",                 "Typ: {}",                 "Tipo: {}",                  "Type : {}",                   "Tipo: {}",                  "类型：{}",     "Typ: {}",                    "Tipo: {}"]),
    ("create",            ["Create",              "Создать",                 "Створити",                "Erstellen",               "Crear",                     "Créer",                       "Criar",                     "创建",         "Utwórz",                     "Crea"]),
    ("kind_normal",       ["Normal",              "Обычная",                 "Звичайна",                "Normal",                  "Normal",                    "Normal",                      "Normal",                    "普通",         "Normalna",                   "Normale"]),
    ("kind_open",         ["Open",                "Открытая",                "Відкрита",                "Offen",                   "Abierto",                   "Ouvert",                      "Aberto",                    "开放",         "Otwarta",                    "Aperto"]),
    ("kind_isolated",     ["Isolated",            "Изолированная",           "Ізольована",              "Isoliert",                "Aislado",                   "Isolé",                       "Isolado",                   "隔离",         "Izolowana",                  "Isolato"]),
    ("kind_normal_desc",  ["Hear nearby players; they can't hear the group.", "Группа слышит игроков рядом, но они не слышат её.", "Група чує гравців поруч, але вони не чують її.", "Du hörst nahe Spieler; sie hören die Gruppe nicht.", "Oyes a jugadores cercanos; ellos no oyen al grupo.", "Vous entendez les joueurs proches, pas l'inverse.", "Ouça quem está perto; eles não ouvem o grupo.", "可听到附近玩家，但他们听不到群组。", "Słyszysz graczy w pobliżu; oni nie słyszą grupy.", "Senti i giocatori vicini; loro non sentono il gruppo."]),
    ("kind_open_desc",    ["Nearby players and the group hear each other.", "Группа и игроки рядом слышат друг друга.", "Група і гравці поруч чують одне одного.", "Nahe Spieler und die Gruppe hören sich gegenseitig.", "Los jugadores cercanos y el grupo se oyen entre sí.", "Les joueurs proches et le groupe s'entendent.", "Quem está perto e o grupo se ouvem.", "附近玩家与群组成员可以互相听到。", "Gracze w pobliżu i grupa słyszą się nawzajem.", "Giocatori vicini e gruppo si sentono a vicenda."]),
    ("kind_isolated_desc", ["Only group members hear each other.", "Слышны только участники группы.", "Чути лише учасників групи.", "Nur Gruppenmitglieder hören einander.", "Solo los miembros del grupo se oyen.", "Seuls les membres du groupe s'entendent.", "Apenas membros do grupo se ouvem.", "只有群组成员能互相听到。", "Tylko członkowie grupy słyszą się nawzajem.", "Solo i membri del gruppo si sentono."]),

    ("wrong_password",    ["Wrong password",      "Неверный пароль",         "Невірний пароль",         "Falsches Passwort",       "Contraseña incorrecta",     "Mot de passe incorrect",      "Senha incorreta",           "密码错误",     "Błędne hasło",               "Password errata"]),
    ("joined_named",      ["Joined {}",           "Вошли в {}",              "Увійшли в {}",            "{} beigetreten",          "Te uniste a {}",            "Rejoint {}",                  "Entrou em {}",              "已加入 {}",    "Dołączono do {}",            "Entrato in {}"]),
    ("joined_group",      ["Joined the group",    "Вошли в группу",          "Увійшли в групу",         "Gruppe beigetreten",      "Te uniste al grupo",        "Groupe rejoint",              "Entrou no grupo",           "已加入群组",   "Dołączono do grupy",         "Entrato nel gruppo"]),
    ("left_group",        ["Left the group",      "Вышли из группы",         "Вийшли з групи",          "Gruppe verlassen",        "Saliste del grupo",         "Groupe quitté",               "Saiu do grupo",             "已离开群组",   "Opuszczono grupę",           "Uscito dal gruppo"]),
    ("joining",           ["Joining {}...",       "Вход в {}...",            "Вхід у {}...",            "Beitreten zu {}...",      "Uniéndose a {}...",         "Connexion à {}...",           "Entrando em {}...",         "正在加入 {}...", "Dołączanie do {}...",       "Ingresso in {}..."]),
    ("leaving",           ["Leaving the group...", "Выход из группы...",     "Вихід із групи...",       "Gruppe wird verlassen...", "Saliendo del grupo...",     "Départ du groupe...",         "Saindo do grupo...",        "正在离开群组...", "Opuszczanie grupy...",      "Uscita dal gruppo..."]),
    ("creating",          ["Creating {}...",      "Создание {}...",          "Створення {}...",         "{} wird erstellt...",     "Creando {}...",             "Création de {}...",           "Criando {}...",             "正在创建 {}...", "Tworzenie {}...",           "Creazione di {}..."]),
    ("type_name_first",   ["Type a name for the group", "Введите название группы", "Введіть назву групи", "Gib einen Gruppennamen ein", "Escribe un nombre para el grupo", "Entrez un nom de groupe", "Digite um nome para o grupo", "请输入群组名", "Wpisz nazwę grupy", "Inserisci un nome per il gruppo"]),
    ("type_password_first", ["Type the group's password first", "Сначала введите пароль группы", "Спершу введіть пароль групи", "Gib zuerst das Gruppenpasswort ein", "Escribe primero la contraseña", "Entrez d'abord le mot de passe", "Digite a senha do grupo primeiro", "请先输入群组密码", "Najpierw wpisz hasło grupy", "Inserisci prima la password"]),

    ("not_on_server",     ["Not on a server",     "Не на сервере",           "Не на сервері",           "Nicht auf einem Server",  "No estás en un servidor",   "Hors serveur",                "Fora de um servidor",       "不在服务器中", "Poza serwerem",              "Non su un server"]),
    ("no_voice_server",   ["No voice chat on this server", "На сервере нет голосового чата", "На сервері немає голосового чату", "Kein Sprachchat auf diesem Server", "Sin chat de voz en este servidor", "Pas de chat vocal sur ce serveur", "Sem chat de voz neste servidor", "此服务器无语音聊天", "Brak czatu głosowego na serwerze", "Nessuna chat vocale su questo server"]),
    ("asking_server",     ["Asking the server...", "Запрос к серверу...",    "Запит до сервера...",     "Server wird gefragt...",  "Consultando al servidor...", "Interrogation du serveur...", "Consultando o servidor...", "正在询问服务器...", "Pytanie serwera...",        "Richiesta al server..."]),
    ("connected_proto",   ["Connected (protocol {})", "Подключено (протокол {})", "Підключено (протокол {})", "Verbunden (Protokoll {})", "Conectado (protocolo {})", "Connecté (protocole {})", "Conectado (protocolo {})", "已连接（协议 {}）", "Połączono (protokół {})", "Connesso (protocollo {})"]),
    ("connecting",        ["Connecting...",       "Подключение...",          "Підключення...",          "Verbinden...",            "Conectando...",             "Connexion...",                "Conectando...",             "连接中...",    "Łączenie...",                "Connessione..."]),
    ("voice_chat_prefix", ["Voice chat: {}",      "Голосовой чат: {}",       "Голосовий чат: {}",       "Sprachchat: {}",          "Chat de voz: {}",           "Chat vocal : {}",             "Chat de voz: {}",           "语音聊天：{}", "Czat głosowy: {}",           "Chat vocale: {}"]),
    ("voice_chat_off",    ["Voice chat off",      "Голосовой чат выключен",  "Голосовий чат вимкнено",  "Sprachchat aus",          "Chat de voz desactivado",   "Chat vocal désactivé",        "Chat de voz desligado",     "语音聊天已关闭", "Czat głosowy wyłączony",    "Chat vocale disattivata"]),
    ("voice_chat_on",     ["Voice chat on",       "Голосовой чат включён",   "Голосовий чат увімкнено",  "Sprachchat an",          "Chat de voz activado",      "Chat vocal activé",           "Chat de voz ligado",        "语音聊天已开启", "Czat głosowy włączony",     "Chat vocale attivata"]),
    ("microphone_muted",  ["Microphone muted",    "Микрофон выключен",       "Мікрофон вимкнено",       "Mikrofon stumm",          "Micrófono silenciado",      "Micro coupé",                 "Microfone mudo",            "麦克风已静音", "Mikrofon wyciszony",         "Microfono muto"]),
    ("microphone_on",     ["Microphone on",       "Микрофон включён",        "Мікрофон увімкнено",      "Mikrofon an",             "Micrófono activado",        "Micro activé",                "Microfone ligado",          "麦克风已开启", "Mikrofon włączony",          "Microfono attivo"]),
    ("whispering",        ["Whispering",          "Шёпот",                   "Шепіт",                   "Flüstern",                "Susurrando",                "Chuchotement",                "Sussurrando",               "低语中",       "Szept",                      "Sussurro"]),
    ("talking",           ["Talking",             "Говорите",                "Говорите",                "Sprechen",                "Hablando",                  "Parole",                      "Falando",                   "通话中",       "Mówisz",                     "Parli"]),
    ("hud_ptt",           ["Push to talk: {}",    "Рация: {}",               "Рація: {}",               "Push-to-Talk: {}",        "Pulsar para hablar: {}",    "Appuyer pour parler : {}",    "Aperte para falar: {}",     "按键通话：{}", "Naciśnij, by mówić: {}",     "Premi per parlare: {}"]),
    ("hud_voice",         ["Voice activation ({} dB)", "Активация голосом ({} дБ)", "Активація голосом ({} дБ)", "Sprachaktivierung ({} dB)", "Activación por voz ({} dB)", "Activation vocale ({} dB)", "Ativação por voz ({} dB)", "声控（{} dB）", "Aktywacja głosem ({} dB)", "Attivazione vocale ({} dB)"]),
    ("hud_group",         ["Group: {}",           "Группа: {}",              "Група: {}",               "Gruppe: {}",              "Grupo: {}",                 "Groupe : {}",                 "Grupo: {}",                 "群组：{}",     "Grupa: {}",                  "Gruppo: {}"]),
    ("hud_with",          ["  with {}",           "  с {}",                  "  з {}",                  "  mit {}",                "  con {}",                  "  avec {}",                   "  com {}",                  "  与 {}",      "  z {}",                     "  con {}"]),
    ("mic_error",         ["Microphone: {}",      "Микрофон: {}",            "Мікрофон: {}",            "Mikrofon: {}",            "Micrófono: {}",             "Microphone : {}",             "Microfone: {}",             "麦克风：{}",   "Mikrofon: {}",               "Microfono: {}"]),
    ("spk_error",         ["Speakers: {}",        "Динамики: {}",            "Динаміки: {}",            "Lautsprecher: {}",        "Altavoces: {}",             "Haut-parleurs : {}",          "Alto-falantes: {}",         "扬声器：{}",   "Głośniki: {}",               "Altoparlanti: {}"]),

    ("proto_auto",        ["Automatic",           "Автоматически",           "Автоматично",             "Automatisch",             "Automático",                "Automatique",                 "Automático",                "自动",         "Automatycznie",              "Automatico"]),

    ("hk_menu",           ["Open this menu",      "Открыть это меню",        "Відкрити це меню",        "Dieses Menü öffnen",      "Abrir este menú",           "Ouvrir ce menu",              "Abrir este menu",           "打开此菜单",   "Otwórz to menu",             "Apri questo menu"]),
    ("hk_ptt",            ["Push to talk",        "Рация (зажать)",          "Рація (затиснути)",       "Push-to-Talk",            "Pulsar para hablar",        "Appuyer pour parler",         "Aperte para falar",         "按键通话",     "Naciśnij, by mówić",         "Premi per parlare"]),
    ("hk_whisper",        ["Whisper",             "Шёпот",                   "Шепіт",                   "Flüstern",                "Susurro",                   "Chuchoter",                   "Sussurrar",                 "低语",         "Szept",                      "Sussurra"]),
    ("hk_mute",           ["Mute microphone",     "Выключить микрофон",      "Вимкнути мікрофон",       "Mikrofon stummschalten",  "Silenciar micrófono",       "Couper le micro",             "Silenciar microfone",       "麦克风静音",   "Wycisz mikrofon",            "Muta microfono"]),
    ("hk_disable",        ["Voice chat off / on", "Голосовой чат вкл/выкл",  "Голосовий чат увімк/вимк", "Sprachchat aus / an",    "Chat de voz on/off",        "Chat vocal on/off",           "Chat de voz on/off",        "语音聊天开/关", "Czat głosowy wł./wył.",     "Chat vocale on/off"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_and_formats() {
        set_language("en_us");
        assert_eq!(tr("settings"), "Settings");
        assert_eq!(tr("devices"), "Devices");
        assert_eq!(tr("default_device"), "Default");
        assert_eq!(fmt("joining", &["Builders"]), "Joining Builders...");
        set_language("ru_ru");
        assert_eq!(tr("settings"), "Настройки");
        assert_eq!(tr("devices"), "Устройства");
        assert_eq!(tr("default_device"), "По умолчанию");
        assert_eq!(fmt("hud_group", &["Строители"]), "Группа: Строители");
        // An unlisted language falls back to English; a prefix variant finds its base.
        set_language("xx_yy");
        assert_eq!(tr("back"), "Back");
        set_language("de_at");
        assert_eq!(tr("back"), "Zurück");
        // A missing key shows itself rather than panicking.
        set_language("en_us");
        assert_eq!(tr("no_such_key"), "no_such_key");
    }
}
