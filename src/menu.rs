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
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame, Terminal,
};

use crate::theme::{self, Theme};
use crate::utils::cmd;
use crate::utils::context::{ProjectContext, ProjectType};
use crate::utils::platform;

const COLS: usize = 3;
const CARD_W: u16 = 24;
const CARD_H: u16 = 4;
const FOOTER_H: u16 = 1;

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
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildArmv7,
    AndroidBuildAll,
    InstallDebug,
    FlutterProfile,
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
    ContextMenu,
}

impl MenuItem {
    fn icon(&self) -> &str {
        match self {
            Self::WindowsDev => "\u{1f5a5}",
            Self::AndroidDev => "\u{1f4f1}",
            Self::WindowsBuild => "\u{1f4e6}",
            Self::AndroidBuildArm64 => "\u{1f680}",
            Self::AndroidBuildArmv7 => "\u{1f680}",
            Self::AndroidBuildAll => "\u{1f680}",
            Self::InstallDebug => "\u{1f4e6}",
            Self::FlutterProfile => "\u{1f3af}",
            Self::Clean => "\u{1f9f9}",
            Self::DebugAndroid => "\u{1f50d}",
            Self::Signing => "\u{1f511}",
            Self::Version => "\u{1f516}",
            Self::Git => "\u{1f527}",
            Self::BuildWhatsApp => "\u{1f4ac}",
            Self::ConfigureTerminal => "\u{2699}",
            Self::ContextMenu => "\u{1f4c1}",
        }
    }

    fn section(&self) -> &str {
        match self {
            Self::WindowsDev | Self::AndroidDev | Self::WindowsBuild | Self::AndroidBuildArm64 | Self::AndroidBuildArmv7 | Self::AndroidBuildAll | Self::InstallDebug | Self::FlutterProfile => "DEV",
            Self::Clean | Self::DebugAndroid | Self::Signing | Self::Version => "MNT",
            Self::Git | Self::BuildWhatsApp | Self::ConfigureTerminal | Self::ContextMenu => "CFG",
        }
    }

    fn label(&self, project_type: ProjectType) -> &str {
        match self {
            Self::WindowsDev => match project_type {
                ProjectType::Rust => "Run",
                // La tarjeta cubre el dev de escritorio, así que se nombra por
                // plataforma: decir "Windows" en Pop!_OS-era confuso y la tarea
                // emitía `-d windows`, que no existe fuera de Windows.
                _ if platform::IS_WINDOWS => "Windows",
                _ => "Linux",
            },
            Self::AndroidDev => "Android",
            Self::WindowsBuild => match project_type {
                ProjectType::Rust => "Build",
                _ if platform::IS_WINDOWS => "Win Build",
                _ => "Linux Build",
            },
            Self::AndroidBuildArm64 => match project_type {
                ProjectType::Tauri => "Android a64",
                ProjectType::Flutter => "APK",
                ProjectType::Kotlin => "APK",
                ProjectType::Rust => "",
            },
            Self::AndroidBuildArmv7 => match project_type {
                ProjectType::Tauri => "Android a7",
                ProjectType::Flutter => "",
                ProjectType::Kotlin => "",
                ProjectType::Rust => "",
            },
            Self::AndroidBuildAll => match project_type {
                ProjectType::Tauri => "Android All",
                ProjectType::Flutter => "AAB",
                ProjectType::Kotlin => "AAB",
                ProjectType::Rust => "",
            },
            Self::InstallDebug => "Install",
            Self::FlutterProfile => "Profile",
            Self::Clean => "Clean",
            Self::DebugAndroid => "Debug Andr",
            Self::Signing => "Signing",
            Self::Version => "Version",
            Self::Git => "Git",
            Self::BuildWhatsApp => "WhatsApp",
            Self::ConfigureTerminal => "Terminal",
            Self::ContextMenu => "Menu Ctx",
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
                Self::ContextMenu => "Menu contextual",
                Self::FlutterProfile => "",
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
                Self::ContextMenu => "Menu contextual",
                Self::FlutterProfile => "",
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
                Self::ContextMenu => "Menu contextual",
                Self::FlutterProfile => "flutter run --profile",
            },
            ProjectType::Kotlin => match self {
                Self::WindowsDev => "gradlew :composeApp:run",
                Self::AndroidDev => "gradlew :installDebug",
                Self::WindowsBuild => "gradlew :packageDist",
                Self::AndroidBuildArm64 => "gradlew :assembleRelease",
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
                Self::ContextMenu => "Menu contextual",
                Self::FlutterProfile => "",
            },
        }
    }
}

