use std::io::{self, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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

use crate::container::{Container, BAR_HEIGHT};
use crate::theme::{self, Theme};
use crate::utils::cmd;
use crate::utils::console_guard;
use crate::utils::context::{ProjectContext, ProjectType};
use crate::utils::platform;

/// Alto de una card: borde superior + una línea de contenido + borde inferior.
pub(crate) const CARD_H: u16 = 3;
/// Columnas vacías entre cards de una fila y entre filas: no se tocan.
pub(crate) const CARD_GAP: u16 = 1;
/// Alto de una fila de la rejilla: la card más su separación inferior.
pub(crate) const ROW_H: u16 = CARD_H + CARD_GAP;
/// Ancho mínimo de una card. Por debajo, la rejilla baja de columnas en vez
/// de comprimirlas hasta romper etiqueta y descripción.
const CARD_MIN_W: u16 = 24;
/// Cards por fila como máximo.
const GRID_MAX_COLS: usize = 4;
pub(crate) const FOOTER_H: u16 = 1;
pub(crate) const TABS_H: u16 = 1;
/// Duración de una vuelta completa del halo de hover.
pub(crate) const HOVER_CYCLE: Duration = Duration::from_millis(2600);
/// Longitud de la cola luminosa del halo, en celdas de borde.
const SHIMMER_TRAIL: f32 = 9.0;
/// Radio del resplandor interior alrededor de la cabeza del halo.
const GLOW_RADIUS: f32 = 11.0;
/// Intervalo entre fotogramas mientras hay una animación en curso (~30 fps).
pub(crate) const ANIM_FRAME: Duration = Duration::from_millis(33);
/// Espera del bucle de eventos cuando no hay nada que animar.
pub(crate) const IDLE_POLL: Duration = Duration::from_secs(1);

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
    /// Respaldo y restauración del entorno de desarrollo de Windows
    /// (variables, inventario de herramientas y archivos de configuración).
    Environment,
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
            Self::Environment => "\u{1f4be}",
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
            Self::Git
            | Self::BuildWhatsApp
            | Self::ConfigureTerminal
            | Self::Environment => "CFG",
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
            Self::Environment => "environment",
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
                ProjectType::GoWails => "APK x3",
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
            Self::Environment => "Entorno",
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
                Self::Environment => "Backup y restauracion",
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
                Self::Environment => "Backup y restauracion",
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
                Self::Environment => "Backup y restauracion",
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
                Self::Environment => "Backup y restauracion",
                Self::FlutterProfile => "",
                Self::AndroidHmr => "",
                Self::Tests => "",
            },
            ProjectType::GoWails => match self {
                Self::WindowsDev => "dev escritorio",
                Self::AndroidDev => "APK dev",
                Self::AndroidHmr => "HMR (Vite)",
                Self::WindowsBuild => "x86 · x64 · ARM64",
                Self::AndroidBuildArm64 => "arm64 · armv7 · universal",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "AAB (Play Store)",
                Self::InstallDebug => "APK dev",
                Self::Clean => "Limpia builds/deps",
                Self::DebugAndroid => "logcat de la app",
                Self::Signing => "Firmas y huellas",
                Self::Git => "Init · gitignore",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::Version => "build/config.yml",
                Self::ConfigureTerminal => "Autocompletado",
                Self::Environment => "Backup y restauracion",
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
                Self::Environment => "Backup y restauracion",
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
                Self::Environment => "Backup y restauracion",
                Self::FlutterProfile => "",
                Self::Tests => "go test ./...",
            },
        }
    }
}

/// Regiones de la pantalla. Lo calcula [`layout`], pura, para que el dibujado y
/// el hit-testing del ratón no se desincronicen al redimensionar.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UiLayout {
    pub(crate) header: Rect,
    pub(crate) tabs: Rect,
    /// Región visible de la rejilla de cards (recorta el scroll).
    pub(crate) list: Rect,
    /// Cards por fila que caben en `list`.
    pub(crate) cols: usize,
    /// Columna de una celda a la derecha de la lista para la barra de scroll.
    /// `width == 0` si la terminal no deja hueco.
    pub(crate) scrollbar: Rect,
    pub(crate) footer: Rect,
}

/// Cards por fila para un ancho dado: hasta [`GRID_MAX_COLS`], y menos si las
/// cards bajarían de [`CARD_MIN_W`].
fn grid_cols(width: u16) -> usize {
    ((width + CARD_GAP) / (CARD_MIN_W + CARD_GAP)).clamp(1, GRID_MAX_COLS as u16) as usize
}

/// Ancho de una columna de la rejilla: la card más su separación derecha.
pub(crate) fn grid_cell_w(width: u16, cols: usize) -> u16 {
    (width / cols.max(1) as u16).max(1)
}

/// Ancho de la card dentro de su columna.
pub(crate) fn grid_card_w(width: u16, cols: usize) -> u16 {
    grid_cell_w(width, cols).saturating_sub(CARD_GAP)
}

/// Filas de cards que caben enteras en un viewport de `height`.
pub(crate) fn visible_rows(height: u16) -> usize {
    ((height + CARD_GAP) / ROW_H) as usize
}

