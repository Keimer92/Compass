//! Contenedor de terminal propio de Compass.
//!
//! Las cards del menú ya no abren ventanas externas: abren una **pestaña**
//! dentro de esta ventana. Cada pestaña es una sesión ConPTY (`portable-pty`)
//! que corre el shell activo (`cmd::shell()`) ejecutando
//! `compass --task … --path …`; su salida se parsea con `vt100` y se pinta con
//! el widget `PseudoTerminal` de `tui-term`.
//!
//! El proceso de la tarea hereda `COMPASS_EMBEDDED=1` (ver `cmd::embedded`):
//! con esa marca, los comandos interactivos de las tareas (`flutter run`,
//! `gradlew :run`…) se ejecutan en esta misma consola en vez de abrir otra
//! ventana, que es justo lo que el contenedor evita.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use tui_term::widget::PseudoTerminal;

use crate::theme::Theme;
use crate::utils::cmd;

/// Espera del bucle del terminal (≈30 fps): así la salida del hijo y el
/// cursor se ven fluir, y la pulsación de una tecla no espera al siguiente
/// fotograma.
const FRAME: Duration = Duration::from_millis(33);

/// Líneas de historial que conserva cada sesión para el parser.
const SCROLLBACK: usize = 2000;

/// Alto de la barra de pestañas: borde superior, etiqueta y borde inferior.
pub const BAR_HEIGHT: u16 = 3;

/// Columnas vacías entre pestañas.
const TAB_GAP: u16 = 1;

/// Una sesión ConPTY con su parser de salida.
pub struct PtySession {
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    pid: u32,
    exited: Arc<AtomicBool>,
    size: Mutex<(u16, u16)>,
}

