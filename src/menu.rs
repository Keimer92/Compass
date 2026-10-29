use std::io::{self, stdout};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs,
    },
    Frame, Terminal,
};

use crate::theme::{self, Theme};
use crate::utils::cmd;
use crate::utils::context::{ProjectContext, ProjectType};
use crate::utils::platform;

/// Alto de una card: borde superior + una línea de contenido + borde inferior.
const CARD_H: u16 = 3;
/// Ancho máximo de la columna de cards; se estrecha con la terminal.
const CARD_MAX_W: u16 = 64;
const FOOTER_H: u16 = 1;
const TABS_H: u16 = 1;

static TRACE: AtomicBool = AtomicBool::new(false);

pub fn set_trace(on: bool) {
    TRACE.store(on, Ordering::Relaxed);
}

fn trace_event(msg: &str) {
    if !TRACE.load(Ordering::Relaxed) {
        return;
    }
    let log = std::env::temp_dir().join("compass-tui.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", msg);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    // DEV section
    WindowsDev,
    AndroidDev,
    AndroidHmr,
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildArmv7,
    AndroidBuildAll,
    InstallDebug,
    FlutterProfile,
    Tests,
    // MNT section
    Clean,
    DebugAndroid,
    /// Firmas: keystore, huellas, respaldos, secretos de CI y verificación de
    /// APK. Absorbe las antiguas tarjetas Configure + Keystore + SHA1.
    Signing,
    Version,
    // CFG section
    /// Git: init, .gitignore y conexión con el remoto. Absorbe las antiguas
    /// tarjetas Git Init y .gitignore.
    Git,
    BuildWhatsApp,
    ConfigureTerminal,
}

/// Pestañas del menú: DEV∪MNT en la primera, sólo CFG en la segunda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    DevMnt,
    Cfg,
}

impl Tab {
    fn index(self) -> usize {
        self as usize
    }

    fn next(self) -> Self {
        match self {
            Self::DevMnt => Self::Cfg,
            Self::Cfg => Self::DevMnt,
        }
    }

    fn prev(self) -> Self {
        self.next()
    }
}

/// Rango de `items` que pertenece a una pestaña (`end` exclusivo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TabRange {
    start: usize,
    end: usize,
}

impl TabRange {
    fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

impl MenuItem {
    fn icon(&self) -> &str {
        match self {
            Self::WindowsDev => "\u{1f5a5}",
            Self::AndroidDev => "\u{1f4f1}",
            Self::AndroidHmr => "\u{1f525}",
            Self::WindowsBuild => "\u{1f4e6}",
            Self::AndroidBuildArm64 => "\u{1f680}",
            Self::AndroidBuildArmv7 => "\u{1f680}",
            Self::AndroidBuildAll => "\u{1f680}",
            Self::InstallDebug => "\u{1f4e6}",
            Self::FlutterProfile => "\u{1f3af}",
            Self::Tests => "\u{1f9ea}",
            Self::Clean => "\u{1f9f9}",
            Self::DebugAndroid => "\u{1f50d}",
            Self::Signing => "\u{1f511}",
            Self::Version => "\u{1f516}",
            Self::Git => "\u{1f527}",
            Self::BuildWhatsApp => "\u{1f4ac}",
            Self::ConfigureTerminal => "\u{2699}",
        }
    }

    fn section(&self) -> &str {
        match self {
            Self::WindowsDev
            | Self::AndroidDev
            | Self::AndroidHmr
            | Self::WindowsBuild
            | Self::AndroidBuildArm64
            | Self::AndroidBuildArmv7
            | Self::AndroidBuildAll
            | Self::InstallDebug
            | Self::FlutterProfile
            | Self::Tests => "DEV",
            Self::Clean | Self::DebugAndroid | Self::Signing | Self::Version => "MNT",
            Self::Git | Self::BuildWhatsApp | Self::ConfigureTerminal => "CFG",
        }
    }

    fn tab(&self) -> Tab {
        match self.section() {
            "CFG" => Tab::Cfg,
            _ => Tab::DevMnt,
        }
    }

    /// Clave del acento en `theme::CARD_KEYS` (uno por variante, sin repetir).
    fn color_key(&self) -> &'static str {
        match self {
            Self::WindowsDev => "run",
            Self::AndroidDev => "android",
            Self::AndroidHmr => "reload",
            Self::WindowsBuild => "build",
            Self::AndroidBuildArm64 => "apk",
            Self::AndroidBuildArmv7 => "apk32",
            Self::AndroidBuildAll => "aab",
            Self::InstallDebug => "install",
            Self::FlutterProfile => "profile",
            Self::Tests => "test",
            Self::Clean => "clean",
            Self::DebugAndroid => "debug",
            Self::Signing => "sign",
            Self::Version => "version",
            Self::Git => "git",
            Self::BuildWhatsApp => "whatsapp",
            Self::ConfigureTerminal => "terminal",
        }
    }

    fn label(&self, project_type: ProjectType) -> &str {
        match self {
            Self::WindowsDev => match project_type {
                ProjectType::Rust => "Run",
                ProjectType::Python => "Run",
                ProjectType::Go => "Run",
                _ => "Windows",
            },
            Self::AndroidDev => match project_type {
                ProjectType::GoWails => "And. Dev",
                _ => "Android",
            },
            Self::AndroidHmr => {
                if project_type == ProjectType::Python {
                    "Reload"
                } else {
                    "HMR"
                }
            }
            Self::WindowsBuild => match project_type {
                ProjectType::Rust => "Build",
                ProjectType::Python => "Package",
                ProjectType::Go => "Build",
                // En Go + Wails el release ya no es un `.exe` portable: la
                // tarea empaqueta el instalador (NSIS en Windows).
                ProjectType::GoWails => "Installer",
                _ => "Win Build",
            },
            Self::AndroidBuildArm64 => match project_type {
                ProjectType::Tauri => "Android a64",
                ProjectType::Flutter => "APK",
                ProjectType::Kotlin => "APK",
                ProjectType::Rust => "",
                ProjectType::Go => "",
                ProjectType::GoWails => "APK",
                ProjectType::Python => "",
            },
            Self::AndroidBuildArmv7 => match project_type {
                ProjectType::Tauri => "Android a7",
                ProjectType::Flutter => "",
                ProjectType::Kotlin => "",
                ProjectType::Rust => "",
                ProjectType::Go => "",
                ProjectType::GoWails => "",
                ProjectType::Python => "",
            },
            Self::AndroidBuildAll => match project_type {
                ProjectType::Tauri => "Android All",
                ProjectType::Flutter => "AAB",
                ProjectType::Kotlin => "AAB",
                ProjectType::Rust => "",
                ProjectType::Go => "",
                ProjectType::GoWails => "AAB",
                ProjectType::Python => "",
            },
            Self::InstallDebug => "Install",
            Self::FlutterProfile => "Profile",
            Self::Tests => {
                if matches!(project_type, ProjectType::Python | ProjectType::Go) {
                    "Test"
                } else {
                    ""
                }
            }
            Self::Clean => "Clean",
            Self::DebugAndroid => "Debug Andr",
            Self::Signing => "Signing",
            Self::Version => "Version",
            Self::Git => "Git",
            Self::BuildWhatsApp => "WhatsApp",
            Self::ConfigureTerminal => "Terminal",
        }
    }

