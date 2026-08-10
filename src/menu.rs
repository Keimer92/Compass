use std::io::{self, stdout};

use anyhow::Context;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame, Terminal,
};

use crate::utils::cmd;
use crate::utils::context::{ProjectContext, ProjectType};

const COLS: usize = 3;
const CARD_W: u16 = 24;
const CARD_H: u16 = 4;
const EXIT_H: u16 = 3;
const FOOTER_H: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    // DEV section
    WindowsDev,
    AndroidDev,
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildArmv7,
    AndroidBuildAll,
    LaunchEmulator,
    // MNT section
    CleanCache,
    DebugAndroid,
    ConfigureSigning,
    CleanProject,
    // CFG section
    CreateGitignore,
    InitGit,
    ManageKeystore,
    BuildWhatsApp,
    ConfigureTerminal,
    ContextMenu,
    Exit,
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
            Self::LaunchEmulator => "\u{1f4f1}",
            Self::CleanCache => "\u{1f9f9}",
            Self::DebugAndroid => "\u{1f50d}",
            Self::ConfigureSigning => "\u{1f511}",
            Self::CleanProject => "\u{1f9fc}",
            Self::CreateGitignore => "\u{1f4c4}",
            Self::InitGit => "\u{1f527}",
            Self::ManageKeystore => "\u{1f5dd}",
            Self::BuildWhatsApp => "\u{1f4ac}",
            Self::ConfigureTerminal => "\u{2699}",
            Self::ContextMenu => "\u{1f4c1}",
            Self::Exit => "\u{1f6aa}",
        }
    }

    fn section(&self) -> &str {
        match self {
            Self::WindowsDev | Self::AndroidDev | Self::WindowsBuild | Self::AndroidBuildArm64 | Self::AndroidBuildArmv7 | Self::AndroidBuildAll | Self::LaunchEmulator => "DEV",
            Self::CleanCache | Self::DebugAndroid | Self::ConfigureSigning | Self::CleanProject => "MNT",
            Self::CreateGitignore | Self::InitGit | Self::ManageKeystore | Self::BuildWhatsApp | Self::ConfigureTerminal | Self::ContextMenu => "CFG",
            Self::Exit => "",
        }
    }

    fn label(&self, project_type: ProjectType) -> &str {
        match self {
            Self::WindowsDev => match project_type {
                ProjectType::Rust => "Run",
                _ => "Windows",
            },
            Self::AndroidDev => "Android",
            Self::WindowsBuild => match project_type {
                ProjectType::Rust => "Build",
                _ => "Win Build",
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
            Self::LaunchEmulator => "Emulator",
            Self::CleanCache => "Clean Cache",
            Self::DebugAndroid => "Debug Andr",
            Self::ConfigureSigning => "Configure",
            Self::CleanProject => "Clean Proj",
            Self::CreateGitignore => ".gitignore",
            Self::InitGit => "Git Init",
            Self::ManageKeystore => "Keystore",
            Self::BuildWhatsApp => "WhatsApp",
            Self::ConfigureTerminal => "Terminal",
            Self::ContextMenu => "Menu Ctx",
            Self::Exit => "Salir",
        }
    }

    fn section_color(&self) -> Color {
        match self.section() {
            "DEV" => Color::Cyan,
            "MNT" => Color::Yellow,
            "CFG" => Color::Magenta,
            _ => Color::White,
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
                Self::CleanCache => "Limpia target/ y cache",
                Self::DebugAndroid => "",
                Self::ConfigureSigning => "",
                Self::CleanProject => "cargo clean",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
                Self::ManageKeystore => "",
                Self::LaunchEmulator => "",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::ConfigureTerminal => "Autocompletado",
                Self::ContextMenu => "Menu contextual",
                Self::Exit => "Cierra programa",
            },
            ProjectType::Tauri => match self {
                Self::WindowsDev => "Inicia el dev server",
                Self::AndroidDev => "Dev con target Android",
                Self::WindowsBuild => "Compila produccion",
                Self::AndroidBuildArm64 => "Compila APK arm64",
                Self::AndroidBuildArmv7 => "Compila APK armv7",
                Self::AndroidBuildAll => "Compila todas las APK",
                Self::CleanCache => "Limpia cache y npm",
                Self::DebugAndroid => "Chrome DevTools",
                Self::ConfigureSigning => "Configura firma",
                Self::CleanProject => "Limpia proyecto",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
                Self::ManageKeystore => "Administra llaves",
                Self::LaunchEmulator => "Lanza emulador",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::ConfigureTerminal => "Autocompletado",
                Self::ContextMenu => "Menu contextual",
                Self::Exit => "Cierra programa",
            },
            ProjectType::Flutter => match self {
                Self::WindowsDev => "flutter run windows",
                Self::AndroidDev => "flutter run android",
                Self::WindowsBuild => "flutter build windows",
                Self::AndroidBuildArm64 => "flutter build apk",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "flutter build appbundle",
                Self::CleanCache => "Limpia cache Flutter",
                Self::DebugAndroid => "Depurar Android",
                Self::ConfigureSigning => "Configurar firma",
                Self::CleanProject => "Limpia proyecto",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
                Self::ManageKeystore => "Administrar firmas",
                Self::LaunchEmulator => "Lanza emulador",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::ConfigureTerminal => "Autocompletado",
                Self::ContextMenu => "Menu contextual",
                Self::Exit => "Cierra programa",
            },
            ProjectType::Kotlin => match self {
                Self::WindowsDev => "gradlew :composeApp:run",
                Self::AndroidDev => "gradlew :installDebug",
                Self::WindowsBuild => "gradlew :packageDist",
                Self::AndroidBuildArm64 => "gradlew :assembleDebug",
                Self::AndroidBuildArmv7 => "",
                Self::AndroidBuildAll => "gradlew :bundleRelease",
                Self::CleanCache => "Limpia Gradle/Kotlin",
                Self::DebugAndroid => "Depurar Android",
                Self::ConfigureSigning => "Configurar firma",
                Self::CleanProject => "Limpia proyecto",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
                Self::LaunchEmulator => "Lanza emulador",
                Self::ManageKeystore => "Administrar firmas",
                Self::BuildWhatsApp => "Compila WhatsApp",
                Self::ConfigureTerminal => "Autocompletado",
                Self::ContextMenu => "Menu contextual",
                Self::Exit => "Cierra programa",
            },
        }
    }
}