struct MenuState {
    items: Vec<MenuItem>,
    selected: usize,
    groups: Vec<(String, Color, Vec<MenuItem>)>,
    hover: Option<usize>,
    area_width: u16,
    header_height: u16,
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
            MenuItem::WindowsBuild,
            MenuItem::AndroidBuildArm64,
            MenuItem::AndroidBuildArmv7,
            MenuItem::AndroidBuildAll,
            MenuItem::InstallDebug,
            MenuItem::FlutterProfile,
            MenuItem::Clean,
            MenuItem::DebugAndroid,
            MenuItem::Signing,
            MenuItem::Version,
            MenuItem::Git,
            MenuItem::BuildWhatsApp,
            MenuItem::ConfigureTerminal,
            MenuItem::ContextMenu,
        ];

        let items: Vec<MenuItem> = base.into_iter()
            .filter(|item| match project_type {
                ProjectType::Rust => !matches!(item,
                    MenuItem::AndroidDev
                    | MenuItem::AndroidBuildArm64
                    | MenuItem::AndroidBuildArmv7
                    | MenuItem::AndroidBuildAll
                    | MenuItem::InstallDebug
                    | MenuItem::DebugAndroid
                    | MenuItem::Signing
                    | MenuItem::FlutterProfile
                ),
                ProjectType::Tauri => !matches!(item, MenuItem::InstallDebug | MenuItem::FlutterProfile),
                ProjectType::Flutter => !matches!(item, MenuItem::AndroidBuildArmv7 | MenuItem::Version | MenuItem::InstallDebug),
                ProjectType::Kotlin => !matches!(item, MenuItem::AndroidBuildArmv7 | MenuItem::Version | MenuItem::FlutterProfile),
            })
            // `ConfigureTerminal` escribe un perfil de PowerShell (PSReadLine), que
            // no existe en Linux, así que se oculta en vez de exponer una tarjeta
            // que fallaría. `ContextMenu` sí se muestra: en Pop!_OS/COSMIC lo
            // implementa `context_menu_cosmic.rs`.
            .filter(|item| cfg!(windows) || !matches!(item, MenuItem::ConfigureTerminal))
            .collect();

        Self {
            items,
            selected: 0,
            groups: Vec::new(),
            hover: None,
            area_width: 0,
            header_height: 2,
            theme,
            picker: None,
        }
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

fn calc_group_sizes(items: &[MenuItem], theme: &Theme) -> Vec<(String, Color, Vec<MenuItem>)> {
    let mut dev = Vec::new();
    let mut mnt = Vec::new();
    let mut cfg = Vec::new();

    for item in items {
        match item.section() {
            "DEV" => dev.push(*item),
            "MNT" => mnt.push(*item),
            _ => cfg.push(*item),
        }
    }

    let mut groups = Vec::new();
    if !dev.is_empty() { groups.push(("DESARROLLO".to_string(), theme.dev, dev)); }
    if !mnt.is_empty() { groups.push(("MANTENIMIENTO".to_string(), theme.mnt, mnt)); }
    if !cfg.is_empty() { groups.push(("CONFIGURACION".to_string(), theme.cfg, cfg)); }
    groups
}

fn calc_total_height(ctx: &ProjectContext, items: &[MenuItem], theme: &Theme) -> u16 {
    let warn = ctx.android_home.is_none();
    let header_h: u16 = if warn { 3 } else { 2 };
    let groups = calc_group_sizes(items, theme);
    let sec_h: u16 = groups.len() as u16;
    let cards_h: u16 = groups.iter().map(|(_, _, items)| {
        let rows = (items.len() + COLS - 1) / COLS;
        rows as u16 * CARD_H
    }).sum();
    header_h + sec_h + cards_h + FOOTER_H
}