    fn description(&self, project_type: ProjectType) -> &str {
        match project_type {
            ProjectType::Rust => match self {
                Self::WindowsDev => "cargo run",
                Self::AndroidDev => "",
                Self::WindowsBuild => "cargo build --release",
                Self::AndroidBuildArm64 => "",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "",
                Self::InstallDebug => "",
                Self::Clean => "Limpia target/ y cache",
                Self::DebugAndroid => "",
                Self::Signing => "",
                Self::Version => "Actualiza Cargo.toml",
                Self::Git => "Init · gitignore · remoto",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "",
                Self::Tests => "",
                Self::AndroidHmr => "",
            },
            ProjectType::Tauri => match self {
                Self::WindowsDev => "Inicia el dev server",
                Self::AndroidDev => "Dev con target Android",
                Self::WindowsBuild => "Compila produccion",
                Self::AndroidBuildArm64 => "Compila APK arm64",
                Self::AndroidBuildArmv7 => "Compila APK armv7",
                Self::AndroidBuildAll => "Compila todas las APK",
                Self::InstallDebug => "",
                Self::Clean => "Limpia builds/deps",
                Self::DebugAndroid => "Dev + Chrome DevTools",
                Self::Signing => "Firmas y huellas",
                Self::Git => "Init · gitignore · remoto",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::Version => "Bump pkg/cargo/tauri",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "",
                Self::AndroidHmr => "",
                Self::Tests => "",
            },
            ProjectType::Flutter => match self {
                Self::WindowsDev => "flutter run windows",
                Self::AndroidDev => "flutter run android",
                Self::WindowsBuild => "flutter build windows",
                Self::AndroidBuildArm64 => "flutter build apk",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "flutter build appbundle",
                Self::InstallDebug => "",
                Self::Clean => "Limpia builds/deps",
                Self::DebugAndroid => "Depurar Android",
                Self::Signing => "Firmas y huellas",
                Self::Git => "Init · gitignore · remoto",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::Version => "",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "flutter run --profile",
                Self::AndroidHmr => "",
                Self::Tests => "",
            },
            ProjectType::Kotlin => match self {
                Self::WindowsDev => "gradlew :composeApp:run",
                Self::AndroidDev => "gradlew :installDebug",
                Self::WindowsBuild => "gradlew :packageDist",
                Self::AndroidBuildArm64 => "assembleRelease",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "gradlew :bundleRelease",
                Self::InstallDebug => "gradlew :installDebug",
                Self::Clean => "Limpia builds/deps",
                Self::DebugAndroid => "Depurar Android",
                Self::Signing => "Firmas y huellas",
                Self::Git => "Init · gitignore · remoto",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::Version => "",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "",
                Self::AndroidHmr => "",
                Self::Tests => "",
            },
            ProjectType::GoWails => match self {
                Self::WindowsDev => "dev escritorio",
                Self::AndroidDev => "APK dev",
                Self::AndroidHmr => "HMR (Vite)",
                Self::WindowsBuild => "instalador (.exe)",
                Self::AndroidBuildArm64 => "APK release",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "AAB release",
                Self::InstallDebug => "APK dev",
                Self::Clean => "Limpia builds/deps",
                Self::DebugAndroid => "logcat de la app",
                Self::Signing => "Firmas y huellas",
                Self::Git => "Init · gitignore",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::Version => "build/config.yml",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "",
                Self::Tests => "",
            },
            ProjectType::Python => match self {
                Self::WindowsDev => "python <script/module>",
                Self::AndroidDev => "",
                Self::AndroidHmr => "Uvicorn ASGI --reload",
                Self::WindowsBuild => "python -m build",
                Self::AndroidBuildArm64 => "",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "",
                Self::InstallDebug => "",
                Self::Clean => "Limpia caches y builds Python",
                Self::DebugAndroid => "",
                Self::Signing => "",
                Self::Version => "Actualiza pyproject.toml",
                Self::Git => "Init · gitignore · remoto",
                Self::BuildWhatsApp => "",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "",
                Self::Tests => "python -m pytest",
            },
            ProjectType::Go => match self {
                Self::WindowsDev => "go run .",
                Self::AndroidHmr => "air (rebuild + restart)",
                Self::AndroidDev => "",
                Self::WindowsBuild => "go build",
                Self::AndroidBuildArm64 => "",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "",
                Self::InstallDebug => "",
                Self::Clean => "Limpia binarios",
                Self::DebugAndroid => "",
                Self::Signing => "",
                Self::Version => "",
                Self::Git => "Init · gitignore · remoto",
                Self::BuildWhatsApp => "",
                Self::ConfigureTerminal => "Autocompletado",
                Self::FlutterProfile => "",
                Self::Tests => "go test ./...",
            },
        }
    }
}

/// Regiones de la pantalla. Lo calcula [`layout`], pura, para que el dibujado y
/// el hit-testing del ratón no se desincronicen al redimensionar.
#[derive(Debug, Clone, Copy)]
struct UiLayout {
    header: Rect,
    tabs: Rect,
    /// Región visible de la columna de cards (recorta el scroll).
    list: Rect,
    /// Columna de una celda a la derecha de la lista para la barra de scroll.
    /// `width == 0` si la terminal no deja hueco.
    scrollbar: Rect,
    footer: Rect,
}

fn layout(area: Rect, warn: bool) -> UiLayout {
    let header_h: u16 = if warn { 3 } else { 2 };
    let [header, tabs, list, footer] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(TABS_H),
        Constraint::Min(0),
        Constraint::Length(FOOTER_H),
    ])
    .areas(area);

    let list_w = CARD_MAX_W.min(list.width);
    let list_x = list.x + (list.width.saturating_sub(list_w)) / 2;
    let list_rect = Rect {
        x: list_x,
        width: list_w,
        ..list
    };
    // La barra va una columna a la derecha de las cards para no pisar su
    // borde; si no cabe, se omite (dibujado y `card_at` la ignoran).
    let scrollbar = if list.width >= list_w + 2 {
        Rect {
            x: list_x + list_w + 1,
            y: list.y,
            width: 1,
            height: list.height,
        }
    } else {
        Rect {
            x: list.right().saturating_sub(1),
            y: list.y,
            width: 0,
            height: list.height,
        }
    };

    UiLayout {
        header,
        tabs,
        list: list_rect,
        scrollbar,
        footer,
    }
}

struct MenuState {
    /// Ordenado por pestaña: DEV∪MNT primero, luego CFG. Contiguo y
    /// pestaña-major, así que `selected` (índice global) sigue siendo válido y
    /// "saltar de pestaña al llegar al borde" es caminar linealmente.
    items: Vec<MenuItem>,
    tabs: [TabRange; 2],
    tab: Tab,
    selected: usize,
    /// Selección recordada por pestaña, para volver donde se estaba.
    remember: [usize; 2],
    hover: Option<usize>,
    /// Desplazamiento vertical en líneas, siempre múltiplo de [`CARD_H`].
    scroll: u16,
    /// Alto del viewport. La fija `draw_menu` en cada frame.
    vh: u16,
    /// Última geometría dibujada; `None` si la terminal no admite ni una card.
    ui: Option<UiLayout>,
    /// Tema activo. `&'static` porque los temas viven en `theme::THEMES`.
    theme: &'static Theme,
    /// Selector de tema abierto (`Ctrl+T`). `None` = menú normal.
    picker: Option<ThemePicker>,
}

/// Estado del modal de temas.
///
/// `index` apunta a `theme::THEMES`; `original` es el tema que estaba activo al
/// abrirse, para poder **revertir** con `Esc`. Mientras el modal está abierto el
/// menú de detrás se repinta con el tema resaltado: esa es la vista previa.
#[derive(Debug, Clone, Copy)]
struct ThemePicker {
    index: usize,
    original: &'static Theme,
}