impl PtySession {
    /// Arranca `script` con el shell activo dentro de una ConPTY nueva.
    pub fn spawn(script: &str, dir: &Path, rows: u16, cols: u16) -> Result<Self> {
        let rows = rows.max(2);
        let cols = cols.max(10);

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("No se pudo crear la sesión de terminal")?;

        let mut command = CommandBuilder::new(cmd::shell());
        for arg in ["-NoLogo", "-NoProfile", "-Command", script] {
            command.arg(arg);
        }
        command.cwd(dir);
        command.env("COMPASS_EMBEDDED", "1");

        let child = pair
            .slave
            .spawn_command(command)
            .context("No se pudo iniciar el shell de la pestaña")?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("No se pudo leer de la sesión de terminal")?;
        let writer = pair
            .master
            .take_writer()
            .context("No se pudo escribir en la sesión de terminal")?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK)));
        let exited = Arc::new(AtomicBool::new(false));
        let pid = child.process_id().unwrap_or(0);

        {
            let parser = Arc::clone(&parser);
            let exited = Arc::clone(&exited);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut parser) = parser.lock() {
                                parser.process(&buf[..n]);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        // `ERROR_BROKEN_PIPE` al cerrar la pestaña: fin normal.
                        Err(_) => break,
                    }
                }
                exited.store(true, Ordering::Relaxed);
            });
        }

        Ok(Self {
            parser,
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
            pid,
            exited,
            size: Mutex::new((rows, cols)),
        })
    }

    /// Acceso a la pantalla parseada (el lock dura lo que el callback).
    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        let parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        f(parser.screen())
    }

    pub fn write(&self, bytes: &[u8]) {
        if self.exited.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    /// Propaga el tamaño al parser y a la ConPTY.
    ///
    /// Los dos tienen que ir juntos: el parser decide dónde corta cada línea
    /// al pintar y la ConPTY decide hasta dónde puede escribir el hijo.
    pub fn resize(&self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        {
            let mut size = self.size.lock().unwrap_or_else(|e| e.into_inner());
            if *size == (rows, cols) {
                return;
            }
            *size = (rows, cols);
        }
        if let Ok(mut parser) = self.parser.lock() {
            parser.set_size(rows, cols);
        }
        if let Ok(master) = self.master.lock() {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::Relaxed)
    }

    /// Mata el árbol de procesos de la pestaña.
    ///
    /// `taskkill /T` hace falta porque matar sólo al `pwsh` de la pestaña deja
    /// vivos a sus nietos (`compass --task`, `cargo`, `gradlew`, `flutter`…),
    /// que seguirían escribiendo en la ConPTY.
    pub fn close(&self) {
        if self.pid != 0 {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &self.pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.close();
    }
}

struct Pane {
    title: String,
    session: PtySession,
}

/// Pestañas del contenedor. `active == None` es la pestaña del menú (Inicio).
#[derive(Default)]
pub struct Container {
    panes: Vec<Pane>,
    active: Option<usize>,
    /// Última pestaña de terminal visitada, para que F12 vuelva a ella.
    last: Option<usize>,
    /// Zonas clicables de la barra dibujada: `None` = pestaña Compass.
    bar: Vec<(Option<usize>, Rect)>,
    /// Botones de cierre (`×`) dibujados: índice de pestaña y zona clicable.
    closes: Vec<(usize, Rect)>,
}

impl Container {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Abre una pestaña nueva y le da el foco.
    pub fn open(
        &mut self,
        title: String,
        script: &str,
        dir: &Path,
        rows: u16,
        cols: u16,
    ) -> Result<()> {
        let session = PtySession::spawn(script, dir, rows, cols)?;
        self.panes.push(Pane { title, session });
        self.active = Some(self.panes.len() - 1);
        self.last = self.active;
        Ok(())
    }

    /// Vuelve al menú sin cerrar ninguna sesión.
    pub fn show_menu(&mut self) {
        if self.active.is_some() {
            self.last = self.active;
        }
        self.active = None;
    }

    /// Vuelve a la última pestaña de terminal. `false` si no hay ninguna.
    pub fn show_last(&mut self) -> bool {
        if let Some(i) = self.last {
            if i < self.panes.len() {
                self.active = Some(i);
                return true;
            }
        }
        match self.panes.len() {
            0 => false,
            n => {
                self.active = Some(n - 1);
                self.last = self.active;
                true
            }
        }
    }

    pub fn select(&mut self, target: Option<usize>) {
        match target {
            None => self.show_menu(),
            Some(i) if i < self.panes.len() => {
                self.active = Some(i);
                self.last = Some(i);
            }
            _ => {}
        }
    }

    /// Salta a la pestaña `n` (1-based, como los números de la barra).
    pub fn select_number(&mut self, n: usize) {
        if n >= 1 && n <= self.panes.len() {
            self.select(Some(n - 1));
        }
    }

    pub fn next(&mut self, delta: isize) {
        if self.panes.is_empty() {
            self.active = None;
            return;
        }
        let n = self.panes.len() as isize;
        let cur = self.active.unwrap_or(0) as isize;
        let i = (cur + delta).rem_euclid(n) as usize;
        self.active = Some(i);
        self.last = Some(i);
    }

    pub fn close_active(&mut self) {
        let Some(i) = self.active else {
            return;
        };
        self.close_pane(i);
    }

    fn close_pane(&mut self, i: usize) {
        if i >= self.panes.len() {
            return;
        }
        let pane = self.panes.remove(i);
        pane.session.close();
        if self.panes.is_empty() {
            self.active = None;
            self.last = None;
            return;
        }
        // Cerrar una pestaña que no es la activa no debe mover el foco; sólo
        // se corrigen los índices cuando quedan por encima de la borrada.
        match self.active {
            Some(a) if a == i => self.active = Some(i.min(self.panes.len() - 1)),
            Some(a) if a > i => self.active = Some(a - 1),
            _ => {}
        }
        match self.last {
            Some(l) if l == i => self.last = self.active,
            Some(l) if l > i => self.last = Some(l - 1),
            _ => {}
        }
    }

    /// Cierra todas las pestañas (al salir de Compass: `process::exit` no
    /// ejecuta destructores).
    pub fn close_all(&mut self) {
        for pane in self.panes.drain(..) {
            pane.session.close();
        }
        self.active = None;
        self.last = None;
    }

    /// Clic en la barra de pestañas. `true` si cayó en una o en un botón `×`.
    pub fn click(&mut self, col: u16, row: u16) -> bool {
        let pos = Position::new(col, row);
        let close = self
            .closes
            .iter()
            .find(|(_, rect)| rect.contains(pos))
            .map(|&(i, _)| i);
        if let Some(i) = close {
            self.close_pane(i);
            return true;
        }
        let hit = self
            .bar
            .iter()
            .find(|(_, rect)| rect.contains(pos))
            .map(|(target, _)| *target);
        match hit {
            Some(target) => {
                self.select(target);
                true
            }
            None => false,
        }
    }

    /// Espera eventos y los reenvía a la sesión activa.
    ///
    /// Se vacía la cola entera por fotograma: si se leyera un solo evento por
    /// iteración, escribir rápido quedaría limitado a ~30 teclas por segundo.
    pub fn poll_input(&mut self) -> Result<()> {
        if !event::poll(FRAME)? {
            return Ok(());
        }
        loop {
            let evt = event::read()?;
            self.handle_event(evt);
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, evt: Event) {
        match evt {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                // Teclas reservadas del contenedor: no llegan al hijo.
                if key.code == KeyCode::F(12) {
                    self.show_menu();
                    return;
                }
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Tab => {
                            self.next(1);
                            return;
                        }
                        KeyCode::BackTab => {
                            self.next(-1);
                            return;
                        }
                        KeyCode::Char(c) if c.eq_ignore_ascii_case(&'w') => {
                            self.close_active();
                            return;
                        }
                        _ => {}
                    }
                }
                if key.modifiers.contains(KeyModifiers::ALT) {
                    if let KeyCode::Char(c) = key.code {
                        if let Some(d) = c.to_digit(10) {
                            self.select_number(d as usize);
                            return;
                        }
                        if c.eq_ignore_ascii_case(&'m') {
                            self.show_menu();
                            return;
                        }
                    }
                }
                if let Some(bytes) = encode_key(key) {
                    if let Some(i) = self.active {
                        if let Some(pane) = self.panes.get(i) {
                            pane.session.write(&bytes);
                        }
                    }
                }
            }
            // La barra superior pertenece al contenedor; el resto del ratón se
            // reenvía al hijo activo como SGR 1006. ConPTY lo traduce a
            // `MOUSE_EVENT`s, así que los TUIs hijos (firmas, Git) tienen el
            // mismo hover y clic que el menú. Un shell que no pidió el modo
            // ratón los ignora (verificado: no llegan como texto a la entrada).
            Event::Mouse(mouse) => {
                let clicked_bar = matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left))
                    && self.click(mouse.column, mouse.row);
                if !clicked_bar {
                    self.forward_mouse(mouse);
                }
            }
            _ => {}
        }
    }

    fn forward_mouse(&mut self, mouse: event::MouseEvent) {
        let Some(i) = self.active else {
            return;
        };
        let Some(pane) = self.panes.get(i) else {
            return;
        };
        if let Some(seq) = encode_mouse_sgr(mouse, BAR_HEIGHT) {
            pane.session.write(&seq);
        }
    }

    /// Pinta la barra superior: una caja redondeada por pestaña, con un botón
    /// `×` en las de tarea. La pestaña del menú se llama "Compass".
    pub fn draw_bar(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // (destino, etiqueta, terminada, cerrable)
        let mut entries: Vec<(Option<usize>, String, bool, bool)> =
            vec![(None, "Compass".to_string(), false, false)];
        for (i, pane) in self.panes.iter().enumerate() {
            let mut label = format!("{} {}", i + 1, short_title(&pane.title, 20));
            let exited = pane.session.is_exited();
            if exited {
                label.push_str(" \u{2713}");
            }
            entries.push((Some(i), label, exited, true));
        }

        let (bar, closes) = draw_tab_bar(frame, area, theme, &entries, self.active);
        self.bar = bar;
        self.closes = closes;
    }

    /// Pinta la sesión activa y su pie. El menú no se dibuja en este modo.
    pub fn draw_body(&mut self, frame: &mut Frame, body: Rect, theme: &Theme) {
        let Some(i) = self.active else {
            return;
        };
        let Some(pane) = self.panes.get(i) else {
            return;
        };

        let [term, footer] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(body);
        if term.height == 0 || term.width == 0 {
            return;
        }

        pane.session.resize(term.width, term.height);
        pane.session.with_screen(|screen| {
            frame.render_widget(PseudoTerminal::new(screen), term);
        });

        let state = if pane.session.is_exited() {
            Span::styled("proceso finalizado", Style::default().fg(theme.warning))
        } else {
            Span::styled("en ejecucion", Style::default().fg(theme.success))
        };
        let left = Line::from(vec![
            Span::styled("  F12", Style::default().fg(theme.text)),
            Span::styled(" men\u{fa}  ", Style::default().fg(theme.muted)),
            Span::styled("Ctrl+W", Style::default().fg(theme.text)),
            Span::styled(" cerrar  ", Style::default().fg(theme.muted)),
            Span::styled("Ctrl+Tab", Style::default().fg(theme.text)),
            Span::styled(" siguiente  ", Style::default().fg(theme.muted)),
            Span::styled("Alt+1..9", Style::default().fg(theme.text)),
            Span::styled(" pesta\u{f1}a  ", Style::default().fg(theme.muted)),
            state,
        ]);
        let left_w = (left.width() as u16).min(footer.width);
        frame.render_widget(
            Paragraph::new(left),
            Rect {
                width: left_w,
                ..footer
            },
        );

        let right = format!("{} ", cmd::shell());
        let right_w = (right.chars().count() as u16).min(footer.width.saturating_sub(left_w));
        if right_w > 0 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    right,
                    Style::default().fg(theme.muted),
                )))
                .alignment(ratatui::layout::Alignment::Right),
                Rect {
                    x: footer.x + footer.width - right_w,
                    width: right_w,
                    ..footer
                },
            );
        }
    }
}