pub(crate) fn layout(area: Rect, warn: bool) -> UiLayout {
    let header_h: u16 = if warn { 3 } else { 2 };
    let [header, tabs, list, footer] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(TABS_H),
        Constraint::Min(0),
        Constraint::Length(FOOTER_H),
    ])
    .areas(area);

    // La rejilla de cards se adapta al ancho de la ventana: ocupa todo el
    // espacio disponible salvo un margen a la izquierda y, si cabe, el hueco
    // más la barra de scroll a la derecha.
    let margin = if list.width > 4 { 1 } else { 0 };
    let bar_room = if list.width >= 8 { 2 } else { 0 };
    let list_w = list.width.saturating_sub(margin + bar_room);
    let list_rect = Rect {
        x: list.x + margin,
        width: list_w,
        ..list
    };
    let cols = grid_cols(list_w);
    // La barra va una columna a la derecha de las cards para no pisar su
    // borde; si no cabe, se omite (dibujado y `card_at` la ignoran).
    let scrollbar = if bar_room == 2 {
        Rect {
            x: list_rect.right() + 1,
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
        cols,
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
    /// Instante en que el puntero entró en la card hovereada. Arranca el
    /// barrido de brillo y se limpia al salir de la card.
    hover_started: Option<Instant>,
    /// Primera fila visible de la rejilla. Se desplaza de fila completa.
    scroll: u16,
    /// Alto del viewport. La fija `draw_menu` en cada frame.
    vh: u16,
    /// Cards por fila. La fija `draw_menu` en cada frame.
    cols: usize,
    /// Última geometría dibujada; `None` si la terminal no admite ni una card.
    ui: Option<UiLayout>,
    /// Tema activo. `&'static` porque los temas viven en `theme::THEMES`.
    theme: &'static Theme,
    /// Selector de tema abierto (`Ctrl+T`). `None` = menú normal.
    picker: Option<ThemePicker>,
    /// Aviso al salir con pestañas de tarea abiertas. `None` = no se ha
    /// intentado salir todavía (o se canceló).
    quit_confirm: Option<QuitConfirm>,
}

/// Datos del aviso de salida: cuántas pestañas hay y cuántas siguen vivas.
#[derive(Debug, Clone, Copy)]
struct QuitConfirm {
    tabs: usize,
    running: usize,
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
            MenuItem::Environment,
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
            hover_started: None,
            scroll: 0,
            vh: 0,
            cols: 1,
            ui: None,
            theme,
            picker: None,
            quit_confirm: None,
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

    /// Actualiza la card bajo el puntero. El halo luminoso arranca al entrar en
    /// una card nueva: mover el ratón dentro de la misma no lo reinicia.
    fn set_hover(&mut self, hover: Option<usize>) {
        if self.hover == hover {
            return;
        }
        self.hover = hover;
        self.hover_started = hover.map(|_| Instant::now());
    }

    /// Fase (0..1) del halo mientras el puntero está sobre una card. El halo
    /// gira en bucle: una vuelta completa por [`HOVER_CYCLE`].
    fn hover_phase(&self) -> Option<f32> {
        let started = self.hover_started?;
        let phase = started.elapsed().as_secs_f32() / HOVER_CYCLE.as_secs_f32();
        Some(phase.fract())
    }

    fn hover_animating(&self) -> bool {
        self.hover.is_some() && self.hover_started.is_some()
    }

    /// Ajusta `scroll` (en filas) para que la card seleccionada quede entera
    /// dentro del viewport. Las filas se desplazan completas, así que ninguna
    /// card se dibuja a medias.
    fn ensure_visible(&mut self) {
        let n = self.tab_len();
        if n == 0 || self.vh < CARD_H {
            self.scroll = 0;
            return;
        }
        let cols = self.cols.max(1);
        let rows = visible_rows(self.vh).max(1);
        let i = self.local_selected().min(n - 1);
        let row = i / cols;
        let total_rows = (n + cols - 1) / cols;
        let max = total_rows.saturating_sub(rows);

        let mut s = self.scroll as usize;
        if row < s {
            s = row;
        } else if row >= s + rows {
            s = row + 1 - rows;
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
    /// al borde de una pestaña salta a la adyacente. Es el paso horizontal de
    /// la rejilla (y el de la rueda del ratón).
    fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let max = self.items.len() as isize - 1;
        let next = (self.selected as isize + delta).clamp(0, max) as usize;
        self.jump_to(next);
    }

    /// Flechas verticales: mueven una fila de la rejilla sin salir de la
    /// pestaña actual (cambiar de pestaña es cosa de `←`/`→` y de `Tab`).
    fn move_row(&mut self, delta: isize) {
        let n = self.tab_len();
        if n == 0 {
            return;
        }
        let step = self.cols.max(1) as isize;
        let local = self.local_selected().min(n - 1) as isize;
        let next = (local + delta * step).clamp(0, n as isize - 1) as usize;
        self.jump_to(self.tab_range().start + next);
    }
}

pub(crate) fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    Terminal::new(backend)
}

pub(crate) fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

pub(crate) fn show_android_warning(ctx: &ProjectContext) -> bool {
    ctx.android_home.is_none() && ctx.project_type != ProjectType::Python
}

/// Dibuja el menú a pantalla completa (sólo tests).
#[cfg(test)]
fn draw_menu(frame: &mut Frame, state: &mut MenuState, ctx: &ProjectContext) {
    draw_menu_in(frame, frame.area(), state, ctx);
}

/// Dibuja el menú dentro de `area` (la zona bajo la barra de pestañas del
/// contenedor).
fn draw_menu_in(frame: &mut Frame, area: Rect, state: &mut MenuState, ctx: &ProjectContext) {
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
    state.cols = ui.cols;

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
    render_scrollbar(frame, ui, state.tab_len(), state.scroll, theme);
    render_footer(frame, ui.footer, theme, ctx);

    // Los modales van al final para quedar por encima de todo lo demás.
    if let Some(picker) = state.picker {
        render_theme_picker(frame, area, theme, picker);
    }
    if let Some(confirm) = state.quit_confirm {
        render_quit_confirm(frame, area, theme, confirm);
    }

    state.ui = Some(ui);
}

/// Frame completo: barra de pestañas del contenedor y, debajo, el menú o la
/// sesión de terminal activa.
fn draw_ui(frame: &mut Frame, state: &mut MenuState, ctx: &ProjectContext, panes: &mut Container) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(state.theme.background)),
        area,
    );

    let [bar, body] =
        Layout::vertical([Constraint::Length(BAR_HEIGHT), Constraint::Min(0)]).areas(area);

    if panes.active().is_some() {
        // El terminal no tiene cards: el hit-testing del menú queda apagado
        // para que un clic no ejecute nada de la pantalla oculta.
        state.ui = None;
        panes.draw_body(frame, body, state.theme);
    } else {
        draw_menu_in(frame, body, state, ctx);
    }
    panes.draw_bar(frame, bar, state.theme);
}

/// Etiquetas de la barra de pestañas, con su cuenta de cards.
///
/// Las comparten el dibujado y [`tab_at`]: si el texto y las cuentas cambian,
/// el hit-testing del ratón sigue apuntando a la pestaña correcta.
fn tab_titles(state: &MenuState) -> [String; 2] {
    [
        format!(
            " DESARROLLO · MNT {} ",
            state.tabs[Tab::DevMnt.index()].len()
        ),
        format!(" CONFIGURACION {} ", state.tabs[Tab::Cfg.index()].len()),
    ]
}