impl MenuState {
    fn new(project_type: ProjectType, theme: &'static Theme) -> Self {
        let base = vec![
            MenuItem::WindowsDev,
            MenuItem::AndroidDev,
            MenuItem::AndroidHmr,
            MenuItem::WindowsBuild,
            MenuItem::AndroidBuildArm64,
            MenuItem::AndroidBuildArmv7,
            MenuItem::AndroidBuildAll,
            MenuItem::InstallDebug,
            MenuItem::FlutterProfile,
            MenuItem::Tests,
            MenuItem::Clean,
            MenuItem::DebugAndroid,
            MenuItem::Signing,
            MenuItem::Version,
            MenuItem::Git,
            MenuItem::BuildWhatsApp,
            MenuItem::ConfigureTerminal,
        ];

        let mut items: Vec<MenuItem> = base
            .into_iter()
            .filter(|item| match project_type {
                ProjectType::Rust => !matches!(
                    item,
                    MenuItem::AndroidDev
                        | MenuItem::AndroidBuildArm64
                        | MenuItem::AndroidBuildArmv7
                        | MenuItem::AndroidBuildAll
                        | MenuItem::InstallDebug
                        | MenuItem::DebugAndroid
                        | MenuItem::Signing
                        | MenuItem::FlutterProfile
                        | MenuItem::AndroidHmr
                        | MenuItem::Tests
                ),
                ProjectType::Tauri => !matches!(
                    item,
                    MenuItem::InstallDebug
                        | MenuItem::FlutterProfile
                        | MenuItem::AndroidHmr
                        | MenuItem::Tests
                ),
                ProjectType::Flutter => !matches!(
                    item,
                    MenuItem::AndroidBuildArmv7
                        | MenuItem::Version
                        | MenuItem::InstallDebug
                        | MenuItem::AndroidHmr
                        | MenuItem::Tests
                ),
                ProjectType::Kotlin => !matches!(
                    item,
                    MenuItem::AndroidBuildArmv7
                        | MenuItem::Version
                        | MenuItem::FlutterProfile
                        | MenuItem::AndroidHmr
                        | MenuItem::Tests
                ),
                // Go + Wails: la firma la hace el Gradle de Wails con
                // variables de entorno (ANDROID_KEYSTORE_*), que Compass
                // exporta al compilar; la tarjeta gestiona el keystore y el
                // `.properties` de donde salen.
                ProjectType::GoWails => !matches!(
                    item,
                    MenuItem::AndroidBuildArmv7 | MenuItem::FlutterProfile | MenuItem::Tests
                ),
                ProjectType::Python => !matches!(
                    item,
                    MenuItem::AndroidDev
                        | MenuItem::AndroidBuildArm64
                        | MenuItem::AndroidBuildArmv7
                        | MenuItem::AndroidBuildAll
                        | MenuItem::InstallDebug
                        | MenuItem::FlutterProfile
                        | MenuItem::DebugAndroid
                        | MenuItem::Signing
                        | MenuItem::BuildWhatsApp
                ),
                ProjectType::Go => !matches!(
                    item,
                    MenuItem::AndroidDev
                        | MenuItem::AndroidBuildArm64
                        | MenuItem::AndroidBuildArmv7
                        | MenuItem::AndroidBuildAll
                        | MenuItem::InstallDebug
                        | MenuItem::FlutterProfile
                        | MenuItem::DebugAndroid
                        | MenuItem::Signing
                        | MenuItem::Version
                        | MenuItem::BuildWhatsApp
                ),
            })
            .collect();

        // Sort estable: conserva el orden DEV → MNT dentro de la pestaña 1.
        items.sort_by_key(|item| item.tab() as u8);

        let mut tabs = [TabRange::default(); 2];
        for (i, item) in items.iter().enumerate() {
            let t = item.tab().index();
            if tabs[t].end == tabs[t].start {
                tabs[t].start = i;
            }
            tabs[t].end = i + 1;
        }

        Self {
            items,
            tabs,
            tab: Tab::DevMnt,
            selected: tabs[0].start,
            remember: [tabs[0].start, tabs[1].start],
            hover: None,
            scroll: 0,
            vh: 0,
            ui: None,
            theme,
            picker: None,
        }
    }

    fn tab_range(&self) -> TabRange {
        self.tabs[self.tab.index()]
    }

    fn tab_len(&self) -> usize {
        self.tab_range().len()
    }

    fn local_selected(&self) -> usize {
        self.selected.saturating_sub(self.tab_range().start)
    }

    /// Abre el modal de temas situándolo en el tema activo.
    fn open_picker(&mut self) {
        self.picker = Some(ThemePicker {
            index: theme::index_of(self.theme.name),
            original: self.theme,
        });
    }

    /// Mueve el resaltado del modal a `t` y aplica la vista previa.
    fn preview_theme(&mut self, t: &'static Theme) {
        if let Some(p) = self.picker.as_mut() {
            p.index = theme::index_of(t.name);
        }
        self.theme = t;
    }

    /// Ajusta `scroll` para que la card seleccionada quede entera dentro del
    /// viewport. El desplazamiento se mantiene múltiplo de [`CARD_H`] para que
    /// ninguna card se dibuje a medias.
    fn ensure_visible(&mut self) {
        let n = self.tab_len();
        if n == 0 || self.vh < CARD_H {
            self.scroll = 0;
            return;
        }
        let visible = (self.vh / CARD_H) as usize;
        let i = self.local_selected().min(n - 1);
        let top = i * CARD_H as usize;
        let max = n.saturating_sub(visible) * CARD_H as usize;

        let mut s = self.scroll as usize;
        if top < s {
            s = top;
        } else if top + CARD_H as usize > s + visible * CARD_H as usize {
            s = i.saturating_add(1).saturating_sub(visible) * CARD_H as usize;
        }
        self.scroll = s.min(max) as u16;
    }

    /// Cambia de pestaña recordando la selección de la que se deja.
    fn set_tab(&mut self, t: Tab) {
        if self.tab == t {
            return;
        }
        self.remember[self.tab.index()] = self.selected;
        self.tab = t;
        let r = self.tabs[t.index()];
        if r.len() == 0 {
            return;
        }
        self.selected = self.remember[t.index()].clamp(r.start, r.end - 1);
        self.remember[t.index()] = self.selected;
        self.ensure_visible();
    }

    /// Selecciona `idx` (global); cambia de pestaña si la card pertenece a otra.
    fn jump_to(&mut self, idx: usize) {
        if self.items.is_empty() {
            return;
        }
        let idx = idx.min(self.items.len() - 1);
        let target = self.items[idx].tab();
        if target != self.tab {
            self.remember[self.tab.index()] = self.selected;
            self.tab = target;
        }
        self.selected = idx;
        self.ensure_visible();
    }

    /// Mueve la selección `delta` posiciones sobre la lista global: al llegar
    /// al borde de una pestaña salta a la adyacente.
    fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len() as isize - 1;
        let next = (self.selected as isize + delta).clamp(0, max) as usize;
        self.jump_to(next);
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn show_android_warning(ctx: &ProjectContext) -> bool {
    ctx.android_home.is_none() && ctx.project_type != ProjectType::Python
}

fn draw_menu(frame: &mut Frame, state: &mut MenuState, ctx: &ProjectContext) {
    let area = frame.area();
    let theme = state.theme;

    // Fondo de toda la pantalla, a lo OpenCode: cada tema lleva su propio
    // `background`, y el resto de widgets no lo pisan porque sus `Style`
    // no definen `bg` (se pintan "sobre" este bloque).
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );

    let warn = show_android_warning(ctx);
    let ui = layout(area, warn);
    state.vh = ui.list.height;