struct MenuState {
    items: Vec<MenuItem>,
    selected: usize,
    groups: Vec<(String, Color, Vec<MenuItem>)>,
    exit_area: Rect,
    area_width: u16,
    header_height: u16,
}

impl MenuState {
    fn new(project_type: ProjectType) -> Self {
        let base = vec![
            MenuItem::WindowsDev,
            MenuItem::AndroidDev,
            MenuItem::WindowsBuild,
            MenuItem::AndroidBuildArm64,
            MenuItem::AndroidBuildArmv7,
            MenuItem::AndroidBuildAll,
            MenuItem::LaunchEmulator,
            MenuItem::CleanCache,
            MenuItem::DebugAndroid,
            MenuItem::ConfigureSigning,
            MenuItem::CleanProject,
            MenuItem::CreateGitignore,
            MenuItem::InitGit,
            MenuItem::ManageKeystore,
            MenuItem::BuildWhatsApp,
            MenuItem::ConfigureTerminal,
            MenuItem::ContextMenu,
            MenuItem::Exit,
        ];

        let items: Vec<MenuItem> = base.into_iter()
            .filter(|item| match project_type {
                ProjectType::Rust => !matches!(item,
                    MenuItem::AndroidDev
                    | MenuItem::AndroidBuildArm64
                    | MenuItem::AndroidBuildArmv7
                    | MenuItem::AndroidBuildAll
                    | MenuItem::LaunchEmulator
                    | MenuItem::DebugAndroid
                    | MenuItem::ConfigureSigning
                    | MenuItem::ManageKeystore
                ),
                ProjectType::Tauri => true,
                _ => !matches!(item, MenuItem::AndroidBuildArmv7),
            })
            .collect();

        Self {
            items,
            selected: 0,
            groups: Vec::new(),
            exit_area: Rect::default(),
            area_width: 0,
            header_height: 2,
        }
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

fn calc_group_sizes(items: &[MenuItem]) -> Vec<(String, Color, Vec<MenuItem>)> {
    let mut dev = Vec::new();
    let mut mnt = Vec::new();
    let mut cfg = Vec::new();

    for item in items {
        if *item == MenuItem::Exit { continue; }
        match item.section() {
            "DEV" => dev.push(*item),
            "MNT" => mnt.push(*item),
            _ => cfg.push(*item),
        }
    }

    let mut groups = Vec::new();
    if !dev.is_empty() { groups.push(("DESARROLLO".to_string(), Color::Cyan, dev)); }
    if !mnt.is_empty() { groups.push(("MANTENIMIENTO".to_string(), Color::Yellow, mnt)); }
    if !cfg.is_empty() { groups.push(("CONFIGURACION".to_string(), Color::Magenta, cfg)); }
    groups
}

fn calc_total_height(ctx: &ProjectContext, items: &[MenuItem]) -> u16 {
    let warn = ctx.android_home.is_none();
    let header_h: u16 = if warn { 3 } else { 2 };
    let groups = calc_group_sizes(items);
    let sec_h: u16 = groups.len() as u16;
    let cards_h: u16 = groups.iter().map(|(_, _, items)| {
        let rows = (items.len() + COLS - 1) / COLS;
        rows as u16 * CARD_H
    }).sum();
    header_h + sec_h + cards_h + EXIT_H + FOOTER_H
}

fn draw_menu(frame: &mut Frame, state: &mut MenuState, ctx: &ProjectContext) {
    let area = frame.area();
    let total_h = calc_total_height(ctx, &state.items);

    if area.height < total_h {
        let msg = Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled(
                " Terminal demasiado pequena.",
                Style::default().fg(Color::Yellow),
            )]),
            Line::from(vec![Span::styled(
                format!(" Necesita al menos {} lineas.", total_h),
                Style::default().fg(Color::DarkGray),
            )]),
            Line::from(vec![Span::styled(
                " Maximiza la ventana (Alt+Enter) y presiona una tecla.",
                Style::default().fg(Color::DarkGray),
            )]),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    state.groups.clear();
    state.exit_area = Rect::default();

    let mut y = area.y;
    let header_h: u16 = if ctx.android_home.is_none() { 3 } else { 2 };

    render_header(frame, Rect { x: area.x, y, width: area.width, height: header_h }, ctx);
    y += header_h;

    state.header_height = header_h;
    state.area_width = area.width;

    let groups = calc_group_sizes(&state.items);
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
        render_group_cards(frame, Rect { x: area.x, y, width: area.width, height: cards_h }, state, items, start_idx, ctx.project_type);
        y += cards_h;
    }

