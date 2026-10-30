//! Submenús de tareas (Firmas y Git) con el **mismo aspecto y comportamiento
//! que el menú principal**: cabecera con la versión real de Compass, fila de
//! sección al estilo de las pestañas, rejilla adaptativa de cards de tres
//! líneas con acento propio y halo de hover, scroll por filas y pie con
//! atajos + ruta completa.
//!
//! Los submenús viven en una pestaña del contenedor (ConPTY), así que dibujan
//! a pantalla completa de su propio terminal y reciben el ratón ya reenviado
//! por [`crate::container`] con las filas relativas al cuerpo.
//!
//! Cada módulo dueño (signer/git) sólo aporta su lista de entradas y ejecuta la
//! acción elegida cuando [`SubmenuAction::Run`] llega.

use std::time::Instant;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Tabs},
    Frame,
};

use crate::menu::{
    card_widget, grid_card_w, grid_cell_w, grid_index_at, layout, render_header, render_hover_glow,
    render_scrollbar, show_android_warning, visible_rows, UiLayout, ANIM_FRAME, CARD_H, FOOTER_H,
    IDLE_POLL, ROW_H, TABS_H,
};
use crate::theme::Theme;
use crate::utils::context::ProjectContext;

/// Una entrada del submenú: lo que la card necesita para pintarse.
pub(crate) struct SubmenuItem {
    pub icon: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Clave de `theme::CARD_KEYS` que da el acento de esta card.
    pub color_key: &'static str,
}

/// Resultado de un evento del submenú.
pub(crate) enum SubmenuAction<T> {
    Run(T),
    /// `Esc`: volver al menú principal (el llamador termina su `manage`).
    Back,
    Continue,
}

/// Estado del navegador de cards de un submenú.
pub(crate) struct Submenu<T: Copy> {
    /// Etiqueta de la fila de sección, p. ej. `FIRMAS · KEYSTORE`.
    title: &'static str,
    /// Sección del tema (`"MNT"` | `"CFG"`) que colorea esa etiqueta, igual
    /// que las pestañas del menú principal.
    section: &'static str,
    entries: Vec<(T, SubmenuItem)>,
    selected: usize,
    hover: Option<usize>,
    hover_started: Option<Instant>,
    /// Primera fila visible de la rejilla. Se desplaza de fila completa.
    scroll: u16,
    /// Alto del viewport. Lo fija `draw` en cada frame.
    vh: u16,
    /// Cards por fila. Lo fija `draw` en cada frame.
    cols: usize,
    /// Última geometría dibujada; `None` si la terminal no admite ni una card.
    ui: Option<UiLayout>,
}

impl<T: Copy> Submenu<T> {
    pub fn new(title: &'static str, section: &'static str, entries: Vec<(T, SubmenuItem)>) -> Self {
        Self {
            title,
            section,
            entries,
            selected: 0,
            hover: None,
            hover_started: None,
            scroll: 0,
            vh: 0,
            cols: 1,
            ui: None,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn item(&self, idx: usize) -> &SubmenuItem {
        &self.entries[idx].1
    }

    /// Actualiza la card bajo el puntero. El halo arranca al entrar en una card
    /// nueva: moverse dentro de la misma no lo reinicia.
    fn set_hover(&mut self, hover: Option<usize>) {
        if self.hover == hover {
            return;
        }
        self.hover = hover;
        self.hover_started = hover.map(|_| Instant::now());
    }

    fn hover_phase(&self) -> Option<f32> {
        let started = self.hover_started?;
        let phase = started.elapsed().as_secs_f32() / crate::menu::HOVER_CYCLE.as_secs_f32();
        Some(phase.fract())
    }

    fn hover_animating(&self) -> bool {
        self.hover.is_some() && self.hover_started.is_some()
    }

    /// Ajusta `scroll` (en filas) para que la card seleccionada quede entera
    /// dentro del viewport. Las filas se desplazan completas.
    fn ensure_visible(&mut self) {
        let n = self.len();
        if n == 0 || self.vh < CARD_H {
            self.scroll = 0;
            return;
        }
        let cols = self.cols.max(1);
        let rows = visible_rows(self.vh).max(1);
        let row = self.selected.min(n - 1) / cols;
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

    /// Paso horizontal: una card dentro de la fila.
    fn move_selection(&mut self, delta: isize) {
        let max = self.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, max) as usize;
        self.ensure_visible();
    }

    /// Paso vertical: una fila completa de la rejilla.
    fn move_row(&mut self, delta: isize) {
        let step = self.cols.max(1) as isize;
        let max = self.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta * step).clamp(0, max) as usize;
        self.ensure_visible();
    }

    fn card_at(&self, col: u16, row: u16) -> Option<usize> {
        let ui = self.ui?;
        grid_index_at(ui, self.scroll, self.len(), col, row)
    }

    /// Dibuja cabecera, sección, rejilla con halo, scroll y pie.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, ctx: &ProjectContext, theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.background)),
            area,
        );