    if ui.list.height < CARD_H {
        // Ni una card cabe: única situación en la que no hay nada que dibujar.
        let min_h = ui.header.height + TABS_H + FOOTER_H + CARD_H;
        let msg = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                " Terminal demasiado pequena.",
                Style::default().fg(theme.warning),
            )),
            Line::from(Span::styled(
                format!(" Necesita al menos {} lineas.", min_h),
                Style::default().fg(theme.muted),
            )),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        state.ui = None;
        return;
    }

    render_header(frame, ui.header, ctx, theme);
    render_tabs(frame, ui.tabs, state, theme);

    state.ensure_visible();
    render_cards(frame, ui, state, ctx.project_type, theme);
    render_scrollbar(frame, ui, state, theme);
    render_footer(frame, ui.footer, theme, ctx);

    // El modal va al final para quedar por encima de todo lo demás.
    if let Some(picker) = state.picker {
        render_theme_picker(frame, area, theme, picker);
    }

    state.ui = Some(ui);
}

fn render_tabs(frame: &mut Frame, area: Rect, state: &MenuState, theme: &Theme) {
    let dev_count = state.tabs[Tab::DevMnt.index()].len();
    let cfg_count = state.tabs[Tab::Cfg.index()].len();
    let titles = vec![
        Line::from(format!(" DESARROLLO · MNT {dev_count} ")),
        Line::from(format!(" CONFIGURACION {cfg_count} ")),
    ];
    // El acento del tab activo es el de *su* sección (DEV para la 1, CFG para
    // la 2), para conservar la identidad cromática de cada tema.
    let active = match state.tab {
        Tab::DevMnt => theme.dev,
        Tab::Cfg => theme.cfg,
    };
    let tabs = Tabs::new(titles)
        .select(state.tab.index())
        .divider(Span::styled(" │ ", Style::default().fg(theme.muted)))
        .style(Style::default().fg(theme.muted))
        .highlight_style(Style::default().fg(active).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn render_cards(
    frame: &mut Frame,
    ui: UiLayout,
    state: &MenuState,
    project_type: ProjectType,
    theme: &Theme,
) {
    let n = state.tab_len();
    let start = state.tab_range().start;
    let list_y = ui.list.y as i32;
    let list_bottom = ui.list.bottom() as i32;

    for local in 0..n {
        let y = list_y + (local * CARD_H as usize) as i32 - state.scroll as i32;
        // Cards enteras o nada: con `scroll` múltiplo de `CARD_H` sólo la
        // última del viewport puede sobrar, y se omite en vez de dibujarla
        // recortada.
        if y < list_y || y + CARD_H as i32 > list_bottom {
            continue;
        }
        let idx = start + local;
        let rect = Rect {
            x: ui.list.x,
            y: y as u16,
            width: ui.list.width,
            height: CARD_H,
        };
        let card = create_card(
            &state.items[idx],
            state.selected == idx,
            state.hover == Some(idx),
            project_type,
            theme,
        );
        frame.render_widget(card, rect);
    }
}

fn render_scrollbar(frame: &mut Frame, ui: UiLayout, state: &MenuState, theme: &Theme) {
    let content = state.tab_len().saturating_mul(CARD_H as usize);
    if ui.scrollbar.width == 0 || content as u16 <= ui.list.height {
        return;
    }
    let mut sb_state = ScrollbarState::new(content)
        .position(state.scroll as usize)
        .viewport_content_length(ui.list.height as usize);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_style(Style::default().fg(theme.brand))
        .track_style(Style::default().fg(theme.border));
    frame.render_stateful_widget(scrollbar, ui.scrollbar, &mut sb_state);
}

/// Dibuja el modal de selección de tema.
///
/// Se centra en la pantalla y se limpia su rect con [`Clear`] porque es una capa
/// opaca sobre el menú; el menú de detrás ya se ha repintado con el tema en
/// vista previa (lo aplica [`MenuState::preview_theme`]).
fn render_theme_picker(frame: &mut Frame, area: Rect, theme: &Theme, picker: ThemePicker) {
    let count = theme::THEMES.len();
    // Ancho fijo holgado (nombre de 12 + marca + muestras). Alto = filas + bordes.
    let w = 44u16.min(area.width);
    let h = (count as u16 + 2).min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, rect);

    let mut lines: Vec<Line> = Vec::with_capacity(count);
    for (i, t) in theme::THEMES.iter().enumerate() {
        let selected = i == picker.index;
        let marker = if selected { "\u{25b8} " } else { "  " };
        let mut line = Line::from(vec![
            Span::styled(
                format!(" {marker}"),
                Style::default().fg(if selected { theme.brand } else { theme.muted }),
            ),
            Span::styled(
                format!("{:<12}", t.name),
                Style::default().fg(if selected { theme.text } else { theme.muted }),
            ),
            Span::raw("  "),
            // Muestras de la propia paleta de cada tema (DEV/MNT/CFG), para
            // reconocerlos de un vistazo sin aplicarlos.
            Span::styled("\u{2588}\u{2588}", Style::default().fg(t.dev)),
            Span::styled("\u{2588}\u{2588}", Style::default().fg(t.mnt)),
            Span::styled("\u{2588}\u{2588}", Style::default().fg(t.cfg)),
        ]);
        if selected {
            line = line.style(
                Style::default()
                    .bg(theme.hover_bg)
                    .add_modifier(Modifier::BOLD),
            );
        }
        lines.push(line);
    }

    // `Clear` borra el menú de detrás, pero deja las celdas con estilo por
    // defecto (el fondo nativo de la terminal): el Block del Paragraph pinta el
    // `background` del tema para que el modal no destaque como un rectángulo
    // de otro color.
    let block = Block::default()
        .style(Style::default().bg(theme.background))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.brand))
        .title(Span::styled(
            " Temas ",
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(vec![
            Span::styled(" \u{2191}\u{2193} ", Style::default().fg(theme.text)),
            Span::styled("elegir  ", Style::default().fg(theme.muted)),
            Span::styled("Enter ", Style::default().fg(theme.text)),
            Span::styled("aplicar  ", Style::default().fg(theme.muted)),
            Span::styled("Esc ", Style::default().fg(theme.text)),
            Span::styled("cancelar ", Style::default().fg(theme.muted)),
        ]));

    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);
}

