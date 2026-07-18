use std::io::{self, stdout};

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
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

use crate::tasks;
use crate::utils::{cmd, context::{ProjectContext, ProjectType}};

const COLS: usize = 3;
const CARD_W: u16 = 24;
const CARD_H: u16 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    // DEV section
    WindowsDev,
    AndroidDev,
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildAll,
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
            Self::AndroidBuildAll => "\u{1f680}",
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
            Self::WindowsDev | Self::AndroidDev | Self::WindowsBuild | Self::AndroidBuildArm64 | Self::AndroidBuildAll => "DEV",
            Self::CleanCache | Self::DebugAndroid | Self::ConfigureSigning | Self::CleanProject => "MNT",
            Self::CreateGitignore | Self::InitGit | Self::ManageKeystore | Self::BuildWhatsApp | Self::ConfigureTerminal | Self::ContextMenu => "CFG",
            Self::Exit => "",
        }
    }

    fn label(&self, project_type: ProjectType) -> &str {
        match self {
            Self::WindowsDev => "Windows",
            Self::AndroidDev => "Android",
            Self::WindowsBuild => "Win Build",
            Self::AndroidBuildArm64 => match project_type {
                ProjectType::Tauri => "Android a64",
                ProjectType::Flutter => "APK",
                ProjectType::Kotlin => "APK",
            },
            Self::AndroidBuildAll => match project_type {
                ProjectType::Tauri => "Android All",
                ProjectType::Flutter => "AAB",
                ProjectType::Kotlin => "AAB",
            },
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
            ProjectType::Tauri => match self {
                Self::WindowsDev => "Inicia el dev server",
                Self::AndroidDev => "Dev con target Android",
                Self::WindowsBuild => "Compila produccion",
                Self::AndroidBuildArm64 => "Compila APK arm64",
                Self::AndroidBuildAll => "Compila todas las APK",
                Self::CleanCache => "Limpia cache y npm",
                Self::DebugAndroid => "Chrome DevTools",
                Self::ConfigureSigning => "Configura firma",
                Self::CleanProject => "Limpia proyecto",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
                Self::ManageKeystore => "Administra llaves",
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
                Self::AndroidBuildAll => "flutter build appbundle",
                Self::CleanCache => "Limpia cache Flutter",
                Self::DebugAndroid => "Depurar Android",
                Self::ConfigureSigning => "Configurar firma",
                Self::CleanProject => "Limpia proyecto",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
                Self::ManageKeystore => "Administrar firmas",
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
                Self::AndroidBuildAll => "gradlew :bundleRelease",
                Self::CleanCache => "Limpia Gradle/Kotlin",
                Self::DebugAndroid => "Depurar Android",
                Self::ConfigureSigning => "Configurar firma",
                Self::CleanProject => "Limpia proyecto",
                Self::CreateGitignore => "Crea .gitignore",
                Self::InitGit => "Git init + commit",
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
            MenuItem::AndroidBuildAll,
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
            .filter(|item| matches!((item, project_type), _))
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
    header_h + sec_h + cards_h + CARD_H
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

    render_exit(frame, Rect { x: area.x, y, width: area.width, height: CARD_H }, state);
}

fn render_header(frame: &mut Frame, area: Rect, ctx: &ProjectContext) {
    let project_name = ctx
        .script_dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed("Proyecto"));

    let (emoji, label) = match ctx.project_type {
        ProjectType::Tauri => ("\u{1f980}", "Code Runner for Rust / Tauri"),
        ProjectType::Flutter => ("\u{1f3af}", "Code Runner for Dart / Flutter"),
        ProjectType::Kotlin => ("\u{1f537}", "Code Runner for Kotlin / KMP"),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("  {}  {}  v", emoji, label), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("0.1.0", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("     Proyecto: ", Style::default().fg(Color::DarkGray)),
            Span::styled(project_name.to_string(), Style::default().fg(Color::White)),
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
    let btn_w = 16u16;
    let btn_x = area.x + (area.width.saturating_sub(btn_w)) / 2;
    let btn_y = area.y;

    let rect = Rect { x: btn_x, y: btn_y, width: btn_w, height: CARD_H };
    state.exit_area = rect;

    let is_selected = state.selected == state.items.len() - 1;
    let card = create_card(&MenuItem::Exit, is_selected, ProjectType::Tauri);
    frame.render_widget(card, rect);
}

fn create_card(item: &MenuItem, is_selected: bool, project_type: ProjectType) -> Paragraph<'static> {
    let border = if is_selected {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let icon = item.icon();
    let section = item.section();
    let label = item.label(project_type);
    let desc = item.description(project_type);
    let fg = if is_selected { Color::Cyan } else { item.section_color() };

    let line1 = if section.is_empty() {
        format!(" {}  {}", icon, label)
    } else {
        format!(" {} [{}] {}", icon, section, label)
    };

    Paragraph::new(Text::from(vec![
        Line::from(vec![Span::styled(line1, Style::default().fg(fg))]),
        Line::from(vec![Span::styled(format!(" {}", desc), Style::default().fg(Color::DarkGray))]),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).border_style(border))
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
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
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

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    let mut terminal = setup_terminal()?;
    let mut state = MenuState::new(ctx.project_type);

    loop {
        terminal.draw(|f| draw_menu(f, &mut state, ctx))?;

        let selection = handle_event(&mut state, ctx)?;

        if let Some(item) = selection {
            if item == MenuItem::Exit {
                let _ = restore_terminal(&mut terminal);
                std::process::exit(0);
            }

            restore_terminal(&mut terminal)?;
            cmd::clear_screen();

            match (item, ctx.project_type) {
                (MenuItem::WindowsDev, ProjectType::Tauri) => tasks::windows::dev(ctx)?,
                (MenuItem::WindowsDev, ProjectType::Flutter) => tasks::flutter::run_windows(ctx)?,
                (MenuItem::WindowsDev, ProjectType::Kotlin) => tasks::kotlin::run_windows(ctx)?,

                (MenuItem::AndroidDev, ProjectType::Tauri) => tasks::android::dev(ctx)?,
                (MenuItem::AndroidDev, ProjectType::Flutter) => tasks::flutter::run_android(ctx)?,
                (MenuItem::AndroidDev, ProjectType::Kotlin) => tasks::kotlin::run_android(ctx)?,

                (MenuItem::WindowsBuild, ProjectType::Tauri) => tasks::windows::build(ctx)?,
                (MenuItem::WindowsBuild, ProjectType::Flutter) => tasks::flutter::build_windows(ctx)?,
                (MenuItem::WindowsBuild, ProjectType::Kotlin) => tasks::kotlin::build_windows(ctx)?,

                (MenuItem::AndroidBuildArm64, ProjectType::Tauri) => tasks::android::build_arm64(ctx)?,
                (MenuItem::AndroidBuildArm64, ProjectType::Flutter) => tasks::flutter::build_apk(ctx)?,
                (MenuItem::AndroidBuildArm64, ProjectType::Kotlin) => tasks::kotlin::build_apk(ctx)?,

                (MenuItem::AndroidBuildAll, ProjectType::Tauri) => tasks::android::build_all(ctx)?,
                (MenuItem::AndroidBuildAll, ProjectType::Flutter) => tasks::flutter::build_aab(ctx)?,
                (MenuItem::AndroidBuildAll, ProjectType::Kotlin) => tasks::kotlin::build_aab(ctx)?,

                (MenuItem::CleanCache, _) => tasks::cleanup::clean_cache(ctx)?,

                (MenuItem::DebugAndroid, ProjectType::Tauri) => tasks::android::debug(ctx)?,
                (MenuItem::DebugAndroid, ProjectType::Flutter) => {
                    cmd::clear_screen();
                    println!("╔══════════════════════════════════════════════════╗");
                    println!("║         DEPURAR ANDROID (FLUTTER)                ║");
                    println!("╚══════════════════════════════════════════════════╝");
                    println!("[INFO] Ejecuta en otra terminal:");
                    println!("  flutter run --debug");
                    pause();
                }
                (MenuItem::DebugAndroid, ProjectType::Kotlin) => {
                    cmd::clear_screen();
                    println!("╔══════════════════════════════════════════════════╗");
                    println!("║         DEPURAR ANDROID (KMP)                   ║");
                    println!("╚══════════════════════════════════════════════════╝");
                    println!("[INFO] Ejecuta en otra terminal:");
                    println!("  ./gradlew :composeApp:installDebug");
                    pause();
                }

                (MenuItem::ConfigureSigning, _) => tasks::android::configure_signing(ctx)?,

                (MenuItem::CleanProject, ProjectType::Tauri) => tasks::cleanup::clean_project(ctx)?,
                (MenuItem::CleanProject, ProjectType::Flutter) => tasks::flutter::clean(ctx)?,
                (MenuItem::CleanProject, ProjectType::Kotlin) => tasks::kotlin::clean(ctx)?,

                (MenuItem::CreateGitignore, _) => tasks::gitignore::create(ctx)?,
                (MenuItem::InitGit, _) => tasks::git::init(ctx)?,

                (MenuItem::ManageKeystore, ProjectType::Tauri) => tasks::keystore::manage(ctx)?,
                (MenuItem::ManageKeystore, ProjectType::Flutter) => tasks::android::configure_signing(ctx)?,
                (MenuItem::ManageKeystore, ProjectType::Kotlin) => tasks::android::configure_signing(ctx)?,

                (MenuItem::BuildWhatsApp, _) => tasks::whatsapp::build(ctx)?,
                (MenuItem::ConfigureTerminal, _) => tasks::terminal::configure(ctx)?,
                (MenuItem::ContextMenu, _) => tasks::context_menu::toggle()?,

                _ => unreachable!(),
            }

            terminal = setup_terminal()?;
        }
    }
}

fn pause() {
    use std::io::{self, Write};
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