        let ui = layout(area, show_android_warning(ctx));
        self.vh = ui.list.height;
        self.cols = ui.cols;

        if ui.list.height < CARD_H {
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
            self.ui = None;
            return;
        }

        render_header(frame, ui.header, ctx, theme);
        self.render_section_title(frame, ui.tabs, theme);

        self.ensure_visible();
        self.render_cards(frame, ui, theme);
        render_scrollbar(frame, ui, self.len(), self.scroll, theme);
        render_footer(frame, ui.footer, ctx, theme);

        self.ui = Some(ui);
    }

    /// Fila de sección con la misma forma que las pestañas del menú principal:
    /// `" <título> <n> "` en el acento de su sección.
    fn render_section_title(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let title = format!(" {} {} ", self.title, self.len());
        let tabs = Tabs::new(vec![Line::from(title)])
            .select(0)
            .style(Style::default().fg(theme.muted))
            .highlight_style(
                Style::default()
                    .fg(theme.section(self.section))
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(tabs, area);
    }

    fn render_cards(&self, frame: &mut Frame, ui: UiLayout, theme: &Theme) {
        let n = self.len();
        let cols = ui.cols;
        let cell_w = grid_cell_w(ui.list.width, cols);
        let card_w = grid_card_w(ui.list.width, cols);
        let list_y = ui.list.y as i32;
        let list_bottom = ui.list.bottom() as i32;
        let total_rows = (n + cols - 1) / cols;

        for row in self.scroll as usize..total_rows {
            let y = list_y + ((row - self.scroll as usize) * ROW_H as usize) as i32;
            // Filas enteras o nada: sólo la última podría sobrar.
            if y + CARD_H as i32 > list_bottom {
                break;
            }
            for col in 0..cols {
                let idx = row * cols + col;
                if idx >= n {
                    break;
                }
                let rect = Rect {
                    x: ui.list.x + col as u16 * cell_w,
                    y: y as u16,
                    width: card_w,
                    height: CARD_H,
                };
                let item = self.item(idx);
                let card = card_widget(
                    item.icon,
                    item.label,
                    item.description,
                    item.color_key,
                    self.selected == idx,
                    self.hover == Some(idx),
                    theme,
                    card_w,
                );
                frame.render_widget(card, rect);

                if self.hover == Some(idx) {
                    if let Some(phase) = self.hover_phase() {
                        render_hover_glow(
                            frame,
                            rect,
                            phase,
                            theme.card_accent(item.color_key),
                            theme.card_bg(item.color_key),
                            theme.text,
                        );
                    }
                }
            }
        }
    }

    /// Espera un evento con el mismo ritmo que el menú: ~30 fps mientras hay
    /// halo y un segundo en reposo.
    pub fn handle_event(&mut self) -> Result<SubmenuAction<T>> {
        let timeout = if self.hover_animating() {
            ANIM_FRAME
        } else {
            IDLE_POLL
        };
        if !crossterm::event::poll(timeout)? {
            return Ok(SubmenuAction::Continue);
        }
        let evt = crossterm::event::read()?;
        Ok(self.handle_input(evt))
    }

    fn handle_input(&mut self, evt: Event) -> SubmenuAction<T> {
        match evt {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.set_hover(None);
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return SubmenuAction::Back,
                    KeyCode::Enter => return SubmenuAction::Run(self.entries[self.selected].0),
                    // Misma geometría que el menú principal: verticales de fila
                    // en fila y horizontales dentro de la fila.
                    KeyCode::Up | KeyCode::Char('k') => self.move_row(-1),
                    KeyCode::Down | KeyCode::Char('j') => self.move_row(1),
                    KeyCode::Left | KeyCode::Char('h') => self.move_selection(-1),
                    KeyCode::Right | KeyCode::Char('l') => self.move_selection(1),
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                // La rueda mueve la selección una fila: lo resaltado y la
                // posición de la lista nunca se contradicen.
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.move_row(-1),
                    MouseEventKind::ScrollDown => self.move_row(1),
                    _ => {}
                }

                // Up (release) en vez de Down: en Windows el primer Down puede
                // consumirse al recuperar el foco de la terminal.
                if let MouseEventKind::Up(MouseButton::Left) = mouse.kind {
                    if let Some(idx) = self.card_at(mouse.column, mouse.row) {
                        self.selected = idx;
                        self.set_hover(Some(idx));
                        return SubmenuAction::Run(self.entries[idx].0);
                    }
                }

                // Cualquier actividad del ratón actualiza el hover (Moved no
                // llega de forma fiable en consolas de Windows; Drag sí).
                let h = self.card_at(mouse.column, mouse.row);
                self.set_hover(h);
            }
            Event::Resize(_, _) => self.ensure_visible(),
            _ => {}
        }
        SubmenuAction::Continue
    }
}