fn render_header(frame: &mut Frame, area: Rect, ctx: &ProjectContext, theme: &Theme) {
    let project_name = ctx
        .script_dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed("Proyecto"));

    let (emoji, label, badge) = match ctx.project_type {
        ProjectType::Tauri => ("\u{1f980}", "Compass for Rust / Tauri", "TAURI"),
        ProjectType::Flutter => ("\u{1f3af}", "Compass for Dart / Flutter", "FLUTTER"),
        ProjectType::Kotlin => ("\u{1f537}", "Compass for Kotlin / KMP", "KOTLIN"),
        ProjectType::Rust => ("\u{2699}", "Compass for Rust", "RUST"),
        ProjectType::Go => ("\u{1f4e6}", "Compass for Go", "GO"),
        ProjectType::GoWails => ("\u{1f408}", "Compass for Go / Wails", "GO"),
        ProjectType::Python => ("\u{1f40d}", "Compass for Python", "PYTHON"),
    };
    let badge_color = match ctx.project_type {
        ProjectType::Tauri => theme.success,
        ProjectType::Flutter => theme.info,
        ProjectType::Kotlin => theme.accent,
        ProjectType::Rust => theme.warning,
        ProjectType::Go => theme.info,
        ProjectType::GoWails => theme.info,
        ProjectType::Python => theme.accent,
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  {}  {}  v", emoji, label),
                Style::default()
                    .fg(theme.brand)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("0.1.0", Style::default().fg(theme.muted)),
        ]),
        Line::from(vec![
            Span::styled("     Proyecto: ", Style::default().fg(theme.muted)),
            Span::styled(project_name.to_string(), Style::default().fg(theme.text)),
            Span::styled("  ", Style::default()),
            Span::styled(
                format!(" {} ", badge),
                Style::default()
                    .bg(badge_color)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if show_android_warning(ctx) {
        lines.push(Line::from(vec![Span::styled(
            "    \u{26a0} ANDROID_HOME no configurada",
            Style::default().fg(theme.warning),
        )]));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_footer(frame: &mut Frame, area: Rect, theme: &Theme, ctx: &ProjectContext) {
    // Izquierda: atajos y tema. Derecha: la **ruta completa** del proyecto (la
    // barra de título sólo lleva el nombre del directorio, así que el path
    // entero va aquí). Si no cabe, se recorta por la izquierda con "…".
    let left = Line::from(vec![
        Span::styled("  Tab", Style::default().fg(theme.text)),
        Span::styled(" pestañas   ", Style::default().fg(theme.muted)),
        Span::styled("\u{2191}\u{2193}", Style::default().fg(theme.text)),
        Span::styled(" navegar   ", Style::default().fg(theme.muted)),
        Span::styled("Enter", Style::default().fg(theme.text)),
        Span::styled(" ejecutar   ", Style::default().fg(theme.muted)),
        Span::styled("Esc", Style::default().fg(theme.text)),
        Span::styled(" salir   ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("Ctrl+T temas: {}  ", theme.name),
            Style::default().fg(theme.brand),
        ),
    ]);
    let left_w = (left.width() as u16).min(area.width);

    let full = ctx.script_dir.display().to_string();
    let right_w = area.width - left_w;
    let chars: Vec<char> = full.chars().collect();
    let shown = if chars.len() as u16 > right_w && right_w >= 2 {
        let keep = right_w as usize - 1;
        format!(
            "…{}",
            chars[chars.len() - keep..].iter().collect::<String>()
        )
    } else {
        full
    };
    let shown_w = (shown.chars().count() as u16).min(area.width);

    frame.render_widget(
        Paragraph::new(left),
        Rect {
            width: left_w,
            ..area
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            shown,
            Style::default().fg(theme.muted),
        )))
        .alignment(Alignment::Right),
        Rect {
            x: area.x + area.width - shown_w,
            width: shown_w,
            ..area
        },
    );
}

/// Card compacta de tres líneas: borde + una línea de contenido + borde.
///
/// Cada card usa **su** acento (`theme.card_accent(item.color_key())`), así que
/// dentro de una pestaña no hay dos iguales. Los tres estados:
///
/// | Estado | Borde | Fondo |
/// |---|---|---|
/// | reposo | acento propio, sin BOLD | ninguno |
/// | hover | `text` + BOLD | `hover_bg` neutro |
/// | seleccionado | acento propio + BOLD | `card_bg` (tinte del acento) |
///
/// Que hover y selección nunca compartan fondo lo cubren
/// `theme::tests::hover_is_distinct_from_every_selection` y
/// `tests::hover_is_visually_distinct_from_selected`.
fn create_card(
    item: &MenuItem,
    is_selected: bool,
    is_hovered: bool,
    project_type: ProjectType,
    theme: &Theme,
) -> Paragraph<'static> {
    let key = item.color_key();
    let accent = theme.card_accent(key);

    // Selección gana a hover: es la que Enter ejecutaría.
    let border = if is_selected {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else if is_hovered {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(accent)
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border);
    if is_selected {
        block = block.style(Style::default().bg(theme.card_bg(key)));
    } else if is_hovered {
        block = block.style(Style::default().bg(theme.hover_bg));
    }

    let icon = item.icon();
    let label = item.label(project_type);
    let desc = item.description(project_type);

    let mut label_style = Style::default().fg(accent);
    if is_selected || is_hovered {
        label_style = label_style.add_modifier(Modifier::BOLD);
    }
    let desc_style = if is_hovered && !is_selected {
        Style::default().fg(theme.text)
    } else {
        Style::default().fg(theme.muted)
    };

    let mut spans = vec![Span::styled(format!(" {icon}  {label}"), label_style)];
    if !desc.is_empty() {
        // Prefijo fijo + relleno: el centrado con emojis de ancho ambiguo
        // descuadra el texto, y el truncado lo hace el propio `Rect`.
        let inner_w = CARD_MAX_W.saturating_sub(2) as usize;
        let used = spans[0].content.chars().count() + desc.chars().count() + 1;
        let pad = inner_w.saturating_sub(used).max(1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(desc.to_string(), desc_style));
        spans.push(Span::raw(" "));
    }

    Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left)
        .block(block)
}

/// Índice global (en `state.items`) de la card bajo las coordenadas del mouse,
/// o `None` si está fuera de la columna visible.
fn card_at(col: u16, row: u16, state: &MenuState) -> Option<usize> {
    let ui = state.ui?;
    if !ui.list.contains(Position::new(col, row)) {
        return None;
    }
    let offset = (row - ui.list.y) as usize + state.scroll as usize;
    let local = offset / CARD_H as usize;
    if local >= state.tab_len() {
        return None;
    }
    Some(state.tab_range().start + local)
}

enum MenuAction {
    Run(MenuItem),
    Quit,
    Continue,
}

fn handle_event(state: &mut MenuState) -> anyhow::Result<MenuAction> {
    let evt = crossterm::event::read()?;
    trace_event(&format!("EVENT: {:?}", evt));

    // Con el modal de temas abierto, el teclado le pertenece por completo: el
    // menú de detrás no debe reaccionar (nada de Enter ejecutando una tarea).
    if state.picker.is_some() {
        if let Event::Key(key) = evt {
            if key.kind == KeyEventKind::Press {
                let Some(picker) = state.picker else {
                    return Ok(MenuAction::Continue);
                };
                match key.code {
                    KeyCode::Esc => {
                        // Cancelar: se vuelve al tema que estaba activo al abrir.
                        state.theme = picker.original;
                        state.picker = None;
                    }
                    KeyCode::Enter => {
                        // Aplicar: persiste el resaltado. Best-effort.
                        theme::save(&theme::THEMES[picker.index]);
                        state.picker = None;
                    }
                    KeyCode::Up | KeyCode::Left => {
                        state.preview_theme(theme::prev(state.theme.name));
                    }
                    KeyCode::Down | KeyCode::Right => {
                        state.preview_theme(theme::next(state.theme.name));
                    }
                    // Ctrl+T también cierra, sin aplicar (como Esc).
                    KeyCode::Char(c)
                        if c.eq_ignore_ascii_case(&'t')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        state.theme = picker.original;
                        state.picker = None;
                    }
                    _ => {}
                }
            }
        }
        return Ok(MenuAction::Continue);
    }

    match evt {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            state.hover = None;

            // Ctrl+T abre el modal de temas (no cicla directamente). Se resuelve
            // antes del `match key.code` porque, si no, la `t` caería en el brazo
            // `_` y no haría nada.
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if let KeyCode::Char(c) = key.code {
                    if c.eq_ignore_ascii_case(&'t') {
                        state.open_picker();
                        return Ok(MenuAction::Continue);
                    }
                }
            }

            match key.code {
                KeyCode::Esc => return Ok(MenuAction::Quit),
                KeyCode::Enter => return Ok(MenuAction::Run(state.items[state.selected])),
                KeyCode::Up => state.move_selection(-1),
                KeyCode::Down => state.move_selection(1),
                KeyCode::Tab | KeyCode::Right => state.set_tab(state.tab.next()),
                KeyCode::BackTab | KeyCode::Left => state.set_tab(state.tab.prev()),
                _ => {}
            }
        }
        Event::Mouse(mouse) => {
            trace_event(&format!(
                "MOUSE: {:?} col={} row={}",
                mouse.kind, mouse.column, mouse.row
            ));
            // La rueda mueve la selección, no el scroll suelto: así la card
            // resaltada y la posición de la lista nunca se contradicen.
            match mouse.kind {
                MouseEventKind::ScrollUp => state.move_selection(-1),
                MouseEventKind::ScrollDown => state.move_selection(1),
                _ => {}
            }

            // Usamos Up (release) en vez de Down (press) porque en Windows
            // el primer Down puede ser consumido al recuperar el foco de la terminal
            // después de que una ventana secundaria se cierra.
            if let MouseEventKind::Up(MouseButton::Left) = mouse.kind {
                if let Some(idx) = card_at(mouse.column, mouse.row, state) {
                    trace_event(&format!("  -> click en card idx={}", idx));
                    state.jump_to(idx);
                    return Ok(MenuAction::Run(state.items[idx]));
                }
                trace_event("  -> click fuera de las cards");
            }

            // Actualizamos el hover con CUALQUIER actividad del mouse
            // (Moved, Drag, Down, Up, scroll). El movimiento libre sin botón
            // (Moved) no llega de forma fiable en consolas de Windows, pero el
            // Drag (botón presionado, incluye el click) sí, y con esto el
            // resaltado sigue al puntero en todos los escenarios.
            let h = card_at(mouse.column, mouse.row, state);
            trace_event(&format!("  -> hover idx={:?}", h));
            state.hover = h;
        }
        Event::Resize(_, _) => state.ensure_visible(),
        _ => {}
    }

    Ok(MenuAction::Continue)
}