fn render_tabs(frame: &mut Frame, area: Rect, state: &MenuState, theme: &Theme) {
    let titles: Vec<Line> = tab_titles(state).into_iter().map(Line::from).collect();
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

/// Pestaña bajo las coordenadas del mouse, o `None` si el clic no cae en una.
///
/// El ancho de cada etiqueta es su cuenta de caracteres (todas son de ancho 1)
/// y entre ambas va el divisor `" │ "`. Es la misma disposición que pinta
/// [`render_tabs`].
fn tab_at(col: u16, row: u16, state: &MenuState) -> Option<Tab> {
    let ui = state.ui?;
    if !ui.tabs.contains(Position::new(col, row)) {
        return None;
    }
    let titles = tab_titles(state);
    let start = ui.tabs.x as usize;
    let col = col as usize;
    let dev_end = start + titles[0].chars().count();
    if col < dev_end {
        return Some(Tab::DevMnt);
    }
    let cfg_start = dev_end + " │ ".chars().count();
    if col >= cfg_start && col < cfg_start + titles[1].chars().count() {
        return Some(Tab::Cfg);
    }
    None
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
    let cols = ui.cols;
    let cell_w = grid_cell_w(ui.list.width, cols);
    let card_w = grid_card_w(ui.list.width, cols);
    let list_y = ui.list.y as i32;
    let list_bottom = ui.list.bottom() as i32;
    let total_rows = (n + cols - 1) / cols;

    for row in state.scroll as usize..total_rows {
        let y = list_y + ((row - state.scroll as usize) * ROW_H as usize) as i32;
        // Filas enteras o nada: el scroll avanza de fila en fila, así que sólo
        // la última podría sobrar; se omite en vez de dibujarla recortada.
        if y + CARD_H as i32 > list_bottom {
            break;
        }
        for col in 0..cols {
            let local = row * cols + col;
            if local >= n {
                break;
            }
            let idx = start + local;
            let rect = Rect {
                x: ui.list.x + col as u16 * cell_w,
                y: y as u16,
                width: card_w,
                height: CARD_H,
            };
            let card = create_card(
                &state.items[idx],
                state.selected == idx,
                state.hover == Some(idx),
                project_type,
                theme,
                card_w,
            );
            frame.render_widget(card, rect);

            // Halo luminoso circular sobre la card hovereada.
            if state.hover == Some(idx) {
                if let Some(phase) = state.hover_phase() {
                    let key = state.items[idx].color_key();
                    render_hover_glow(
                        frame,
                        rect,
                        phase,
                        theme.card_accent(key),
                        theme.card_bg(key),
                        theme.text,
                    );
                }
            }
        }
    }
}

/// Halo luminoso que recorre el borde de `rect` en bucle (sentido horario) y
/// derrama un resplandor interior alrededor de su cabeza, al estilo del orbe
/// del icono de Google.
///
/// Se pinta **después** de la card, celda a celda sobre el buffer, porque el
/// `Block` del Paragraph tiene un único color de borde. El brillo se mezcla con
/// el color que ya tenía cada celda: en el borde cambia el `fg`; en el interior
/// tiñe el `bg` hacia el tinte oscuro del acento, sin tocar el texto.
pub(crate) fn render_hover_glow(
    frame: &mut Frame,
    rect: Rect,
    phase: f32,
    accent: Color,
    tint: Color,
    bright: Color,
) {
    if rect.width < 2 || rect.height < 2 {
        return;
    }
    let w = rect.width as usize;
    let h = rect.height as usize;
    // Perímetro del borde: arriba + abajo + laterales (las esquinas ya cuentan
    // en arriba/abajo).
    let total = (2 * w + 2 * (h - 2)) as f32;
    let phase = phase.rem_euclid(1.0);
    let head = phase * total;
    // Respiración: dos pulsos de intensidad por vuelta.
    let breath = 0.72 + 0.28 * (phase * std::f32::consts::TAU * 2.0).sin();
    let glow = shimmer_glow(accent, bright);

    // 1. Cola luminosa sobre el borde, con distancia circular para que la
    //    cabeza y su estela crucen las esquinas sin saltos.
    let mut pos = 0usize;
    let mut paint_border = |frame: &mut Frame, x: u16, y: u16| {
        let raw = (pos as f32 - head).abs();
        let dist = raw.min(total - raw);
        let k = (1.0 - dist / SHIMMER_TRAIL).max(0.0) * breath;
        if k > 0.02 {
            let cell = &mut frame.buffer_mut()[(x, y)];
            let fg = match cell.fg {
                Color::Rgb(..) => theme::mix(cell.fg, glow, k.min(1.0)),
                // Los colores ANSI (tema `Compass`) no se pueden interpolar:
                // el tramo pegado a la cabeza se pinta con el brillo y el
                // resto conserva el color del borde.
                _ if k >= 0.3 => glow,
                _ => cell.fg,
            };
            cell.set_fg(fg);
        }
        pos += 1;
    };

    for x in rect.x..rect.right() {
        paint_border(frame, x, rect.y);
    }
    for y in rect.y + 1..rect.bottom() {
        paint_border(frame, rect.right() - 1, y);
    }
    for x in (rect.x..rect.right() - 1).rev() {
        paint_border(frame, x, rect.bottom() - 1);
    }
    for y in (rect.y + 1..rect.bottom() - 1).rev() {
        paint_border(frame, rect.x, y);
    }

    // 2. Resplandor interior: el fondo se oscurece hacia el tinte del acento
    //    cerca de la cabeza. Radio elíptico porque la card es ancha y baja.
    let (cx, cy) = perimeter_point(rect, head);
    for y in rect.y + 1..rect.bottom() - 1 {
        for x in rect.x + 1..rect.right() - 1 {
            let dx = x as f32 - cx as f32;
            let dy = (y as f32 - cy as f32) * 2.0;
            let d = (dx * dx + dy * dy).sqrt();
            let k = (1.0 - d / GLOW_RADIUS).max(0.0) * 0.8 * breath;
            if k > 0.02 {
                let cell = &mut frame.buffer_mut()[(x, y)];
                cell.set_bg(theme::mix(cell.bg, tint, k.min(1.0)));
            }
        }
    }
}

/// Coordenada `(x, y)` del punto `pos` del perímetro de `rect`, en el mismo
/// recorrido que hace [`render_hover_glow`] (arriba → derecha → abajo →
/// izquierda).
fn perimeter_point(rect: Rect, pos: f32) -> (u16, u16) {
    let w = rect.width as f32;
    let h = rect.height as f32;
    let total = 2.0 * w + 2.0 * (h - 2.0);
    let mut p = pos.rem_euclid(total);
    // Arriba (incluye las dos esquinas superiores).
    if p < w {
        return (rect.x + p as u16, rect.y);
    }
    p -= w;
    // Derecha (sin la esquina superior, con la inferior).
    if p < h - 1.0 {
        return (rect.right() - 1, rect.y + p as u16 + 1);
    }
    p -= h - 1.0;
    // Abajo (de derecha a izquierda).
    if p < w - 1.0 {
        return (rect.right() - 2 - p as u16, rect.bottom() - 1);
    }
    p -= w - 1.0;
    // Izquierda (sin esquinas).
    (rect.x, rect.bottom() - 2 - p as u16)
}

/// Brillo del barrido: mezcla el acento de la card con el texto del tema cuando
/// ambos son RGB (temas OpenCode). Con colores ANSI (`Compass`) `mix` no puede
/// interpolar nombres de color, así que el acento se aclara hacia el blanco.
fn shimmer_glow(accent: Color, text: Color) -> Color {
    match (accent, text) {
        (Color::Rgb(..), Color::Rgb(..)) => theme::mix(accent, text, 0.55),
        (Color::Rgb(r, g, b), _) => {
            let light = |c: u8| (c as u16 + (255 - c as u16) * 55 / 100) as u8;
            Color::Rgb(light(r), light(g), light(b))
        }
        _ => accent,
    }
}

pub(crate) fn render_scrollbar(
    frame: &mut Frame,
    ui: UiLayout,
    count: usize,
    scroll: u16,
    theme: &Theme,
) {
    let rows = (count + ui.cols - 1) / ui.cols;
    let content = rows.saturating_mul(ROW_H as usize);
    if ui.scrollbar.width == 0 || content <= ui.list.height as usize {
        return;
    }
    let mut sb_state = ScrollbarState::new(content)
        .position(scroll as usize * ROW_H as usize)
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

/// Aviso al salir: deja claro cuántas pestañas internas se cerrarán (y si
/// alguna sigue con un proceso vivo) antes de matar sus árboles.
fn render_quit_confirm(frame: &mut Frame, area: Rect, theme: &Theme, confirm: QuitConfirm) {
    let w = 52u16.min(area.width);
    let h = 6u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, rect);

    let tabs = match confirm.tabs {
        1 => " Hay 1 pestaña abierta.".to_string(),
        n => format!(" Hay {n} pestañas abiertas."),
    };
    let running = match confirm.running {
        0 => " Sus procesos ya terminaron.".to_string(),
        1 => " 1 de ellas con un proceso en ejecución.".to_string(),
        n => format!(" {n} de ellas con procesos en ejecución."),
    };
    let lines = vec![
        Line::from(Span::styled(
            tabs,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(running, Style::default().fg(theme.warning))),
        Line::from(""),
        Line::from(Span::styled(
            " Al salir se cerrarán sus pestañas y procesos.",
            Style::default().fg(theme.muted),
        )),
    ];

    let block = Block::default()
        .style(Style::default().bg(theme.background))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .title(Span::styled(
            " Salir de Compass ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(vec![
            Span::styled(" Enter ", Style::default().fg(theme.text)),
            Span::styled("salir y cerrar  ", Style::default().fg(theme.muted)),
            Span::styled("Esc ", Style::default().fg(theme.text)),
            Span::styled("cancelar ", Style::default().fg(theme.muted)),
        ]));

    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);
}