/// Pie del submenú: atajos a la izquierda y ruta completa a la derecha, igual
/// que el menú principal. Si la ruta no cabe, se recorta por la izquierda.
fn render_footer(frame: &mut Frame, area: Rect, ctx: &ProjectContext, theme: &Theme) {
    let left = Line::from(vec![
        Span::styled(
            "  \u{2190}\u{2192}\u{2191}\u{2193}",
            Style::default().fg(theme.text),
        ),
        Span::styled(" navegar   ", Style::default().fg(theme.muted)),
        Span::styled("Enter", Style::default().fg(theme.text)),
        Span::styled(" ejecutar   ", Style::default().fg(theme.muted)),
        Span::styled("Esc", Style::default().fg(theme.text)),
        Span::styled(" volver   ", Style::default().fg(theme.muted)),
    ]);
    let left_w = (left.width() as u16).min(area.width);

    let full = ctx.script_dir.display().to_string();
    let right_w = area.width - left_w;
    let chars: Vec<char> = full.chars().collect();
    let shown = if chars.len() as u16 > right_w && right_w >= 2 {
        let keep = right_w as usize - 1;
        format!(
            "\u{2026}{}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::context::ProjectType;
    use ratatui::{backend::TestBackend, Terminal};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Demo {
        A,
        B,
        C,
        D,
        E,
    }

    fn item(label: &'static str, key: &'static str) -> SubmenuItem {
        SubmenuItem {
            icon: "\u{2699}",
            label,
            description: "descripcion de prueba",
            color_key: key,
        }
    }

    fn demo() -> Submenu<Demo> {
        Submenu::new(
            "DEMO",
            "MNT",
            vec![
                (Demo::A, item("Alfa", "run")),
                (Demo::B, item("Beta", "build")),
                (Demo::C, item("Gamma", "apk")),
                (Demo::D, item("Delta", "sign")),
                (Demo::E, item("Epsilon", "git")),
            ],
        )
    }

    fn ctx() -> ProjectContext {
        ProjectContext {
            script_dir: std::path::PathBuf::from("C:/tmp/proyecto"),
            project_type: ProjectType::Tauri,
            android_home: Some(std::path::PathBuf::from("C:/Android")),
            ndk_home: None,
            adb_path: None,
            sdk_manager: None,
            has_node_modules: false,
            has_android_project: false,
            has_whatsapp_engine: false,
        }
    }

    fn draw(menu: &mut Submenu<Demo>, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let ctx = ctx();
        term.draw(|f| menu.draw(f, f.area(), &ctx, crate::theme::DEFAULT))
            .unwrap();
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_card_center_hits_its_own_index() {
        let mut menu = demo();
        draw(&mut menu, 100, 30);
        let ui = menu.ui.expect("layout");
        let cell_w = grid_cell_w(ui.list.width, ui.cols);
        let card_w = grid_card_w(ui.list.width, ui.cols);
        for idx in 0..menu.len() {
            let row = idx / ui.cols;
            let col = idx % ui.cols;
            let x = ui.list.x + col as u16 * cell_w + card_w / 2;
            let y = ui.list.y + row as u16 * ROW_H + CARD_H / 2;
            assert_eq!(menu.card_at(x, y), Some(idx), "card {idx} en ({x},{y})");
        }
    }

    #[test]
    fn gaps_and_header_are_not_cards() {
        let mut menu = demo();
        draw(&mut menu, 100, 30);
        let ui = menu.ui.expect("layout");
        assert_eq!(menu.card_at(ui.list.x, ui.header.y), None);
        // Hueco vertical entre la primera y la segunda fila.
        assert_eq!(menu.card_at(ui.list.x + 1, ui.list.y + CARD_H), None);
    }

    #[test]
    fn header_reuses_the_main_menu_style() {
        let mut menu = demo();
        let screen = draw(&mut menu, 100, 30);
        assert!(screen.contains(&format!("Compass  v{}", env!("CARGO_PKG_VERSION"))));
        assert!(!screen.contains("Compass for"));
        assert!(screen.contains("DEMO 5"));
    }

    #[test]
    fn selection_never_leaves_the_viewport() {
        let mut menu = demo();
        // Terminal corta: con 14 filas sólo caben unas pocas filas de cards.
        draw(&mut menu, 100, 14);
        for _ in 0..10 {
            menu.move_row(1);
            menu.ensure_visible();
            let ui = menu.ui.expect("layout");
            let rows = visible_rows(ui.list.height).max(1);
            let row = menu.selected / ui.cols;
            assert!(
                row >= menu.scroll as usize && row < menu.scroll as usize + rows,
                "selected {row} fuera de [{}, {})",
                menu.scroll,
                menu.scroll as usize + rows
            );
        }
    }

    #[test]
    fn enter_runs_the_selected_entry() {
        let mut menu = demo();
        menu.move_selection(1);
        match menu.handle_input(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ))) {
            SubmenuAction::Run(Demo::B) => {}
            _ => panic!("Enter debe devolver la entrada seleccionada"),
        }
    }
}
