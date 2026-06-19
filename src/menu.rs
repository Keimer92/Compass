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
use crate::utils::context::ProjectContext;

const COLS: usize = 3;
const CARD_W: u16 = 24;
const CARD_H: u16 = 5;
const GAP: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    WindowsDev,
    AndroidDev,
    WindowsBuild,
    AndroidBuildArm64,
    AndroidBuildAll,
    CleanCache,
    DebugAndroid,
    ConfigureSigning,
    CleanProject,
    CreateGitignore,
    InitGit,
    ManageKeystore,
    BuildWhatsApp,
    ConfigureTerminal,
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
            Self::Exit => "\u{1f6aa}",
        }
    }

    fn section(&self) -> &str {
        match self {
            Self::WindowsDev
            | Self::AndroidDev
            | Self::WindowsBuild
            | Self::AndroidBuildArm64
            | Self::AndroidBuildAll => "DEV",
            Self::CleanCache | Self::DebugAndroid | Self::ConfigureSigning | Self::CleanProject => {
                "MNT"
            }
            Self::CreateGitignore
            | Self::InitGit
            | Self::ManageKeystore
            | Self::BuildWhatsApp
            | Self::ConfigureTerminal => "CFG",
            Self::Exit => "",
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::WindowsDev => "Windows",
            Self::AndroidDev => "Android",
            Self::WindowsBuild => "Win Build",
            Self::AndroidBuildArm64 => "Android a64",
            Self::AndroidBuildAll => "Android All",
            Self::CleanCache => "Clean Cache",
            Self::DebugAndroid => "Debug Andr",
            Self::ConfigureSigning => "Configure",
            Self::CleanProject => "Clean Proj",
            Self::CreateGitignore => ".gitignore",
            Self::InitGit => "Git Init",
            Self::ManageKeystore => "Keystore",
            Self::BuildWhatsApp => "WhatsApp",
            Self::ConfigureTerminal => "Terminal",
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

    fn description(&self) -> &str {
        match self {
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
            Self::Exit => "Cierra programa",
        }
    }
}

struct MenuState {
    items: Vec<MenuItem>,
    selected: usize,
    card_areas: Vec<Rect>,
    exit_area: Rect,
}

impl MenuState {
    fn new() -> Self {
        Self {
            items: vec![
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
                MenuItem::Exit,
            ],
            selected: 0,
            card_areas: Vec::new(),
            exit_area: Rect::default(),
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

const GROUPS: &[(&str, Color, usize, usize)] = &[
    ("DESARROLLO",    Color::Cyan,    0, 5),
    ("MANTENIMIENTO", Color::Yellow,  5, 4),
    ("CONFIGURACION", Color::Magenta, 9, 5),
];

fn draw_menu(frame: &mut Frame, state: &mut MenuState, ctx: &ProjectContext) {
    let area = frame.area();
    let header_h = if ctx.android_home.is_none() { 5u16 } else { 4u16 };

    let total_h = calc_total_height(header_h);
    if area.height < total_h {
        return;
    }

    state.card_areas.clear();
    state.exit_area = Rect::default();

    let mut y = area.y + 1;

    let header_area = Rect { x: area.x, y, width: area.width, height: header_h };
    render_header(frame, header_area, ctx);
    y += header_h + 1;

    for (name, color, start, count) in GROUPS {
        let section_rect = Rect { x: area.x, y, width: area.width, height: 1 };
        render_section_header(frame, section_rect, name, *color);
        y += 1;

        let rows = (*count + COLS - 1) / COLS;
        let cards_h = rows as u16 * (CARD_H + GAP) - GAP;
        let cards_area = Rect { x: area.x, y, width: area.width, height: cards_h };
        render_group_cards(frame, cards_area, state, *start, *start + *count);
        y += cards_h + 1;
    }

    let exit_area = Rect { x: area.x, y, width: area.width, height: CARD_H };
    render_exit(frame, exit_area, state);
}

fn calc_total_height(header_h: u16) -> u16 {
    let mut h = header_h + 1;
    for (_name, _color, _start, count) in GROUPS {
        h += 1;
        let rows = (*count + COLS - 1) / COLS;
        h += rows as u16 * (CARD_H + GAP) - GAP;
        h += 1;
    }
    h += CARD_H;
    h
}

fn render_header(frame: &mut Frame, area: Rect, ctx: &ProjectContext) {
    let project_name = ctx
        .script_dir
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed("Proyecto"));

    let title_style = Style::default().fg(Color::Green).add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(Color::DarkGray);
    let value_style = Style::default().fg(Color::White);
    let warn_style = Style::default().fg(Color::Yellow);
    let hint_style = Style::default().fg(Color::Gray);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  \u{1f980}  Code Runner for Rust / Tauri  v", title_style),
            Span::styled("0.1.0", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("     Proyecto: ", label_style),
            Span::styled(project_name.to_string(), value_style),
        ]),
    ];

    if ctx.android_home.is_none() {
        lines.push(Line::from(vec![
            Span::styled("    \u{26a0}  ANDROID_HOME no configurada", warn_style),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(
            "    \u{2190}\u{2191}\u{2192}\u{2193} Navegar  |  Enter Seleccionar  |  Click Elegir  |  Esc Salir",
            hint_style,
        ),
    ]));

    let header = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(header, area);
}

fn render_section_header(frame: &mut Frame, area: Rect, name: &str, color: Color) {
    let line = Line::from(vec![
        Span::styled(
            format!("  {} ", name),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ]);
    let widget = Paragraph::new(Text::from(vec![line])).style(Style::default().bg(Color::Reset));
    frame.render_widget(widget, area);
}

fn render_group_cards(frame: &mut Frame, area: Rect, state: &mut MenuState, start: usize, end: usize) {
    let total_grid_w = COLS as u16 * (CARD_W + GAP) - GAP;
    let grid_start_x = area.x + (area.width.saturating_sub(total_grid_w)) / 2;

    for rel_idx in 0..(end - start) {
        let idx = start + rel_idx;
        let row = rel_idx / COLS;
        let col = rel_idx % COLS;

        let x = grid_start_x + col as u16 * (CARD_W + GAP);
        let y = area.y + row as u16 * (CARD_H + GAP);

        let rect = Rect { x, y, width: CARD_W, height: CARD_H };
        state.card_areas.push(rect);

        let is_selected = state.selected == idx;
        let card = create_card(&state.items[idx], is_selected);
        frame.render_widget(card, rect);
    }
}

fn render_exit(frame: &mut Frame, area: Rect, state: &mut MenuState) {
    let btn_w = 16u16;
    let btn_x = area.x + (area.width.saturating_sub(btn_w)) / 2;
    let btn_y = area.y;

    let rect = Rect {
        x: btn_x,
        y: btn_y,
        width: btn_w,
        height: CARD_H,
    };

    state.exit_area = rect;

    let is_selected = state.selected == state.items.len() - 1;
    let card = create_card(&MenuItem::Exit, is_selected);
    frame.render_widget(card, rect);
}

fn create_card(item: &MenuItem, is_selected: bool) -> Paragraph<'static> {
    let border_style = if is_selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let icon = item.icon();
    let section = item.section();
    let label = item.label();

    if section.is_empty() {
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled(
                format!("  {}", icon),
                Style::default().fg(if is_selected { Color::Cyan } else { Color::Red }),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", label),
                Style::default().fg(if is_selected { Color::Cyan } else { Color::White }),
            )]),
        ]))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        )
    } else {
        let section_color = item.section_color();
        let desc = item.description();
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled(
                format!(" {} [{}]", icon, section),
                Style::default().fg(section_color).add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", label),
                Style::default().fg(Color::White),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", desc),
                Style::default().fg(Color::DarkGray),
            )]),
        ]))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        )
    }
}