fn draw_menu(frame: &mut Frame, state: &mut MenuState, ctx: &ProjectContext) {
    let area = frame.area();
    let theme = state.theme;
    let total_h = calc_total_height(ctx, &state.items, theme);

    if area.height < total_h {
        let msg = Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled(
                " Terminal demasiado pequena.",
                Style::default().fg(theme.warning),
            )]),
            Line::from(vec![Span::styled(
                format!(" Necesita al menos {} lineas.", total_h),
                Style::default().fg(theme.muted),
            )]),
            Line::from(vec![Span::styled(
                if cfg!(windows) {
                    " Maximiza la ventana (Alt+Enter) y presiona una tecla."
                } else {
                    " Maximiza la ventana (Shift+Ctrl+Mas o F11) y presiona una tecla."
                },
                Style::default().fg(theme.muted),
            )]),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    state.groups.clear();

    let mut y = area.y;
    let header_h: u16 = if ctx.android_home.is_none() { 3 } else { 2 };

    render_header(frame, Rect { x: area.x, y, width: area.width, height: header_h }, ctx, theme);
    y += header_h;

    state.header_height = header_h;
    state.area_width = area.width;

    let groups = calc_group_sizes(&state.items, theme);
    for (name, color, items) in &groups {
        let rect = Rect { x: area.x, y, width: area.width, height: 1 };
        let text = Text::from(vec![Line::from(vec![Span::styled(
            format!("  {} ", name),
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        )])]);
        frame.render_widget(Paragraph::new(text), rect);
        y += 1;

        let rows = (items.len() + COLS - 1) / COLS;
        let cards_h = rows as u16 * CARD_H;
        let start_idx = state.items.iter().position(|i| *i == items[0]).unwrap_or(0);
        state.groups.push((name.clone(), *color, items.clone()));
        render_group_cards(frame, Rect { x: area.x, y, width: area.width, height: cards_h }, state, items, start_idx, ctx.project_type, theme);
        y += cards_h;
    }

    render_footer(
        frame,
        Rect {
            x: area.x,
            y: area.y + area.height - FOOTER_H,
            width: area.width,
            height: FOOTER_H,
        },
        theme,
    );

    // El modal va al final para quedar por encima de todo lo demás.
    if let Some(picker) = state.picker {
        render_theme_picker(frame, area, theme, picker);
    }
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
    let rect = Rect { x, y, width: w, height: h };

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
            line = line.style(Style::default().bg(theme.hover_bg).add_modifier(Modifier::BOLD));
        }
        lines.push(line);
    }

    let block = Block::default()
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
    };
    let badge_color = match ctx.project_type {
        ProjectType::Tauri => theme.success,
        ProjectType::Flutter => theme.info,
        ProjectType::Kotlin => theme.accent,
        ProjectType::Rust => theme.warning,
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("  {}  {}  v", emoji, label), Style::default().fg(theme.brand).add_modifier(Modifier::BOLD)),
            Span::styled("0.1.0", Style::default().fg(theme.muted)),
        ]),
        Line::from(vec![
            Span::styled("     Proyecto: ", Style::default().fg(theme.muted)),
            Span::styled(project_name.to_string(), Style::default().fg(theme.text)),
            Span::styled("  ", Style::default()),
            Span::styled(
                format!(" {} ", badge),
                Style::default().bg(badge_color).fg(Color::Black).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if ctx.android_home.is_none() {
        lines.push(Line::from(vec![
            Span::styled("    \u{26a0} ANDROID_HOME no configurada", Style::default().fg(theme.warning)),
        ]));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_group_cards(frame: &mut Frame, area: Rect, state: &MenuState, items: &[MenuItem], start_global_idx: usize, project_type: ProjectType, theme: &Theme) {
    let total_grid_w = COLS as u16 * CARD_W;
    let grid_start_x = area.x + (area.width.saturating_sub(total_grid_w)) / 2;

    for rel_idx in 0..items.len() {
        let idx = start_global_idx + rel_idx;
        let row = rel_idx / COLS;
        let col = rel_idx % COLS;

        let x = grid_start_x + col as u16 * CARD_W;
        let y = area.y + row as u16 * CARD_H;

        let rect = Rect { x, y, width: CARD_W, height: CARD_H };

        let is_selected = state.selected == idx;
        let is_hovered = state.hover == Some(idx);
        let card = create_card(&items[rel_idx], is_selected, is_hovered, project_type, theme);
        frame.render_widget(card, rect);
    }
}

fn render_footer(frame: &mut Frame, area: Rect, theme: &Theme) {
    // El indicador de tema va alineado a la derecha, en su propio rect: así en
    // una terminal de 80 columnas lo que se recorta (si acaso) son los puntos de
    // sección de la izquierda, nunca el nombre del tema activo.
    let right = format!("Ctrl+T temas: {} ", theme.name);
    let right_w = right.chars().count() as u16;
    let left_w = area.width.saturating_sub(right_w);

    let left = Line::from(vec![
        Span::styled("  \u{2191}\u{2193}\u{2190}\u{2192}", Style::default().fg(theme.text)),
        Span::styled(" navegar   ", Style::default().fg(theme.muted)),
        Span::styled("Enter", Style::default().fg(theme.text)),
        Span::styled(" ejecutar   ", Style::default().fg(theme.muted)),
        Span::styled("Esc", Style::default().fg(theme.text)),
        Span::styled(" salir      ", Style::default().fg(theme.muted)),
        Span::styled("\u{25cf} DEV ", Style::default().fg(theme.dev)),
        Span::styled("\u{25cf} MNT ", Style::default().fg(theme.mnt)),
        Span::styled("\u{25cf} CFG", Style::default().fg(theme.cfg)),
    ]);
    frame.render_widget(
        Paragraph::new(left),
        Rect { width: left_w, ..area },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            right,
            Style::default().fg(theme.brand),
        )))
        .alignment(Alignment::Right),
        Rect { x: area.x + left_w, width: right_w, ..area },
    );
}

/// ⚠️ El hover **no funciona en cosmic-term**, y no es culpa de este código.
///
/// cosmic-term implementa el modo 1002 (movimiento sólo con botón pulsado), no
/// el 1003 (all-motion). En `mouse_reporter.rs`, tanto `sgr_mouse_code` como
/// `normal_mouse_code` hacen:
///
/// ```text
/// Event::Mouse(MouseEvent::CursorMoved { .. }) =>
///     self.button.map(|button| (button_number(button) + 32, "M"))
/// ```
///
/// `Option::map` sobre `None` devuelve `None` ⇒ **sin botón pulsado no se envía
/// ningún byte**. Además `report_mouse` sólo consulta `SGR_MOUSE` y el modo
/// normal; nunca mira `ANY_EVENT_MOUSE` (1003), que es el que crossterm activa
/// con `EnableMouseCapture`.
///
/// Resultado: la app pide movimiento, cosmic-term no lo manda, y `state.hover`
/// nunca cambia de `None`. Los colores de hover de aquí sí son correctos y se
/// aplican en terminales que sí implementen 1003 (alacritty, kitty,
/// gnome-terminal, wezterm, foot).
///
/// La card **seleccionada** usa el tinte del tema (`theme.selected_bg`, teñido
/// con el acento de su sección) y la **hovereada** el fondo neutro
/// (`theme.hover_bg`). A propósito no comparten tinte: antes usaban el mismo
/// `glass_tint` y sólo se distinguían por el `BOLD` del borde, una diferencia
/// tan leve que el hover parecía no funcionar. Que no colisionen en ningún tema
/// lo cubren `theme::tests::hover_is_distinct_from_every_selection` y
/// `tests::hover_is_visually_distinct_from_selected`.
fn create_card(item: &MenuItem, is_selected: bool, is_hovered: bool, project_type: ProjectType, theme: &Theme) -> Paragraph<'static> {
    let section = item.section();
    let section_color = theme.section(section);

    // Tres estados bien separados. Si una card está seleccionada y además
    // hovereada, manda la selección: es la que Enter ejecutaría.
    let border = if is_selected {
        Style::default().fg(section_color).add_modifier(Modifier::BOLD)
    } else if is_hovered {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.border)
    };

    let icon = item.icon();
    let label = item.label(project_type);
    let desc = item.description(project_type);

    let line1 = if section.is_empty() {
        format!(" {}  {}", icon, label)
    } else {
        format!(" {} [{}] {}", icon, section, label)
    };

    // El título también marca el estado, no sólo el borde.
    let (label_color, bold) = if is_selected {
        (section_color, true)
    } else if is_hovered {
        (theme.text, true)
    } else {
        (section_color, false)
    };
    let mut line1_style = Style::default().fg(label_color);
    if bold {
        line1_style = line1_style.add_modifier(Modifier::BOLD);
    }

    // La descripción se aclara en hover: en reposo es `muted`, que sobre el
    // panel neutro del hover queda casi ilegible, así que sube a `text`.
    let desc_color = if is_hovered { theme.text } else { theme.muted };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border);
    if is_selected {
        block = block.style(Style::default().bg(theme.selected_bg(section)));
    } else if is_hovered {
        block = block.style(Style::default().bg(theme.hover_bg));
    }

    Paragraph::new(Text::from(vec![
        Line::from(vec![Span::styled(line1, line1_style)]),
        Line::from(vec![Span::styled(format!(" {}", desc), Style::default().fg(desc_color))]),
    ]))
    .alignment(Alignment::Center)
    .block(block)
}