    render_exit(frame, Rect { x: area.x, y, width: area.width, height: EXIT_H }, state);

    render_footer(
        frame,
        Rect {
            x: area.x,
            y: area.y + area.height - FOOTER_H,
            width: area.width,
            height: FOOTER_H,
        },
    );
}

fn render_header(frame: &mut Frame, area: Rect, ctx: &ProjectContext) {
    let project_name = ctx
        .script_dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed("Proyecto"));

    let (emoji, label, badge) = match ctx.project_type {
        ProjectType::Tauri => ("\u{1f980}", "Code Runner for Rust / Tauri", "TAURI"),
        ProjectType::Flutter => ("\u{1f3af}", "Code Runner for Dart / Flutter", "FLUTTER"),
        ProjectType::Kotlin => ("\u{1f537}", "Code Runner for Kotlin / KMP", "KOTLIN"),
        ProjectType::Rust => ("\u{2699}", "Code Runner for Rust", "RUST"),
    };
    let badge_color = match ctx.project_type {
        ProjectType::Tauri => Color::Green,
        ProjectType::Flutter => Color::Cyan,
        ProjectType::Kotlin => Color::Magenta,
        ProjectType::Rust => Color::Yellow,
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("  {}  {}  v", emoji, label), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("0.1.0", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("     Proyecto: ", Style::default().fg(Color::DarkGray)),
            Span::styled(project_name.to_string(), Style::default().fg(Color::White)),
            Span::styled("  ", Style::default()),
            Span::styled(
                format!(" {} ", badge),
                Style::default().bg(badge_color).fg(Color::Black).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if ctx.android_home.is_none() {
        lines.push(Line::from(vec![
            Span::styled("    \u{26a0} ANDROID_HOME no configurada", Style::default().fg(Color::Yellow)),
        ]));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_group_cards(frame: &mut Frame, area: Rect, state: &MenuState, items: &[MenuItem], start_global_idx: usize, project_type: ProjectType) {
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
        let card = create_card(&items[rel_idx], is_selected, project_type);
        frame.render_widget(card, rect);
    }
}

fn render_exit(frame: &mut Frame, area: Rect, state: &mut MenuState) {
    let btn_w = 20u16;
    let btn_x = area.x + (area.width.saturating_sub(btn_w)) / 2;
    let btn_y = area.y;

    let rect = Rect { x: btn_x, y: btn_y, width: btn_w, height: EXIT_H };
    state.exit_area = rect;

    let is_selected = state.selected == state.items.len() - 1;
    let border = if is_selected {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let fg = if is_selected { Color::Red } else { Color::DarkGray };

    let card = Paragraph::new(Text::from(vec![Line::from(vec![
        Span::styled("\u{1f6aa}  Salir", Style::default().fg(fg).add_modifier(Modifier::BOLD)),
    ])]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border),
    );
    frame.render_widget(card, rect);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("  \u{2191}\u{2193}\u{2190}\u{2192}", Style::default().fg(Color::White)),
        Span::styled(" navegar   ", Style::default().fg(Color::DarkGray)),
        Span::styled("Enter", Style::default().fg(Color::White)),
        Span::styled(" ejecutar   ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::White)),
        Span::styled(" salir      ", Style::default().fg(Color::DarkGray)),
        Span::styled("\u{25cf} DEV ", Style::default().fg(Color::Cyan)),
        Span::styled("\u{25cf} MNT ", Style::default().fg(Color::Yellow)),
        Span::styled("\u{25cf} CFG", Style::default().fg(Color::Magenta)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// Tinte translucido para simular vidrio esmerilado: el color de la seccion
/// oscurecido y desaturado hacia el negro del fondo de la terminal.
fn glass_tint(color: Color) -> Color {
    match color {
        Color::Cyan => Color::Rgb(16, 38, 50),
        Color::Yellow => Color::Rgb(52, 44, 16),
        Color::Magenta => Color::Rgb(46, 20, 56),
        _ => Color::Rgb(30, 30, 34),
    }
}

fn create_card(item: &MenuItem, is_selected: bool, project_type: ProjectType) -> Paragraph<'static> {
    let section = item.section();
    let section_color = item.section_color();

    let border = if is_selected {
        Style::default().fg(section_color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let icon = item.icon();
    let label = item.label(project_type);
    let desc = item.description(project_type);

    let line1 = if section.is_empty() {
        format!(" {}  {}", icon, label)
    } else {
        format!(" {} [{}] {}", icon, section, label)
    };

    let mut line1_style = Style::default().fg(section_color);
    if is_selected {
        line1_style = line1_style.add_modifier(Modifier::BOLD);
    }

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border);
    if is_selected {
        block = block.style(Style::default().bg(glass_tint(section_color)));
    }

    Paragraph::new(Text::from(vec![
        Line::from(vec![Span::styled(line1, line1_style)]),
        Line::from(vec![Span::styled(format!(" {}", desc), Style::default().fg(Color::DarkGray))]),
    ]))
    .alignment(Alignment::Center)
    .block(block)
}

fn handle_event(state: &mut MenuState, ctx: &ProjectContext) -> anyhow::Result<Option<MenuItem>> {
    loop {
        let evt = crossterm::event::read()?;

        match evt {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => return Ok(Some(MenuItem::Exit)),
                KeyCode::Enter => return Ok(Some(state.items[state.selected])),
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
            },
            Event::Mouse(mouse) => {
                // Usamos Up (release) en vez de Down (press) porque en Windows
                // el primer Down puede ser consumido al recuperar el foco de la terminal
                // después de que una ventana secundaria se cierra.
                if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                    let col = mouse.column;
                    let row = mouse.row;

                    if col >= state.exit_area.x
                        && col < state.exit_area.x + state.exit_area.width
                        && row >= state.exit_area.y
                        && row < state.exit_area.y + state.exit_area.height
                    {
                        return Ok(Some(MenuItem::Exit));
                    }

                    let header_h: u16 = if ctx.android_home.is_none() { 3 } else { 2 };
                    let mut y_cursor = header_h;

                    let groups = calc_group_sizes(&state.items);
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
                                state.selected = global_idx;
                                return Ok(Some(state.items[global_idx]));
                            }
                        }

                        y_cursor += cards_h;
                    }
                }
            }
            _ => {}
        }
    }
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

        (MenuItem::LaunchEmulator, _) => "launch-emulator",

        (MenuItem::CleanCache, _) => "clean-cache",

        (MenuItem::DebugAndroid, _) => "debug-android",

        (MenuItem::ConfigureSigning, _) => "sign",

        (MenuItem::CleanProject, _) => "clean-project",

        (MenuItem::CreateGitignore, _) => "gitignore",
        (MenuItem::InitGit, _) => "git-init",

        (MenuItem::ManageKeystore, _) => "keystore",

        (MenuItem::BuildWhatsApp, _) => "whatsapp",
        (MenuItem::ConfigureTerminal, _) => "terminal",
        (MenuItem::ContextMenu, _) => "context-menu",

        _ => unreachable!(),
    }
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    let mut terminal = setup_terminal()?;
    let mut state = MenuState::new(ctx.project_type);

    let exe_path = std::env::current_exe()
        .context("No se pudo determinar la ruta del ejecutable")?;
    let project_path = &ctx.script_dir;

    loop {
        terminal.draw(|f| draw_menu(f, &mut state, ctx))?;

        let selection = handle_event(&mut state, ctx)?;

        if let Some(item) = selection {
            if item == MenuItem::Exit {
                let _ = restore_terminal(&mut terminal);
                std::process::exit(0);
            }

            let task = menu_item_to_task(item, ctx.project_type);
            let command = format!(
                "& '{}' --task {} --path '{}'",
                exe_path.display(),
                task,
                project_path.display(),
            );

            cmd::spawn_new_window_autoexit(
                &format!("Code Runner - {}", task),
                project_path,
                &command,
            )?;
        }
    }
}