fn handle_event(state: &mut MenuState) -> anyhow::Result<Option<MenuItem>> {
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

                    for (idx, area) in state.card_areas.iter().enumerate() {
                        if col >= area.x
                            && col < area.x + area.width
                            && row >= area.y
                            && row < area.y + area.height
                        {
                            state.selected = idx;
                            return Ok(Some(state.items[idx]));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn run(ctx: &ProjectContext) -> anyhow::Result<()> {
    let mut terminal = setup_terminal()?;
    let mut state = MenuState::new();

    loop {
        terminal.draw(|f| draw_menu(f, &mut state, ctx))?;

        let selection = handle_event(&mut state)?;

        if let Some(item) = selection {
            if item == MenuItem::Exit {
                let _ = restore_terminal(&mut terminal);
                std::process::exit(0);
            }

            restore_terminal(&mut terminal)?;

            match item {
                MenuItem::WindowsDev => tasks::windows::dev(ctx)?,
                MenuItem::AndroidDev => tasks::android::dev(ctx)?,
                MenuItem::WindowsBuild => tasks::windows::build(ctx)?,
                MenuItem::AndroidBuildArm64 => tasks::android::build_arm64(ctx)?,
                MenuItem::AndroidBuildAll => tasks::android::build_all(ctx)?,
                MenuItem::CleanCache => tasks::cleanup::clean_cache(ctx)?,
                MenuItem::DebugAndroid => tasks::android::debug(ctx)?,
                MenuItem::ConfigureSigning => tasks::android::configure_signing(ctx)?,
                MenuItem::CleanProject => tasks::cleanup::clean_project(ctx)?,
                MenuItem::CreateGitignore => tasks::gitignore::create(ctx)?,
                MenuItem::InitGit => tasks::git::init(ctx)?,
                MenuItem::ManageKeystore => tasks::keystore::manage(ctx)?,
                MenuItem::BuildWhatsApp => tasks::whatsapp::build(ctx)?,
                MenuItem::ConfigureTerminal => tasks::terminal::configure(ctx)?,
                _ => unreachable!(),
            }

            terminal = setup_terminal()?;
        }
    }
}