/// Devuelve el índice global (en `state.items`) de la card bajo las coordenadas
/// del mouse, o None si está fuera de la grilla.
fn card_at(col: u16, row: u16, state: &MenuState, ctx: &ProjectContext) -> Option<usize> {
    let header_h: u16 = if ctx.android_home.is_none() { 3 } else { 2 };
    let mut y_cursor = header_h;

    let groups = calc_group_sizes(&state.items, state.theme);
    for (_, _color, items) in &groups {
        y_cursor += 1;

        let total_grid_w = COLS as u16 * CARD_W;
        let grid_start_x = (state.area_width.saturating_sub(total_grid_w)) / 2;
        let rows = (items.len() + COLS - 1) / COLS;
        let cards_h = rows as u16 * CARD_H;

        if row >= y_cursor && row < y_cursor + cards_h && col >= grid_start_x && col < grid_start_x + total_grid_w {
            let rel_row = (row - y_cursor) / CARD_H;
            let rel_col = (col - grid_start_x) / CARD_W;
            let rel_idx = rel_row as usize * COLS + rel_col as usize;
            if rel_idx < items.len() {
                let global_idx = state.items.iter().position(|i| *i == items[rel_idx]).unwrap_or(0);
                return Some(global_idx);
            }
        }

        y_cursor += cards_h;
    }
    None
}