/// Dibuja las cajas de la barra y devuelve sus zonas clicables: la pestaña y,
/// en las de tarea, el botón `×`.
fn draw_tab_bar(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    entries: &[(Option<usize>, String, bool, bool)],
    active_tab: Option<usize>,
) -> (Vec<(Option<usize>, Rect)>, Vec<(usize, Rect)>) {
    let mut bar = Vec::new();
    let mut closes = Vec::new();
    if area.height == 0 || area.width == 0 {
        return (bar, closes);
    }

    let mut x = area.x;
    for (n, (target, label, exited, closable)) in entries.iter().enumerate() {
        let label_w = label.chars().count() as u16;
        // " etiqueta " (+ " × ") dentro de los bordes.
        let inner_w = label_w + if *closable { 4 } else { 2 };
        let w = inner_w + 2;
        if n > 0 {
            if x + TAB_GAP > area.right() {
                break;
            }
            x += TAB_GAP;
        }
        if x + w > area.right() {
            break;
        }

        let tab = Rect {
            x,
            y: area.y,
            width: w,
            height: BAR_HEIGHT.min(area.height),
        };
        let active = *target == active_tab;

        let border_style = if active {
            Style::default()
                .fg(theme.brand)
                .add_modifier(Modifier::BOLD)
        } else if *exited {
            Style::default().fg(theme.muted)
        } else {
            Style::default().fg(theme.border)
        };
        let text_style = if active {
            Style::default()
                .fg(theme.brand)
                .add_modifier(Modifier::BOLD)
        } else if *exited {
            Style::default().fg(theme.muted)
        } else {
            Style::default().fg(theme.text)
        };

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);
        if active {
            block = block.style(Style::default().bg(theme.hover_bg));
        }

        let mut spans = vec![Span::styled(format!(" {label} "), text_style)];
        if *closable {
            let close_style = if active {
                text_style
            } else {
                Style::default().fg(theme.muted)
            };
            spans.push(Span::styled("\u{d7}", close_style));
            spans.push(Span::raw(" "));

            // El `×` va tras " etiqueta ": borde + hueco + etiqueta + hueco.
            // La zona clicable abarca los dos huecos y el glifo, a todo el
            // alto de la barra para acertar fácil.
            let glyph_x = tab.x + 3 + label_w;
            closes.push((
                target.expect("sólo las pestañas de tarea se cierran"),
                Rect {
                    x: glyph_x - 1,
                    y: area.y,
                    width: 3,
                    height: tab.height,
                },
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)).block(block), tab);
        bar.push((*target, tab));
        x += w;
    }
    (bar, closes)
}