pub(crate) fn render_header(frame: &mut Frame, area: Rect, ctx: &ProjectContext, theme: &Theme) {
    let project_name = ctx
        .script_dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed("Proyecto"));

    // El tipo de proyecto ya lo muestra el badge de la segunda línea: el
    // título es el nombre de la app y su versión real de compilación.
    let emoji = match ctx.project_type {
        ProjectType::Tauri => "\u{1f980}",
        ProjectType::Flutter => "\u{1f3af}",
        ProjectType::Kotlin => "\u{1f537}",
        ProjectType::Rust => "\u{2699}",
        ProjectType::Go => "\u{1f4e6}",
        ProjectType::GoWails => "\u{1f408}",
        ProjectType::Python => "\u{1f40d}",
    };
    let badge = match ctx.project_type {
        ProjectType::Tauri => "TAURI",
        ProjectType::Flutter => "FLUTTER",
        ProjectType::Kotlin => "KOTLIN",
        ProjectType::Rust => "RUST",
        ProjectType::Go => "GO",
        ProjectType::GoWails => "GO",
        ProjectType::Python => "PYTHON",
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
                format!("  {}  Compass  v", emoji),
                Style::default()
                    .fg(theme.brand)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(theme.muted)),
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
        Span::styled(
            "\u{2190}\u{2192}\u{2191}\u{2193}",
            Style::default().fg(theme.text),
        ),
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
    width: u16,
) -> Paragraph<'static> {
    card_widget(
        item.icon(),
        item.label(project_type),
        item.description(project_type),
        item.color_key(),
        is_selected,
        is_hovered,
        theme,
        width,
    )
}

/// Card de tres líneas con acento propio, compartida con los submenús (Firmas,
/// Git) para que su aspecto sea exactamente el del menú principal.
///
/// | Estado | Borde | Fondo |
/// |---|---|---|
/// | reposo | acento propio, sin BOLD | ninguno |
/// | hover | `text` + BOLD | `hover_bg` neutro |
/// | seleccionado | acento propio + BOLD | `card_bg` (tinte del acento) |
pub(crate) fn card_widget(
    icon: &str,
    label: &str,
    desc: &str,
    key: &str,
    is_selected: bool,
    is_hovered: bool,
    theme: &Theme,
    width: u16,
) -> Paragraph<'static> {
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
        // Etiqueta a la izquierda y descripción pegada al borde derecho. Los
        // anchos se miden en celdas (`Span::width`), no en `chars`: los emojis
        // ocupan más de una y descuadraban el relleno. En la rejilla de cuatro
        // columnas la descripción no siempre cabe: se recorta con "…" y, si ni
        // eso deja hueco, se omite para no pisar la etiqueta.
        let inner_w = width.saturating_sub(2) as usize;
        let label_w = spans[0].width();
        let available = inner_w.saturating_sub(label_w);
        // Un hueco delante + el de cierre del borde derecho.
        let room = available.saturating_sub(2);
        if room >= 3 {
            let shown: String = if desc.chars().count() > room {
                let mut s: String = desc.chars().take(room - 1).collect();
                s.push('\u{2026}');
                s
            } else {
                desc.to_string()
            };
            let pad = available.saturating_sub(shown.chars().count() + 1).max(1);
            spans.push(Span::raw(" ".repeat(pad)));
            spans.push(Span::styled(shown, desc_style));
            spans.push(Span::raw(" "));
        }
    }

    Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left)
        .block(block)
}