fn menu_item_to_task(item: MenuItem, ptype: ProjectType) -> &'static str {
    match (item, ptype) {
        (MenuItem::WindowsDev, ProjectType::Tauri) => "windows-dev",
        (MenuItem::WindowsDev, ProjectType::Flutter) => "windows-dev",
        (MenuItem::WindowsDev, ProjectType::Kotlin) => "windows-dev",
        (MenuItem::WindowsDev, ProjectType::Rust) => "windows-dev",
        (MenuItem::WindowsDev, ProjectType::Go) => "windows-dev",
        (MenuItem::WindowsDev, ProjectType::GoWails) => "windows-dev",
        (MenuItem::WindowsDev, ProjectType::Python) => "windows-dev",

        (MenuItem::AndroidDev, _) => "android-dev",
        (MenuItem::AndroidHmr, ProjectType::GoWails) => "android-hmr",
        (MenuItem::AndroidHmr, ProjectType::Python) => "android-hmr",
        (MenuItem::AndroidHmr, ProjectType::Go) => "go-hmr",

        (MenuItem::WindowsBuild, _) => "windows-build",

        (MenuItem::AndroidBuildArm64, ProjectType::Tauri) => "android-arm64",
        (MenuItem::AndroidBuildArm64, ProjectType::Flutter) => "android-arm64",
        (MenuItem::AndroidBuildArm64, ProjectType::Kotlin) => "android-arm64",
        (MenuItem::AndroidBuildArm64, ProjectType::GoWails) => "android-arm64",

        (MenuItem::AndroidBuildArmv7, ProjectType::Tauri) => "android-armv7",

        (MenuItem::AndroidBuildAll, ProjectType::Tauri) => "android-all",
        (MenuItem::AndroidBuildAll, ProjectType::Flutter) => "android-all",
        (MenuItem::AndroidBuildAll, ProjectType::Kotlin) => "android-all",
        (MenuItem::AndroidBuildAll, ProjectType::GoWails) => "android-all",

        (MenuItem::InstallDebug, _) => "android-install-debug",

        (MenuItem::FlutterProfile, ProjectType::Flutter) => "flutter-profile",
        (MenuItem::Tests, ProjectType::Python) => "python-test",
        (MenuItem::Tests, ProjectType::Go) => "go-test",

        (MenuItem::Clean, _) => "clean",

        (MenuItem::DebugAndroid, _) => "debug-android",

        (MenuItem::Signing, _) => "signing",

        (MenuItem::Version, _) => "version",

        (MenuItem::Git, _) => "git",

        (MenuItem::BuildWhatsApp, _) => "whatsapp",
        (MenuItem::ConfigureTerminal, _) => "terminal",

        _ => unreachable!(),
    }
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    if TRACE.load(Ordering::Relaxed) {
        let _ = std::fs::File::create(std::env::temp_dir().join("compass-tui.log"));
    }

    let mut terminal = setup_terminal()?;
    let mut state = MenuState::new(ctx.project_type, theme::load());

    // Igual que en las tareas: la ventana del menú se titula por proyecto, no
    // por ruta del binario.
    cmd::set_console_title(&platform::dir_display_name(&ctx.script_dir));

    trace_event(&format!(
        "TERMINAL SIZE: {:?} | project: {}",
        terminal.size()?,
        ctx.script_dir.display()
    ));

    let exe_path =
        std::env::current_exe().context("No se pudo determinar la ruta del ejecutable")?;
    let project_path = &ctx.script_dir;

    loop {
        terminal.draw(|f| draw_menu(f, &mut state, ctx))?;

        match handle_event(&mut state)? {
            MenuAction::Quit => {
                let _ = restore_terminal(&mut terminal);
                std::process::exit(0);
            }
            MenuAction::Run(item) => {
                let task = menu_item_to_task(item, ctx.project_type);
                let command = format!(
                    "\"{}\" --task {} --path \"{}\"",
                    exe_path.display(),
                    task,
                    project_path.display(),
                );

                cmd::spawn_new_window_autoexit(
                    &format!("Compass - {}", platform::dir_display_name(project_path)),
                    project_path,
                    &command,
                )?;
            }
            MenuAction::Continue => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    const ALL_ITEMS: [MenuItem; 17] = [
        MenuItem::WindowsDev,
        MenuItem::AndroidDev,
        MenuItem::AndroidHmr,
        MenuItem::WindowsBuild,
        MenuItem::AndroidBuildArm64,
        MenuItem::AndroidBuildArmv7,
        MenuItem::AndroidBuildAll,
        MenuItem::InstallDebug,
        MenuItem::FlutterProfile,
        MenuItem::Tests,
        MenuItem::Clean,
        MenuItem::DebugAndroid,
        MenuItem::Signing,
        MenuItem::Version,
        MenuItem::Git,
        MenuItem::BuildWhatsApp,
        MenuItem::ConfigureTerminal,
    ];

    /// Renderiza una card en un buffer nuevo y devuelve `(fg, bg)` del centro.
    fn card_style(
        item: MenuItem,
        is_selected: bool,
        is_hovered: bool,
    ) -> (Option<Color>, Option<Color>) {
        let area = Rect::new(0, 0, CARD_MAX_W, CARD_H);
        let mut buf = Buffer::empty(area);
        create_card(
            &item,
            is_selected,
            is_hovered,
            ProjectType::Rust,
            crate::theme::DEFAULT,
        )
        .render(area, &mut buf);
        // (1, 1) es la esquina superior izquierda interior: siempre parte de la
        // card, nunca borde.
        let s = buf[(1, 1)].style();
        (s.fg, s.bg)
    }

    /// Regresión: hover y selección compartían `glass_tint`, así que el hover no
    /// se distinguía salvo por un `BOLD` en el borde. Este test falla si vuelve
    /// a pasar.
    #[test]
    fn hover_is_visually_distinct_from_selected() {
        let normal = card_style(MenuItem::WindowsBuild, false, false);
        let hover = card_style(MenuItem::WindowsBuild, false, true);
        let selected = card_style(MenuItem::WindowsBuild, true, false);

        assert_ne!(
            normal, hover,
            "el hover debe verse distinto del estado normal"
        );
        assert_ne!(
            hover, selected,
            "el hover debe verse distinto de la selección (no comparten el mismo fondo)"
        );
        assert_ne!(normal, selected);
    }

    /// Cada card visible debe mapear a una tarea CLI. `menu_item_to_task`
    /// acaba en `unreachable!()` para las combinaciones que no listing, y eso
    /// es un panic en tiempo de ejecución: el test lo convierte en fallo de
    /// compilación de CI.
    #[test]
    fn every_visible_item_maps_to_a_task() {
        for pt in [
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::Kotlin,
            ProjectType::GoWails,
            ProjectType::Rust,
            ProjectType::Python,
            ProjectType::Go,
        ] {
            let state = MenuState::new(pt, crate::theme::DEFAULT);
            for item in &state.items {
                let task = std::panic::catch_unwind(|| menu_item_to_task(*item, pt))
                    .unwrap_or_else(|_| panic!("missing mapping for {item:?} in {pt:?}"));
                assert!(!task.is_empty());
            }
        }
    }

    /// Go + Wails muestra la tarjeta de firmas (el keystore se exporta al
    /// build como variables de entorno) y oculta las que no aplican.
    #[test]
    fn gowails_shows_signing_and_hides_unapplicable_cards() {
        let state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        assert!(state.items.contains(&MenuItem::Signing));
        assert!(state.items.contains(&MenuItem::AndroidBuildArm64));
        assert!(state.items.contains(&MenuItem::AndroidBuildAll));
        assert!(!state.items.contains(&MenuItem::AndroidBuildArmv7));
        assert!(!state.items.contains(&MenuItem::FlutterProfile));
    }

    #[test]
    fn python_menu_exposes_run_reload_package_and_tests_only() {
        let state = MenuState::new(ProjectType::Python, crate::theme::DEFAULT);
        for item in [
            MenuItem::WindowsDev,
            MenuItem::AndroidHmr,
            MenuItem::WindowsBuild,
            MenuItem::Tests,
            MenuItem::Clean,
            MenuItem::Version,
            MenuItem::Git,
        ] {
            assert!(state.items.contains(&item), "falta {item:?}");
        }
        assert!(!state.items.contains(&MenuItem::AndroidDev));
        assert!(!state.items.contains(&MenuItem::Signing));
        assert_eq!(
            menu_item_to_task(MenuItem::AndroidHmr, ProjectType::Python),
            "android-hmr"
        );
        assert_eq!(
            menu_item_to_task(MenuItem::Tests, ProjectType::Python),
            "python-test"
        );
    }

    #[test]
    fn pure_go_menu_exposes_run_build_tests_clean_and_git() {
        let state = MenuState::new(ProjectType::Go, crate::theme::DEFAULT);
        for item in [
            MenuItem::WindowsDev,
            MenuItem::WindowsBuild,
            MenuItem::Tests,
            MenuItem::Clean,
            MenuItem::Git,
        ] {
            assert!(state.items.contains(&item), "falta {item:?}");
        }
        assert!(!state.items.contains(&MenuItem::AndroidDev));
        assert!(!state.items.contains(&MenuItem::Signing));
        assert_eq!(
            menu_item_to_task(MenuItem::WindowsDev, ProjectType::Go),
            "windows-dev"
        );
        assert_eq!(
            menu_item_to_task(MenuItem::Tests, ProjectType::Go),
            "go-test"
        );
        assert_eq!(
            menu_item_to_task(MenuItem::AndroidHmr, ProjectType::Go),
            "go-hmr"
        );
    }

    /// Cada variante tiene su clave de acento, todas únicas y presentes en la
    /// paleta del tema.
    #[test]
    fn card_keys_cover_every_item_and_never_repeat() {
        let mut keys = Vec::new();
        for item in ALL_ITEMS {
            let key = item.color_key();
            assert!(
                crate::theme::CARD_KEYS.contains(&key),
                "{item:?}: clave {key} no está en CARD_KEYS"
            );
            keys.push(key);
        }
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                assert_ne!(keys[i], keys[j], "clave de color repetida: {}", keys[i]);
            }
        }
    }

    /// Dentro de una pestaña no puede haber dos cards con el mismo acento, en
    /// ningún tipo de proyecto ni en ningún tema.
    #[test]
    fn no_two_cards_in_a_tab_share_accent() {
        for pt in [
            ProjectType::Tauri,
            ProjectType::Flutter,
            ProjectType::Kotlin,
            ProjectType::GoWails,
            ProjectType::Rust,
            ProjectType::Python,
            ProjectType::Go,
        ] {
            let state = MenuState::new(pt, crate::theme::DEFAULT);
            for tab in [Tab::DevMnt, Tab::Cfg] {
                let r = state.tabs[tab.index()];
                for t in crate::theme::THEMES {
                    for i in r.start..r.end {
                        for j in i + 1..r.end {
                            assert_ne!(
                                t.card_accent(state.items[i].color_key()),
                                t.card_accent(state.items[j].color_key()),
                                "{}: {pt:?} {tab:?}: cards {i} y {j} comparten acento",
                                t.name
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dev_mnt_tab_merges_both_sections_in_order() {
        let state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        let r = state.tabs[Tab::DevMnt.index()];
        let sections: Vec<&str> = state.items[r.start..r.end]
            .iter()
            .map(|i| i.section())
            .collect();
        assert!(!sections.is_empty());
        assert!(!sections.contains(&"CFG"));
        let first_mnt = sections
            .iter()
            .position(|s| *s == "MNT")
            .expect("la pestaña 1 debe incluir MNT");
        assert!(sections[..first_mnt].iter().all(|s| *s == "DEV"));
        assert!(sections[first_mnt..].iter().all(|s| *s == "MNT"));
    }

    #[test]
    fn cfg_tab_holds_only_config_cards() {
        for pt in [ProjectType::Tauri, ProjectType::GoWails, ProjectType::Rust] {
            let state = MenuState::new(pt, crate::theme::DEFAULT);
            let r = state.tabs[Tab::Cfg.index()];
            assert!(r.len() > 0);
            for item in &state.items[r.start..r.end] {
                assert_eq!(item.section(), "CFG", "{pt:?}: {item:?}");
            }
        }
    }

    #[test]
    fn ensure_visible_keeps_the_selection_in_view() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.vh = 3 * CARD_H; // 3 cards visibles
        s.jump_to(s.tabs[Tab::DevMnt.index()].start + 10);
        s.ensure_visible();

        let top = s.local_selected() as u16 * CARD_H;
        assert!(top >= s.scroll, "la card quedaría por encima del viewport");
        assert!(
            top + CARD_H <= s.scroll + s.vh,
            "la card quedaría por debajo del viewport"
        );
    }

    #[test]
    fn ensure_visible_clamps_at_the_end() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.vh = 3 * CARD_H;
        s.jump_to(s.tabs[Tab::DevMnt.index()].end - 1);

        let n = s.tab_len() as u16;
        assert_eq!(s.scroll, (n - 3) * CARD_H);
    }

    /// Regresión del "Terminal demasiado pequena": con un viewport de una sola
    /// card el menú sigue siendo navegable y hace scroll.
    #[test]
    fn short_viewport_still_scrolls() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.vh = CARD_H;
        for target in 0..s.items.len() {
            s.jump_to(target);
            let top = s.local_selected() as u16 * CARD_H;
            assert!(
                top >= s.scroll && top + CARD_H <= s.scroll + s.vh,
                "card {target} fuera del viewport (scroll={})",
                s.scroll
            );
        }
    }

    #[test]
    fn switching_tab_restores_its_own_selection() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.jump_to(s.tabs[Tab::DevMnt.index()].start + 2);

        s.set_tab(Tab::Cfg);
        assert_eq!(s.selected, s.tabs[Tab::Cfg.index()].start);

        s.jump_to(s.tabs[Tab::Cfg.index()].start + 1);
        s.set_tab(Tab::DevMnt);
        assert_eq!(s.selected, s.tabs[Tab::DevMnt.index()].start + 2);
    }

    #[test]
    fn arrows_jump_tabs_at_the_boundaries() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        let last_dev = s.tabs[Tab::DevMnt.index()].end - 1;
        s.jump_to(last_dev);

        s.move_selection(1);
        assert_eq!(s.tab, Tab::Cfg);
        assert_eq!(s.selected, s.tabs[Tab::Cfg.index()].start);

        s.move_selection(-1);
        assert_eq!(s.tab, Tab::DevMnt);
        assert_eq!(s.selected, last_dev);
    }

    #[test]
    fn card_at_hits_the_right_card_after_scroll() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        let ui = layout(Rect::new(0, 0, 80, 20), false);
        s.ui = Some(ui);
        s.vh = ui.list.height;
        s.scroll = CARD_H;

        // La primera fila visible es la card local 1 tras desplazar una card.
        let hit = card_at(ui.list.x + 2, ui.list.y, &s);
        assert_eq!(hit, Some(s.tabs[Tab::DevMnt.index()].start + 1));
    }

    #[test]
    fn card_at_returns_none_outside_the_viewport() {
        let mut s = MenuState::new(ProjectType::Rust, crate::theme::DEFAULT);
        let ui = layout(Rect::new(0, 0, 80, 20), false);
        s.ui = Some(ui);

        // A la izquierda de la columna.
        let left = ui.list.x.saturating_sub(1);
        assert_eq!(card_at(left, ui.list.y, &s), None);
        // En el pie.
        assert_eq!(card_at(ui.list.x + 2, ui.footer.y, &s), None);
        // Más abajo de la última card del tab (Rust: 4 cards).
        let last_row = ui.list.bottom() - 1;
        assert_eq!(card_at(ui.list.x + 2, last_row, &s), None);
    }

    /// La card seleccionada usa **su** tinte (`card_bg`), no el de la sección.
    #[test]
    fn selected_uses_the_card_own_tint() {
        let (_, bg) = card_style(MenuItem::WindowsBuild, true, false);
        assert_eq!(bg, Some(crate::theme::DEFAULT.card_bg("build")));
    }

    /// El fondo seleccionado debe salir del tema activo, no de una tabla fija:
    /// con otro tema, la misma card cambia de color.
    #[test]
    fn selected_background_follows_the_active_theme() {
        let area = Rect::new(0, 0, CARD_MAX_W, CARD_H);
        let mut buf = Buffer::empty(area);
        let dracula = crate::theme::by_name("Dracula").unwrap();
        create_card(
            &MenuItem::WindowsBuild,
            true,
            false,
            ProjectType::Rust,
            dracula,
        )
        .render(area, &mut buf);
        let bg = buf[(1, 1)].style().bg;
        assert_eq!(bg, Some(dracula.card_bg("build")));
        assert_ne!(bg, Some(crate::theme::DEFAULT.card_bg("build")));
    }

    #[test]
    fn normal_card_has_no_background() {
        let (_, bg) = card_style(MenuItem::WindowsBuild, false, false);
        // `Buffer::empty` inicializa el fondo a `Color::Reset` (el del terminal),
        // no a `None`. `Reset` significa "no pintar", que es lo que se quiere.
        assert!(
            matches!(bg, None | Some(Color::Reset)),
            "una card en reposo no debe pintar fondo propio, recibió {bg:?}"
        );
    }

    /// El hover nunca debe ganar al estado seleccionado cuando ambas cosas
    /// ocurren a la vez (pasa con el ratón sobre la card ya elegida con teclado).
    #[test]
    fn selected_wins_over_hover() {
        assert_eq!(
            card_style(MenuItem::WindowsBuild, true, true),
            card_style(MenuItem::WindowsBuild, true, false),
            "seleccionada+hovereada debe pintarse como seleccionada"
        );
    }

    /// Abrir el modal resalta el tema activo y permite previsualizar otro sin
    /// persistirlo.
    #[test]
    fn picker_opens_on_the_active_theme_and_previews() {
        let mut state = MenuState::new(ProjectType::Rust, crate::theme::DEFAULT);
        state.open_picker();

        let picker = state.picker.expect("el modal debe quedar abierto");
        assert_eq!(
            picker.index,
            crate::theme::index_of(crate::theme::DEFAULT.name)
        );
        assert_eq!(picker.original.name, crate::theme::DEFAULT.name);

        // Vista previa: cambia el tema y el resaltado, sin cerrar el modal.
        let next = crate::theme::next(crate::theme::DEFAULT.name);
        state.preview_theme(next);
        assert!(
            state.picker.is_some(),
            "la vista previa no debe cerrar el modal"
        );
        assert_eq!(state.theme.name, next.name);
        assert_eq!(
            state.picker.unwrap().index,
            crate::theme::index_of(next.name)
        );
    }

    /// `Esc` debe poder volver al tema con el que se abrió el modal.
    #[test]
    fn picker_keeps_the_original_theme_for_cancel() {
        let mut state = MenuState::new(ProjectType::Rust, crate::theme::DEFAULT);
        state.open_picker();
        state.preview_theme(crate::theme::next(crate::theme::DEFAULT.name));

        let original = state.picker.unwrap().original;
        state.theme = original;
        state.picker = None;
        assert_eq!(state.theme.name, crate::theme::DEFAULT.name);
        assert!(state.picker.is_none());
    }

    fn test_ctx(project_type: ProjectType) -> ProjectContext {
        ProjectContext {
            script_dir: std::path::PathBuf::from("C:/tmp/proyecto"),
            project_type,
            android_home: Some(std::path::PathBuf::from("C:/Android")),
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        }
    }

    /// Dibujo real (TestBackend) en terminales grandes y diminutas: el menú
    /// nunca debe entrar en pánico, y con 30 filas debe caber al menos 8 cards.
    #[test]
    fn draws_both_tabs_without_panicking_at_any_size() {
        use ratatui::backend::TestBackend;

        for (w, h) in [(80, 30), (80, 24), (60, 12), (200, 60), (20, 4), (80, 5)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            let ctx = test_ctx(ProjectType::GoWails);
            let mut state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);

            term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();
            state.set_tab(Tab::Cfg);
            term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();
            state.set_tab(Tab::DevMnt);
            term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

            if h >= 6 {
                let ui = state.ui.expect("debería haber layout utilizable");
                if h == 30 {
                    assert!(
                        ui.list.height / CARD_H >= 8,
                        "con 30 filas deben caber 8 cards, caben {}",
                        ui.list.height / CARD_H
                    );
                }
            } else {
                assert!(state.ui.is_none(), "{w}x{h}: ni una card cabe");
            }
        }
    }

    /// En una terminal de 80x24 el scroll sigue siendo necesario para ver las
    /// últimas cards de la pestaña DEV·MNT de Go+Wails (11 cards).
    #[test]
    fn scroll_is_required_on_a_short_terminal() {
        use ratatui::backend::TestBackend;

        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let ctx = test_ctx(ProjectType::GoWails);
        let mut state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);

        state.jump_to(state.tabs[Tab::DevMnt.index()].end - 1);
        term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

        assert!(state.scroll > 0, "la última card exige desplazar la lista");
        let top = state.local_selected() as u16 * CARD_H;
        assert!(top + CARD_H <= state.scroll + state.vh);
    }
}