enum MenuAction {
    Run(MenuItem),
    Quit,
    Continue,
}

fn handle_event(state: &mut MenuState, ctx: &ProjectContext) -> anyhow::Result<MenuAction> {
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
                KeyCode::Up => {
                    state.selected = state.selected.saturating_sub(COLS);
                }
                KeyCode::Down => {
                    let max = state.items.len() - 1;
                    state.selected = std::cmp::min(state.selected + COLS, max);
                }
                KeyCode::Left => {
                    state.selected = state.selected.saturating_sub(1);
                }
                KeyCode::Right => {
                    let max = state.items.len() - 1;
                    state.selected = std::cmp::min(state.selected + 1, max);
                }
                _ => {}
            }
        }
        Event::Mouse(mouse) => {
            trace_event(&format!(
                "MOUSE: {:?} col={} row={}",
                mouse.kind, mouse.column, mouse.row
            ));
            // Usamos Up (release) en vez de Down (press) porque en Windows
            // el primer Down puede ser consumido al recuperar el foco de la terminal
            // después de que una ventana secundaria se cierra.
            if let MouseEventKind::Up(MouseButton::Left) = mouse.kind {
                if let Some(idx) = card_at(mouse.column, mouse.row, state, ctx) {
                    trace_event(&format!("  -> click en card idx={}", idx));
                    state.selected = idx;
                    return Ok(MenuAction::Run(state.items[idx]));
                }
                trace_event("  -> click fuera de las cards");
            }

            // Actualizamos el hover con CUALQUIER actividad del mouse
            // (Moved, Drag, Down, Up, scroll). El movimiento libre sin botón
            // (Moved) no llega de forma fiable en consolas de Windows, pero el
            // Drag (botón presionado, incluye el click) sí, y con esto el
            // resaltado sigue al puntero en todos los escenarios.
            let h = card_at(mouse.column, mouse.row, state, ctx);
            trace_event(&format!("  -> hover idx={:?}", h));
            state.hover = h;
        }
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

        (MenuItem::AndroidDev, _) => "android-dev",

        (MenuItem::WindowsBuild, _) => "windows-build",

        (MenuItem::AndroidBuildArm64, ProjectType::Tauri) => "android-arm64",
        (MenuItem::AndroidBuildArm64, ProjectType::Flutter) => "android-arm64",
        (MenuItem::AndroidBuildArm64, ProjectType::Kotlin) => "android-arm64",

        (MenuItem::AndroidBuildArmv7, ProjectType::Tauri) => "android-armv7",

        (MenuItem::AndroidBuildAll, ProjectType::Tauri) => "android-all",
        (MenuItem::AndroidBuildAll, ProjectType::Flutter) => "android-all",
        (MenuItem::AndroidBuildAll, ProjectType::Kotlin) => "android-all",

        (MenuItem::InstallDebug, _) => "android-install-debug",


        (MenuItem::FlutterProfile, ProjectType::Flutter) => "flutter-profile",

        (MenuItem::Clean, _) => "clean",

        (MenuItem::DebugAndroid, _) => "debug-android",

        (MenuItem::Signing, _) => "signing",

        (MenuItem::Version, _) => "version",

        (MenuItem::Git, _) => "git",

        (MenuItem::BuildWhatsApp, _) => "whatsapp",
        (MenuItem::ConfigureTerminal, _) => "terminal",
        (MenuItem::ContextMenu, _) => "context-menu",

        _ => unreachable!(),
    }
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    if TRACE.load(Ordering::Relaxed) {
        let _ = std::fs::File::create(std::env::temp_dir().join("compass-tui.log"));
    }

    let mut terminal = setup_terminal()?;
    let mut state = MenuState::new(ctx.project_type, theme::load());

    trace_event(&format!(
        "TERMINAL SIZE: {:?} | project: {}",
        terminal.size()?,
        ctx.script_dir.display()
    ));

    let exe_path = std::env::current_exe()
        .context("No se pudo determinar la ruta del ejecutable")?;
    let project_path = &ctx.script_dir;

    loop {
        terminal.draw(|f| draw_menu(f, &mut state, ctx))?;

        match handle_event(&mut state, ctx)? {
            MenuAction::Quit => {
                let _ = restore_terminal(&mut terminal);
                std::process::exit(0);
            }
            MenuAction::Run(item) => {
                let task = menu_item_to_task(item, ctx.project_type);
                // En Windows el comando lo ejecuta `cmd /c` (ver
                // `spawn_new_window_autoexit`), donde las comillas son dobles y
                // `&` no es el call-operator de PowerShell. En Linux el script
                // corre en `sh`, donde las comillas simples sí valen y `&` sería
                // un error de sintaxis (background job), así que se omite.
                let command = if cfg!(windows) {
                    format!(
                        "\"{}\" --task {} --path \"{}\"",
                        exe_path.display(),
                        task,
                        project_path.display(),
                    )
                } else {
                    format!(
                        "'{}' --task {} --path '{}'",
                        exe_path.display(),
                        task,
                        project_path.display(),
                    )
                };

                cmd::spawn_new_window_autoexit(
                    &format!("Compass - {}", task),
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

    /// Renderiza una card en un buffer nuevo y devuelve `(fg, bg)` del centro.
    fn card_style(is_selected: bool, is_hovered: bool) -> (Option<Color>, Option<Color>) {
        let area = Rect::new(0, 0, CARD_W, CARD_H);
        let mut buf = Buffer::empty(area);
        create_card(&MenuItem::WindowsBuild, is_selected, is_hovered, ProjectType::Rust, crate::theme::DEFAULT)
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
        let normal = card_style(false, false);
        let hover = card_style(false, true);
        let selected = card_style(true, false);

        assert_ne!(normal, hover, "el hover debe verse distinto del estado normal");
        assert_ne!(
            hover, selected,
            "el hover debe verse distinto de la selección (no comparten el mismo fondo)"
        );
        assert_ne!(normal, selected);
    }

    #[test]
    fn selected_keeps_the_section_tint() {
        // `WindowsBuild` está en DEV ⇒ usa `dev_bg` del tema por defecto.
        let (_, bg) = card_style(true, false);
        assert_eq!(bg, Some(crate::theme::DEFAULT.selected_bg("DEV")));
    }

    /// El fondo seleccionado debe salir del tema activo, no de una tabla fija:
    /// con otro tema, la misma card cambia de color.
    #[test]
    fn selected_background_follows_the_active_theme() {
        let area = Rect::new(0, 0, CARD_W, CARD_H);
        let mut buf = Buffer::empty(area);
        let dracula = crate::theme::by_name("Dracula").unwrap();
        create_card(&MenuItem::WindowsBuild, true, false, ProjectType::Rust, dracula)
            .render(area, &mut buf);
        let bg = buf[(1, 1)].style().bg;
        assert_eq!(bg, Some(dracula.selected_bg("DEV")));
        assert_ne!(bg, Some(crate::theme::DEFAULT.selected_bg("DEV")));
    }

    #[test]
    fn normal_card_has_no_background() {
        let (_, bg) = card_style(false, false);
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
            card_style(true, true),
            card_style(true, false),
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
        assert_eq!(picker.index, crate::theme::index_of(crate::theme::DEFAULT.name));
        assert_eq!(picker.original.name, crate::theme::DEFAULT.name);

        // Vista previa: cambia el tema y el resaltado, sin cerrar el modal.
        let next = crate::theme::next(crate::theme::DEFAULT.name);
        state.preview_theme(next);
        assert!(state.picker.is_some(), "la vista previa no debe cerrar el modal");
        assert_eq!(state.theme.name, next.name);
        assert_eq!(state.picker.unwrap().index, crate::theme::index_of(next.name));
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
}