/// Índice global (en `state.items`) de la card bajo las coordenadas del mouse,
/// o `None` si está fuera de la rejilla visible o en un hueco entre cards.
fn card_at(col: u16, row: u16, state: &MenuState) -> Option<usize> {
    let ui = state.ui?;
    let local = grid_index_at(ui, state.scroll, state.tab_len(), col, row)?;
    Some(state.tab_range().start + local)
}

/// Índice de card (relativo a la lista pintada) bajo unas coordenadas, o `None`
/// si el punto cae fuera de la rejilla visible o en un hueco entre cards.
///
/// Lo comparten el menú principal y los submenús: la geometría es la misma
/// (`layout` + rejilla adaptativa + scroll por filas).
pub(crate) fn grid_index_at(
    ui: UiLayout,
    scroll: u16,
    len: usize,
    col: u16,
    row: u16,
) -> Option<usize> {
    if !ui.list.contains(Position::new(col, row)) {
        return None;
    }
    let cell_w = grid_cell_w(ui.list.width, ui.cols);
    let card_w = grid_card_w(ui.list.width, ui.cols);

    let x_off = (col - ui.list.x) as usize;
    let grid_col = x_off / cell_w as usize;
    // El ancho de una columna puede no dividir el de la lista (resto < cols):
    // más allá de la última card no hay nada.
    if grid_col >= ui.cols || x_off % cell_w as usize >= card_w as usize {
        return None;
    }

    let y_off = (row - ui.list.y) as usize + scroll as usize * ROW_H as usize;
    if y_off % ROW_H as usize >= CARD_H as usize {
        return None;
    }
    let local = (y_off / ROW_H as usize) * ui.cols + grid_col;
    if local >= len {
        return None;
    }
    Some(local)
}

enum MenuAction {
    Run(MenuItem),
    /// F12 desde el menú: volver a la última pestaña de terminal.
    Back,
    Quit,
    Continue,
}

fn handle_event(state: &mut MenuState, panes: &mut Container) -> anyhow::Result<MenuAction> {
    // Con una animación de hover en curso se repinta a ~30 fps; sin ella el
    // bucle espera un segundo (teclado y ratón lo despiertan igual, y así no
    // se redibuja de más cuando la ventana está inactiva).
    let timeout = if state.hover_animating() {
        ANIM_FRAME
    } else {
        IDLE_POLL
    };
    if !crossterm::event::poll(timeout)? {
        return Ok(MenuAction::Continue);
    }
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

    // Con el aviso de salida abierto el teclado le pertenece: Enter confirma
    // (sale) y Esc o `n` cancelan. El ratón no responde, igual que en el modal
    // de temas.
    if state.quit_confirm.is_some() {
        if let Event::Key(key) = evt {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Enter => return Ok(MenuAction::Quit),
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        state.quit_confirm = None;
                    }
                    _ => {}
                }
            }
        }
        return Ok(MenuAction::Continue);
    }

    match evt {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            state.set_hover(None);

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
                KeyCode::F(12) => return Ok(MenuAction::Back),
                KeyCode::Enter => return Ok(MenuAction::Run(state.items[state.selected])),
                // En la rejilla las flechas siguen la geometría: horizontales
                // dentro de la fila (saltando de pestaña en el borde) y
                // verticales de fila en fila. Las pestañas se cambian con Tab.
                KeyCode::Up => state.move_row(-1),
                KeyCode::Down => state.move_row(1),
                KeyCode::Left => state.move_selection(-1),
                KeyCode::Right => state.move_selection(1),
                KeyCode::Tab => state.set_tab(state.tab.next()),
                KeyCode::BackTab => state.set_tab(state.tab.prev()),
                _ => {}
            }
        }
        Event::Mouse(mouse) => {
            trace_event(&format!(
                "MOUSE: {:?} col={} row={}",
                mouse.kind, mouse.column, mouse.row
            ));
            // La rueda mueve la selección una fila, no el scroll suelto: así la
            // card resaltada y la posición de la lista nunca se contradicen.
            match mouse.kind {
                MouseEventKind::ScrollUp => state.move_row(-1),
                MouseEventKind::ScrollDown => state.move_row(1),
                _ => {}
            }

            // Usamos Up (release) en vez de Down (press) porque en Windows
            // el primer Down puede ser consumido al recuperar el foco de la terminal
            // después de que una ventana secundaria se cierra.
            if let MouseEventKind::Up(MouseButton::Left) = mouse.kind {
                // La barra del contenedor va por encima del menú: un clic ahí
                // cambia (o abre) la pestaña de terminal y no ejecuta cards.
                if panes.click(mouse.column, mouse.row) {
                    state.set_hover(None);
                    return Ok(MenuAction::Continue);
                }
                // Las pestañas también responden al ratón: un clic en su
                // etiqueta cambia de pestaña (sin ejecutar nada).
                if let Some(tab) = tab_at(mouse.column, mouse.row, state) {
                    trace_event(&format!("  -> clic en pestana {:?}", tab));
                    state.set_tab(tab);
                    state.set_hover(None);
                    return Ok(MenuAction::Continue);
                }
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
            state.set_hover(h);
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
        (MenuItem::Environment, _) => "environment",

        _ => unreachable!(),
    }
}