fn short_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        return title.to_string();
    }
    let mut s: String = title.chars().take(max.saturating_sub(1)).collect();
    s.push('\u{2026}');
    s
}

/// Traduce una tecla de crossterm a los bytes que espera la consola.
///
/// Cubre lo que usan las herramientas del menú (CLIs, `flutter run`, gradle y
/// una shell de PowerShell): texto, control, flechas, navegación y F1–F12.
/// Las secuencias son las estándar de xterm, que es lo que ConPTY entrega.
pub fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let mut bytes = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let b = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    'A'..='Z' => c as u8 - b'A' + 1,
                    '@' | ' ' => 0,
                    '[' => 27,
                    '\\' => 28,
                    ']' => 29,
                    '^' => 30,
                    '_' => 31,
                    _ => return None,
                };
                vec![b]
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => return None,
        },
        _ => return None,
    };

    if alt && !bytes.starts_with(&[0x1b]) {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

/// Codifica un evento de ratón como SGR 1006 (`CSI < b ; x ; y M|m`), el
/// formato que ConPTY traduce a `MOUSE_EVENT_RECORD`s para el hijo.
///
/// `top` es la primera fila del área del hijo dentro de la ventana (la barra de
/// pestañas ocupa las de arriba): las coordenadas del ratón son absolutas y el
/// hijo empieza a contar en 1 en su propio origen. Devuelve `None` si el
/// puntero está sobre la barra.
fn encode_mouse_sgr(mouse: event::MouseEvent, top: u16) -> Option<Vec<u8>> {
    if mouse.row < top {
        return None;
    }
    let button = |b: MouseButton| match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    let (code, release) = match mouse.kind {
        MouseEventKind::Down(b) => (button(b), false),
        MouseEventKind::Up(b) => (button(b), true),
        MouseEventKind::Drag(b) => (button(b) + 32, false),
        MouseEventKind::Moved => (35, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };
    let mut mods = 0;
    if mouse.modifiers.contains(KeyModifiers::SHIFT) {
        mods |= 4;
    }
    if mouse.modifiers.contains(KeyModifiers::ALT) {
        mods |= 8;
    }
    if mouse.modifiers.contains(KeyModifiers::CONTROL) {
        mods |= 16;
    }
    let x = mouse.column as u32 + 1;
    let y = (mouse.row - top) as u32 + 1;
    let end = if release { 'm' } else { 'M' };
    Some(format!("\x1b[<{};{};{}{}", code + mods, x, y, end).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn encodes_plain_text_and_enter() {
        assert_eq!(
            encode_key(key(KeyCode::Char('r'), KeyModifiers::NONE)),
            Some(b"r".to_vec())
        );
        assert_eq!(
            encode_key(key(KeyCode::Enter, KeyModifiers::NONE)),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn encodes_control_letters_and_ctrl_c() {
        assert_eq!(
            encode_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![3])
        );
        assert_eq!(
            encode_key(key(KeyCode::Char('z'), KeyModifiers::CONTROL)),
            Some(vec![26])
        );
    }

    #[test]
    fn encodes_arrows_and_navigation() {
        assert_eq!(
            encode_key(key(KeyCode::Up, KeyModifiers::NONE)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode_key(key(KeyCode::Delete, KeyModifiers::NONE)),
            Some(b"\x1b[3~".to_vec())
        );
        assert_eq!(
            encode_key(key(KeyCode::End, KeyModifiers::NONE)),
            Some(b"\x1b[F".to_vec())
        );
    }

    #[test]
    fn alt_prefixes_escape() {
        assert_eq!(
            encode_key(key(KeyCode::Char('f'), KeyModifiers::ALT)),
            Some(b"\x1bf".to_vec())
        );
    }

    #[test]
    fn function_keys_keep_xterm_sequences() {
        assert_eq!(
            encode_key(key(KeyCode::F(5), KeyModifiers::NONE)),
            Some(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            encode_key(key(KeyCode::F(12), KeyModifiers::NONE)),
            Some(b"\x1b[24~".to_vec())
        );
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> event::MouseEvent {
        event::MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// El ratón del cuerpo se reenvía como SGR con la fila relativa al hijo (la
    /// barra de pestañas ocupa las primeras `BAR_HEIGHT` filas).
    #[test]
    fn mouse_is_forwarded_relative_to_the_body() {
        let moved = mouse(MouseEventKind::Moved, 9, BAR_HEIGHT + 4);
        assert_eq!(
            encode_mouse_sgr(moved, BAR_HEIGHT),
            Some(b"\x1b[<35;10;5M".to_vec())
        );

        let down = mouse(MouseEventKind::Down(MouseButton::Left), 0, BAR_HEIGHT);
        assert_eq!(
            encode_mouse_sgr(down, BAR_HEIGHT),
            Some(b"\x1b[<0;1;1M".to_vec())
        );
        // La liberación usa la variante `m` en minúscula.
        let up = mouse(MouseEventKind::Up(MouseButton::Left), 0, BAR_HEIGHT);
        assert_eq!(
            encode_mouse_sgr(up, BAR_HEIGHT),
            Some(b"\x1b[<0;1;1m".to_vec())
        );
    }

    #[test]
    fn mouse_drag_and_wheel_use_the_extended_codes() {
        let drag = mouse(MouseEventKind::Drag(MouseButton::Left), 4, BAR_HEIGHT + 1);
        assert_eq!(
            encode_mouse_sgr(drag, BAR_HEIGHT),
            Some(b"\x1b[<32;5;2M".to_vec())
        );
        let wheel_up = mouse(MouseEventKind::ScrollUp, 4, BAR_HEIGHT + 1);
        assert_eq!(
            encode_mouse_sgr(wheel_up, BAR_HEIGHT),
            Some(b"\x1b[<64;5;2M".to_vec())
        );
        let wheel_down = mouse(MouseEventKind::ScrollDown, 4, BAR_HEIGHT + 1);
        assert_eq!(
            encode_mouse_sgr(wheel_down, BAR_HEIGHT),
            Some(b"\x1b[<65;5;2M".to_vec())
        );
    }

    /// El ratón sobre la barra de pestañas no se reenvía al hijo.
    #[test]
    fn mouse_on_the_bar_is_not_forwarded() {
        for row in 0..BAR_HEIGHT {
            assert_eq!(
                encode_mouse_sgr(mouse(MouseEventKind::Moved, 3, row), BAR_HEIGHT),
                None
            );
        }
    }

    #[test]
    fn shortens_long_tab_titles() {
        assert_eq!(short_title("Run", 20), "Run");
        assert_eq!(
            short_title("0123456789012345678901234", 10),
            "012345678\u{2026}"
        );
    }

    /// Cada `×` dibujado tiene que caer dentro de su zona clicable. La primera
    /// versión calculaba la zona dos columnas a la izquierda del glifo (se
    /// olvidaba el hueco final de `" etiqueta "`), así que el clic sobre la
    /// `×` no cerraba nada.
    #[test]
    fn close_zone_covers_the_rendered_glyph() {
        let entries = vec![
            (None, "Compass".to_string(), false, false),
            (Some(0), "1 Git".to_string(), false, true),
            (Some(1), "2 Etiqueta larga".to_string(), true, true),
        ];
        let width = 80;
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, BAR_HEIGHT))
                .expect("terminal de prueba");

        let mut tabs = Vec::new();
        let mut zones = Vec::new();
        terminal
            .draw(|f| {
                let (bar, closes) =
                    draw_tab_bar(f, f.area(), &crate::theme::THEMES[0], &entries, Some(1));
                tabs = bar;
                zones = closes;
            })
            .expect("dibujo de la barra");

        let buffer = terminal.backend().buffer();
        let mut glyphs = Vec::new();
        for y in 0..BAR_HEIGHT {
            for x in 0..width {
                if buffer[(x, y)].symbol() == "\u{d7}" {
                    glyphs.push(Position::new(x, y));
                }
            }
        }

        assert_eq!(glyphs.len(), 2, "una × por pestaña de tarea");
        // `tabs[0]` es Compass; las de tarea llevan `×`.
        for (i, (_, tab)) in tabs[1..].iter().enumerate() {
            let (_, zone) = zones[i];
            let glyph = glyphs[i];
            assert!(
                tab.contains(glyph),
                "el glifo {i} quedó fuera de su pestaña ({tab:?})"
            );
            assert!(
                zone.contains(glyph),
                "la zona del × {i} ({zone:?}) no cubre el glifo dibujado en {glyph:?}"
            );
            // Clicar la etiqueta selecciona la pestaña; no debe cerrarla.
            let label_cell = Position::new(tab.x + 2, tab.y + 1);
            assert!(
                tab.contains(label_cell) && !zone.contains(label_cell),
                "la zona del × {i} ({zone:?}) invade la etiqueta en {label_cell:?}"
            );
        }
    }

    /// Integración real de la ConPTY: arranca un shell, recoge su salida y la
    /// pinta con el widget. Se ejecuta a mano (`cargo test -- --ignored`)
    /// porque depende del sistema operativo y tarda un par de segundos.
    #[test]
    #[ignore = "abre una ConPTY real"]
    fn session_renders_its_output() {
        let mut panes = Container::new();
        panes
            .open(
                "Prueba".to_string(),
                "Write-Output 'hola contenedor'",
                &std::env::temp_dir(),
                10,
                40,
            )
            .expect("la ConPTY debería abrirse");

        for _ in 0..100 {
            if panes.panes[0].session.is_exited() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 10))
            .expect("terminal de prueba");
        terminal
            .draw(|f| panes.draw_body(f, f.area(), &crate::theme::THEMES[0]))
            .expect("dibujo del terminal");

        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            text.contains("hola contenedor"),
            "la salida de la sesión no llegó a la pantalla: {text:?}"
        );
    }
}