/// Abre la tarea en una ventana externa de PowerShell (plan B y escape
/// `COMPASS_EXTERNAL_WINDOWS=1`).
fn spawn_in_window(
    exe: &std::path::Path,
    task: &str,
    project: &std::path::Path,
) -> anyhow::Result<()> {
    let command = format!(
        "\"{}\" --task {} --path \"{}\"",
        exe.display(),
        task,
        project.display(),
    );
    cmd::spawn_new_window_autoexit(
        &format!("Compass - {}", platform::dir_display_name(project)),
        project,
        &command,
    )
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    if TRACE.load(Ordering::Relaxed) {
        let _ = std::fs::File::create(std::env::temp_dir().join("compass-tui.log"));
    }

    let mut terminal = setup_terminal()?;
    let mut state = MenuState::new(ctx.project_type, theme::load());

    // Cada proyecto abre su propia ventana; el título lleva el directorio y el
    // tipo para distinguirlas de un vistazo.
    cmd::set_console_title(&ctx.window_title());

    // Con pestañas internas, el cierre de la ventana anfitriona no debe
    // llevarse por delante sus procesos (ver `utils::console_guard`).
    console_guard::install();

    trace_event(&format!(
        "TERMINAL SIZE: {:?} | project: {}",
        terminal.size()?,
        ctx.script_dir.display()
    ));

    let exe_path =
        std::env::current_exe().context("No se pudo determinar la ruta del ejecutable")?;
    let project_path = &ctx.script_dir;
    let mut panes = Container::new();

    loop {
        terminal.draw(|f| draw_ui(f, &mut state, ctx, &mut panes))?;

        // Con una pestaña de terminal activa, el teclado y el ratón son suyos.
        if panes.active().is_some() {
            panes.poll_input()?;
            continue;
        }

        match handle_event(&mut state, &mut panes)? {
            MenuAction::Quit => {
                // Salir con pestañas abiertas mata sus procesos: se avisa una
                // vez y se pide confirmación. Con el aviso ya abierto, el Enter
                // llega aquí de nuevo y sale.
                if state.quit_confirm.is_none() {
                    let tabs = panes.tab_count();
                    if tabs > 0 {
                        state.quit_confirm = Some(QuitConfirm {
                            tabs,
                            running: panes.running_count(),
                        });
                        continue;
                    }
                }
                // `process::exit` no ejecuta destructores: sin cerrar aquí
                // las pestañas, sus procesos sobreviven a Compass.
                panes.close_all();
                let _ = restore_terminal(&mut terminal);
                std::process::exit(0);
            }
            MenuAction::Run(item) => {
                let task = menu_item_to_task(item, ctx.project_type);

                // `COMPASS_EXTERNAL_WINDOWS=1` recupera el comportamiento
                // antiguo (una ventana por tarjeta) sin recompilar.
                if std::env::var_os("COMPASS_EXTERNAL_WINDOWS").is_some() {
                    spawn_in_window(&exe_path, task, project_path)?;
                    continue;
                }

                // La pestaña corre exactamente el mismo CLI que abría la
                // ventana externa, pero con su consola dentro de Compass.
                let script = format!(
                    "& {} --task {} --path {}",
                    cmd::ps_quote(&exe_path.to_string_lossy()),
                    task,
                    cmd::ps_quote(&project_path.to_string_lossy()),
                );
                let size = terminal.size()?;
                // Barra de pestañas + pie del contenedor.
                let rows = size.height.saturating_sub(BAR_HEIGHT + 1);
                let cols = size.width;
                let title = item.label(ctx.project_type).to_string();

                if let Err(e) = panes.open(title, &script, project_path, rows, cols) {
                    // Sin ConPTY la tarjeta no puede quedarse sin hacer nada:
                    // se cae a la ventana externa de siempre.
                    trace_event(&format!("SESSION ERROR: {e:#}"));
                    spawn_in_window(&exe_path, task, project_path)?;
                }
            }
            MenuAction::Back => {
                panes.show_last();
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

    const ALL_ITEMS: [MenuItem; 18] = [
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
        MenuItem::Environment,
    ];

    /// Ancho de render de las pruebas de estilo; en la TUI real la card ocupa
    /// el ancho útil de la ventana.
    const TEST_CARD_W: u16 = 64;

    /// Renderiza una card en un buffer nuevo y devuelve `(fg, bg)` del centro.
    fn card_style(
        item: MenuItem,
        is_selected: bool,
        is_hovered: bool,
    ) -> (Option<Color>, Option<Color>) {
        let area = Rect::new(0, 0, TEST_CARD_W, CARD_H);
        let mut buf = Buffer::empty(area);
        create_card(
            &item,
            is_selected,
            is_hovered,
            ProjectType::Rust,
            crate::theme::DEFAULT,
            TEST_CARD_W,
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
        s.cols = 4;
        s.vh = 2 * ROW_H; // dos filas de cards visibles
        s.jump_to(s.tabs[Tab::DevMnt.index()].start + 10);
        s.ensure_visible();

        let row = (s.local_selected() / s.cols) as u16;
        assert!(row >= s.scroll, "la card quedaría por encima del viewport");
        assert!(
            row < s.scroll + visible_rows(s.vh) as u16,
            "la card quedaría por debajo del viewport"
        );
    }

    #[test]
    fn ensure_visible_clamps_at_the_end() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.cols = 4;
        s.vh = 2 * ROW_H;
        s.jump_to(s.tabs[Tab::DevMnt.index()].end - 1);

        let rows = (s.tab_len() + s.cols - 1) / s.cols;
        assert_eq!(s.scroll as usize, rows - visible_rows(s.vh));
    }

    /// Regresión del "Terminal demasiado pequena": con un viewport de una sola
    /// fila el menú sigue siendo navegable y hace scroll.
    #[test]
    fn short_viewport_still_scrolls() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.cols = 4;
        s.vh = CARD_H;
        for target in 0..s.items.len() {
            s.jump_to(target);
            let row = (s.local_selected() / s.cols) as u16;
            assert!(
                row >= s.scroll && row < s.scroll + 1,
                "card {target} fuera del viewport (scroll={})",
                s.scroll
            );
        }
    }

    /// La navegación sigue la rejilla: `↑`/`↓` saltan una fila entera y
    /// `←`/`→` se mueven de card en card. Las filas no cambian de pestaña.
    #[test]
    fn arrows_follow_the_grid() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.cols = 4;
        let start = s.tabs[Tab::DevMnt.index()].start;

        s.jump_to(start + 1);
        s.move_row(1);
        assert_eq!(s.selected, start + 5, "bajar una fila suma las columnas");
        assert_eq!(s.tab, Tab::DevMnt);

        s.move_selection(1);
        assert_eq!(s.selected, start + 6, "la horizontal avanza una card");

        // En la última fila, bajar no sale de la pestaña.
        s.jump_to(s.tabs[Tab::DevMnt.index()].end - 1);
        s.move_row(1);
        assert_eq!(s.tab, Tab::DevMnt);
        assert_eq!(s.selected, s.tabs[Tab::DevMnt.index()].end - 1);
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
        let ui = layout(Rect::new(0, 0, 120, 20), false);
        s.ui = Some(ui);
        s.vh = ui.list.height;
        s.cols = ui.cols;
        s.scroll = 1;

        // La primera fila visible es la 1: su primera card está `cols` más allá.
        let hit = card_at(ui.list.x + 2, ui.list.y, &s);
        assert_eq!(hit, Some(s.tabs[Tab::DevMnt.index()].start + ui.cols));

        // Los huecos entre cards y entre filas no pertenecen a ninguna.
        let card_w = grid_card_w(ui.list.width, ui.cols);
        assert_eq!(card_at(ui.list.x + card_w, ui.list.y, &s), None);
        assert_eq!(
            card_at(ui.list.x + 2, ui.list.y + CARD_H, &s),
            None,
            "el hueco entre filas no es clicable"
        );
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

    /// La barra de pestañas responde al ratón: cada columna de una etiqueta
    /// mapea a su pestaña, y un clic ahí cambia de pestaña sin ejecutar nada.
    #[test]
    fn tab_at_maps_columns_to_each_tab() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        let ui = layout(Rect::new(0, 0, 80, 20), false);
        s.ui = Some(ui);

        let titles = tab_titles(&s);
        let dev_w = titles[0].chars().count() as u16;
        let cfg_start = ui.tabs.x + dev_w + " │ ".chars().count() as u16;
        let row = ui.tabs.y;

        assert_eq!(tab_at(ui.tabs.x, row, &s), Some(Tab::DevMnt));
        assert_eq!(tab_at(ui.tabs.x + dev_w - 1, row, &s), Some(Tab::DevMnt));
        assert_eq!(tab_at(cfg_start, row, &s), Some(Tab::Cfg));
        assert_eq!(tab_at(cfg_start + 1, row, &s), Some(Tab::Cfg));

        // El divisor y las filas fuera de la barra no pertenecen a nadie.
        assert_eq!(tab_at(ui.tabs.x + dev_w, row, &s), None);
        assert_eq!(tab_at(ui.tabs.x, row + 1, &s), None);

        // Clic en la segunda pestaña: cambia de pestaña.
        let clicked = tab_at(cfg_start, row, &s).expect("clic sobre CONFIGURACION");
        s.set_tab(clicked);
        assert_eq!(s.tab, Tab::Cfg);
    }

    /// La rejilla se adapta al ancho: hasta 4 cards por fila, y menos (más
    /// anchas) cuando la ventana no da para separarlas con holgura.
    #[test]
    fn cards_fill_the_window_in_a_four_column_grid() {
        let narrow = layout(Rect::new(0, 0, 60, 20), false);
        let wide = layout(Rect::new(0, 0, 200, 20), false);

        assert_eq!(narrow.cols, 2, "60 columnas dan dos cards por fila");
        assert_eq!(
            wide.cols, GRID_MAX_COLS,
            "la ventana ancha llega al máximo de 4"
        );
        assert!(
            grid_card_w(wide.list.width, wide.cols) > grid_card_w(narrow.list.width, narrow.cols),
            "la card debe crecer con la terminal"
        );
        assert_eq!(narrow.list.x, 1, "margen izquierdo de una columna");
        assert_eq!(wide.list.x, 1);
        // Sólo quedan fuera de la rejilla el margen izquierdo y el hueco + barra.
        assert_eq!(wide.list.right() + 2, 200);
        assert_eq!(wide.scrollbar.x, wide.list.right() + 1);
        assert_eq!(wide.scrollbar.width, 1);
    }

    /// La primera fila se pinta como una rejilla: una esquina `╭` por card y
    /// una columna de hueco entre ellas (4 en ventanas anchas, menos si no
    /// caben con holgura).
    #[test]
    fn first_row_shows_separated_cards() {
        use ratatui::backend::TestBackend;

        for (w, expected) in [(120u16, 4usize), (60, 2)] {
            let mut term = Terminal::new(TestBackend::new(w, 30)).unwrap();
            let ctx = test_ctx(ProjectType::GoWails);
            let mut state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
            term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

            let ui = state.ui.expect("layout dibujado");
            let buf = term.backend().buffer();
            let corners: Vec<u16> = (ui.list.x..ui.list.right())
                .filter(|&x| buf[(x, ui.list.y)].symbol() == "\u{256d}")
                .collect();
            assert_eq!(corners.len(), expected, "{w} columnas: {corners:?}");

            let card_w = grid_card_w(ui.list.width, ui.cols);
            for pair in corners.windows(2) {
                assert_eq!(
                    pair[1] - pair[0],
                    card_w + CARD_GAP,
                    "las cards deben quedar separadas por el hueco"
                );
            }
        }
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
        let area = Rect::new(0, 0, TEST_CARD_W, CARD_H);
        let mut buf = Buffer::empty(area);
        let dracula = crate::theme::by_name("Dracula").unwrap();
        create_card(
            &MenuItem::WindowsBuild,
            true,
            false,
            ProjectType::Rust,
            dracula,
            TEST_CARD_W,
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

    /// El halo arranca al **entrar** en una card: moverse dentro de la misma
    /// no lo reinicia, y salir lo cancela.
    #[test]
    fn hover_starts_the_glow_only_when_entering_a_card() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        let idx = s.tabs[Tab::DevMnt.index()].start + 3;

        s.set_hover(Some(idx));
        assert!(s.hover_animating(), "entrar en una card inicia el halo");
        let started = s.hover_started.expect("el instante de entrada");

        s.set_hover(Some(idx));
        assert_eq!(
            s.hover_started,
            Some(started),
            "seguir en la misma card no reinicia la animación"
        );

        s.set_hover(None);
        assert!(!s.hover_animating());
        assert!(s.hover_started.is_none(), "sin hover no hay animación");
    }

    /// El halo gira en bucle: la fase vuelve a 0 cada `HOVER_CYCLE`, así que
    /// sigue animándose mientras el puntero permanezca sobre la card.
    #[test]
    fn hover_phase_cycles_while_hovering() {
        let mut s = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        s.set_hover(Some(s.tabs[Tab::DevMnt.index()].start));

        s.hover_started = Some(Instant::now() - HOVER_CYCLE / 2);
        let half = s.hover_phase().expect("hay halo");
        assert!((half - 0.5).abs() < 0.05, "media vuelta ≈ fase 0.5: {half}");

        s.hover_started = Some(Instant::now() - HOVER_CYCLE * 3 / 2);
        let wrapped = s.hover_phase().expect("el halo no caduca");
        assert!((wrapped - 0.5).abs() < 0.05, "vuelta y media ≈ fase 0.5");
        assert!(s.hover_animating(), "mientras haya hover el halo sigue");
    }

    /// El halo pinta sobre la card hovereada: el borde cambia de `fg` y el
    /// interior de `bg` (resplandor), respecto del mismo hover sin animación.
    #[test]
    fn hover_glow_brightens_the_border_and_the_interior() {
        use ratatui::backend::TestBackend;

        let mut term = Terminal::new(TestBackend::new(80, 30)).unwrap();
        let ctx = test_ctx(ProjectType::GoWails);
        let mut state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);
        term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

        let local = 1;
        let idx = state.tabs[Tab::DevMnt.index()].start + local;
        state.set_hover(Some(idx));
        // Hover sin halo: éste es el estado de referencia.
        state.hover_started = None;
        term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();
        let settled = term.backend().buffer().clone();

        // Media vuelta: la cabeza está en el borde y su resplandor, dentro.
        state.hover_started = Some(Instant::now() - HOVER_CYCLE / 2);
        term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();
        let glowing = term.backend().buffer().clone();

        let ui = state.ui.expect("layout dibujado");
        let rect = Rect {
            x: ui.list.x + local as u16 * grid_cell_w(ui.list.width, ui.cols),
            y: ui.list.y,
            width: grid_card_w(ui.list.width, ui.cols),
            height: CARD_H,
        };

        let mut border_changes = 0;
        for x in rect.x..rect.right() {
            for y in [rect.y, rect.bottom() - 1] {
                if settled[(x, y)].fg != glowing[(x, y)].fg {
                    border_changes += 1;
                }
            }
        }
        for y in rect.y..rect.bottom() {
            for x in [rect.x, rect.right() - 1] {
                if settled[(x, y)].fg != glowing[(x, y)].fg {
                    border_changes += 1;
                }
            }
        }
        assert!(
            border_changes > 0,
            "el halo debe iluminar celdas del borde de la card hovereada"
        );

        let mut interior_changes = 0;
        for x in rect.x + 1..rect.right() - 1 {
            if settled[(x, rect.y + 1)].bg != glowing[(x, rect.y + 1)].bg {
                interior_changes += 1;
            }
        }
        assert!(
            interior_changes > 0,
            "el resplandor debe teñir el fondo interior de la card"
        );
    }

    /// El recorrido del halo por el perímetro no se sale del borde.
    #[test]
    fn perimeter_point_walks_the_border() {
        let rect = Rect::new(3, 2, 10, 3);
        for pos in [0.0, 4.9, 9.9, 10.0, 15.5, 21.9, 22.5] {
            let (x, y) = perimeter_point(rect, pos);
            assert!(
                x == rect.x || x == rect.right() - 1 || y == rect.y || y == rect.bottom() - 1,
                "pos={pos} cayó fuera del borde: ({x}, {y})"
            );
        }
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
                    let cards = visible_rows(ui.list.height) * ui.cols;
                    assert!(
                        cards >= 8,
                        "con 30 filas deben caber 8 cards, caben {cards}"
                    );
                }
            } else {
                assert!(state.ui.is_none(), "{w}x{h}: ni una card cabe");
            }
        }
    }

    /// En una terminal de 60x20 el scroll sigue siendo necesario para ver las
    /// últimas cards de la pestaña DEV·MNT de Go+Wails (11 cards en 2
    /// columnas = 6 filas; sólo caben 4).
    #[test]
    fn scroll_is_required_on_a_short_terminal() {
        use ratatui::backend::TestBackend;

        let mut term = Terminal::new(TestBackend::new(60, 20)).unwrap();
        let ctx = test_ctx(ProjectType::GoWails);
        let mut state = MenuState::new(ProjectType::GoWails, crate::theme::DEFAULT);

        state.jump_to(state.tabs[Tab::DevMnt.index()].end - 1);
        term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

        assert!(state.scroll > 0, "la última card exige desplazar la lista");
        let row = (state.local_selected() / state.cols) as u16;
        assert!(row >= state.scroll && row < state.scroll + visible_rows(state.vh) as u16);
    }

    /// Salir con pestañas abiertas mata sus procesos: el aviso debe decirlo y
    /// ofrecer cancelar, antes de que el `close_all` llegue a ejecutarse.
    #[test]
    fn quit_confirm_warns_about_open_tabs() {
        use ratatui::backend::TestBackend;

        let ctx = test_ctx(ProjectType::Rust);
        for (confirm, count_text) in [
            (
                QuitConfirm {
                    tabs: 1,
                    running: 0,
                },
                "Hay 1 pestaña abierta",
            ),
            (
                QuitConfirm {
                    tabs: 3,
                    running: 2,
                },
                "Hay 3 pestañas abiertas",
            ),
        ] {
            let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
            let mut state = MenuState::new(ProjectType::Rust, crate::theme::DEFAULT);
            state.quit_confirm = Some(confirm);
            term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

            let buf = term.backend().buffer();
            let screen: String = (0..buf.area.height)
                .map(|y| {
                    (0..buf.area.width)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(screen.contains(count_text), "falta el aviso: {screen}");
            assert!(
                screen.contains("Al salir se cerrarán sus pestañas"),
                "el aviso debe explicar qué se cerrará: {screen}"
            );
            assert!(
                screen.contains("cancelar"),
                "Esc debe poder cancelar la salida: {screen}"
            );
        }
    }

    /// El encabezado muestra la versión real del binario (`CARGO_PKG_VERSION`),
    /// no un literal que se queda atrás al subir la versión en `Cargo.toml`.
    #[test]
    fn header_shows_the_real_compass_version() {
        use ratatui::backend::TestBackend;

        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let ctx = test_ctx(ProjectType::Rust);
        let mut state = MenuState::new(ProjectType::Rust, crate::theme::DEFAULT);
        term.draw(|f| draw_menu(f, &mut state, &ctx)).unwrap();

        let buf = term.backend().buffer();
        let screen: String = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            screen.contains(&format!("Compass  v{}", env!("CARGO_PKG_VERSION"))),
            "el encabezado debe mostrar la versión del paquete: {screen}"
        );
        assert!(
            !screen.contains("Compass for"),
            "el tipo de proyecto va en el badge, no en el título: {screen}"
        );
    }
}
