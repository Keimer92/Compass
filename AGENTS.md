# Compass — Developer Guide

## Overview

La aplicación de escritorio tiene soporte exclusivo para Windows; Android sigue disponible como plataforma de destino para los proyectos compatibles.


`compass` es una herramienta CLI/TUI para Windows que automatiza tareas de desarrollo, compilación y configuración para proyectos **Tauri (Rust)**, **Flutter (Dart)**, **Kotlin Multiplatform (Compose)**, **Go + Wails v3**, **Go puro**, **Python** y **Rust puro**. Detecta automáticamente el tipo de proyecto y adapta comandos, rutas y comportamientos.

El TUI es además un **contenedor de terminal propio**: cada card abre una
pestaña ConPTY dentro de la misma ventana de Compass (PowerShell dentro,
siempre), en lugar de lanzar consolas externas. Ver "Contenedor de terminal".

## Project Types

| Tipo | Detección | Emoji | Menú |
|------|-----------|-------|------|
| `Tauri` | `pubspec.yaml` → Flutter, luego `tauri.conf.json` o `package.json` | 🦀 | Completo |
| `Flutter` | `pubspec.yaml` | 🎯 | Completo |
| `Kotlin` | `gradlew.bat` + `settings.gradle.kts` | 🔷 | Completo |
| `Go + Wails` | `go.mod` con `wailsapp/wails` + `Taskfile.yml` | 🐹 | Completo (sin firma) |
| `Go` | `go.mod` (después de detectar Wails v3) | 🐹 | Run, build, test, clean, git |
| `Python` | `pyproject.toml` con `[project]` o `[tool.poetry]` | 🐍 | Completo (sin Android) |
| `Rust` | `Cargo.toml` (después de todos los anteriores) | ⚙️ | Reducido (sin Android) |

El orden de detección es: **Flutter → Tauri → Go+Wails → KMP → package.json/Tauri → Python → Go → Rust**. Go + Wails v3 se detecta antes que Go puro para conservar su menú especializado. Si un proyecto coincide con `package.json` pero no con `tauri.conf.json`, se clasifica como Tauri (para proyectos JS/Node con frontend).

Para Go puro, Compass ofrece `go run .`, `go build` (guarda el `.exe` en `bin/`), `go test ./...`, limpieza de `bin/` y el flujo Git. No muestra acciones Android ni firma.

## Platform Support

Compass ejecuta como aplicación de escritorio únicamente en **Windows**. Android sigue siendo un destino de desarrollo y compilación para proyectos compatibles; sus targets `*-linux-android` son ABI de Android, no soporte Linux de Compass.

| Función | Windows |
|---------|---------|
| Selector de proyecto (`--pick`) | ✅ |
| Tauri, Flutter, Kotlin, Go + Wails, Go puro, Rust y Python desktop | ✅ |
| Android (dev + release APK/AAB) | ✅ sólo dispositivo físico |
| Menú contextual Compass + Vibe Coding + `.gitignore` | ✅ Registro de Windows |
| Configurar Terminal (PSReadLine) | ✅ |

### Layout: pestañas + rejilla horizontal adaptativa con scroll

El menú ya no pinta las 18 tarjetas en una sola rejilla ni en una columna. Hay
**dos pestañas** y las cards van en una **rejilla que se adapta al ancho**: hasta
**4 por fila**, separadas por una celda vacía, y la fila se desplaza:

| Pestaña | Contenido |
|---------|-----------|
| `DESARROLLO · MNT` | las cards DEV ∪ MNT (run, build, apk, clean, sign, version…) |
| `CONFIGURACIÓN` | Git, WhatsApp, Terminal, Entorno |

`MenuItem::tab()` deriva la pestaña de `section()` (`DEV`/`MNT` → `DevMnt`, el
resto → `Cfg`). `MenuState::new` filtra por tipo de proyecto y luego ordena los
ítems **por pestaña** (`sort_by_key`, estable), así que `items` es contiguo por
pestaña, `tabs: [TabRange; 2]` son rangos sobre él y `selected` sigue siendo un
índice **global**.

Cards de **3 líneas** (`CARD_H = 3`: borde + contenido + borde) que **se adaptan
al tamaño de la ventana**:

- `GRID_MAX_COLS = 4` cards por fila como máximo.
- `CARD_MIN_W = 24` columnas mínimas por card; `grid_cols(width)` cuenta cuántas
  caben (`(width + CARD_GAP) / (CARD_MIN_W + CARD_GAP)`, acotado a `1..=4`), así
  la rejilla pasa a 1/2/3/4 columnas al redimensionar sin perder legibilidad.
- `grid_cell_w` / `grid_card_w` reparten el ancho disponible a partes iguales y
  reservan `CARD_GAP = 1` columna de separación; el ancho real se pasa a
  `create_card`, que calcula el relleno entre etiqueta y descripción con
  `Span::width()` (celdas de pantalla, no `chars()`: los emojis ocupan dos) y
  **trunca** la descripción con `…` si no cabe, en vez de invadir el borde.
- Entre filas también hay `CARD_GAP` filas vacías (`ROW_H = CARD_H + CARD_GAP`),
  así las cards quedan separadas tanto horizontal como verticalmente.

El contenido de cada card es una sola línea con el icono y la etiqueta en el
acento de la card y la descripción en `muted` (`text` si está hovereada). Se
alinean a la izquierda: centrar texto con emoji de ancho ambiguo (⚙) descuadra
la card.

**Scroll** propio, en pasos de **fila completa** (`MenuState.scroll: u16` = primera
fila visible): se pinta cada fila recortando sus `Rect` contra el viewport, y
`ensure_visible()` mantiene en pantalla la fila de la selección
(`row = i / cols`, `visible_rows(height) = (height + CARD_GAP) / ROW_H`, con
`max = total_rows - rows`). Las filas parcialmente visibles no se dibujan
cortadas. El indicador es un `Scrollbar` de una columna junto al viewport; su
contenido es `total_rows * ROW_H` y su posición `scroll * ROW_H`. Si no cabe, la
rejilla se estrecha y el indicador desaparece.

⚠️ Ya **no** existe el aviso "Terminal demasiado pequena" salvo que ni una fila
quepa (`list.height < CARD_H`). Antes el menú se negaba a dibujarse.

`layout(area, warn)` es una **función pura** (header, tabs, list, scrollbar,
footer) que comparten `draw_menu` y `card_at`: el hit-testing del ratón no se
desincroniza al redimensionar. `UiLayout.cols` viaja con ella para que el dibujo
y el clic usen la misma rejilla.

### Estados visuales de una card (`menu.rs::create_card`)

Tres estados, deliberadamente distintos entre sí. El acento es **propio de cada
card** (`theme.card_accent(key)` / `theme.card_bg(key)`), no el de la sección:

| Estado | Borde | Fondo |
|--------|-------|-------|
| reposo | `theme.card_accent(key)`, sin BOLD | ninguno |
| hover | `theme.text` + BOLD | `theme.hover_bg` neutro |
| seleccionado | `theme.card_accent(key)` + BOLD | `theme.card_bg(key)` teñido |

El reposo no lleva BOLD a propósito: en consolas legacy el bold sólo aclara el
foreground y `Color::Rgb` degrada a 16 colores, así que dos acentos pueden verse
idénticos; el estado *además* se distingue por el fondo.

⚠️ Antes hover y selección compartían `glass_tint` y sólo se diferenciaban por el
`BOLD` del borde: el hover pasaba desapercibido. Cubierto por
`menu::tests::hover_is_visually_distinct_from_selected`; si el tint vuelve a
compartirse, el test falla. Además, `theme::tests::hover_is_distinct_from_every_selection`
y `card_bgs_never_collide_with_hover_or_background` verifican que en **ningún**
tema el `hover_bg` ni un `card_bg` colisionan con otro fondo.

Si una card está seleccionada **y** hovereada, manda la selección: es la que
Enter ejecutaría.

### Animación de hover

Mientras el puntero está sobre una card, un **halo luminoso circular** recorre
su borde en bucle (una vuelta cada 2,6 s) con una cola de 9 celdas y un
resplandor interior que sigue a la cabeza, al estilo del orbe del icono de
Google. Arranca al entrar en la card, no se reinicia al moverse dentro de ella
y desaparece al salir.

- Estado: `MenuState.hover_started: Option<Instant>`; `set_hover()` sólo lo
  reinicia al cambiar de card. `hover_phase()` devuelve la fase 0..1 de la
  vuelta en curso y `hover_animating()` mantiene activo el ritmo del bucle.
- Bucle: `handle_event()` usa `event::poll()` con `ANIM_FRAME` (33 ms) mientras
  hay halo y `IDLE_POLL` (1 s) en reposo. Sin esa temporización la UI sólo se
  redibujaba al llegar un evento, así que no podía haber animación.
- Dibujo: `render_hover_glow()` pinta celda a celda sobre el buffer **después**
  de la card (el `Block` sólo admite un color de borde): en el borde mezcla el
  `fg` con el brillo y en el interior tiñe el `bg` hacia el tinte del acento
  (`card_bg`) cerca de la cabeza, con radio elíptico porque la card es ancha y
  baja. `perimeter_point()` da la coordenada de la cabeza para ese resplandor.
- El brillo sale del acento de la card (`shimmer_glow`), que siempre es RGB; en
  temas ANSI (`Compass`) las celdas cercanas a la cabeza pasan al brillo y el
  resto conserva su color, porque `theme::mix` no interpola nombres de color.
  La animación nunca cambia el fondo base ni el layout.

### Teclado y ratón de la TUI

| Entrada | Acción |
|---------|--------|
| `←` `→` | mueven la selección card a card (`∓1`); en el borde **saltan de pestaña** |
| `↑` `↓` | mueven una **fila** de la rejilla (`±cols`) sin salir de la pestaña |
| `Tab` / `Shift+Tab` | pestaña siguiente / anterior |
| `Enter` | ejecuta la card seleccionada |
| `Esc` | sale (con pestañas internas abiertas, pide confirmación) |
| `Ctrl+T` | abre el modal de temas |
| rueda del ratón | mueve una fila (`∓1`), así nunca contradice lo que se ve |
| clic izquierdo | ejecuta la card bajo el cursor (`hover` se actualiza con la actividad del ratón); los huecos entre cards no responden |
| clic en una pestaña | cambia a esa pestaña sin ejecutar nada |

Esta tabla es del **menú** (pestaña `Inicio` de la barra superior). Cuando hay
una pestaña de terminal activa, las teclas van al proceso hijo salvo las
reservadas del contenedor (F12, Ctrl+Tab, Ctrl+W, Alt+1..9), descritas en
"Contenedor de terminal".

El hit-testing de las pestañas usa `tab_titles()` (las mismas etiquetas con su
cuenta) y `tab_at()`, así que clicar una etiqueta nunca apunta a la otra.
`set_tab()` recuerda la selección de cada pestaña (`remember: [usize; 2]`) y
`ensure_visible()` se llama tras mover la selección, al cambiar de pestaña y en
`Event::Resize`.

### Temas del TUI (`Ctrl+T`)

`Ctrl+T` abre un **modal** con la lista de temas. Dentro del modal:

| Tecla | Acción |
|-------|--------|
| `↑` `↓` (o `←` `→`) | mover el resaltado; el menú de detrás se repinta con ese tema (**vista previa**) |
| `Enter` | aplicar y **persistir** el tema resaltado |
| `Esc` (o `Ctrl+T`) | cancelar y volver al tema que estaba activo al abrir |

Mientras el modal está abierto el teclado le pertenece por completo (y el ratón
se ignora): el menú de detrás no reacciona (ni `Enter` ejecuta una tarea). Cada
fila muestra el nombre y tres muestras de color de su propia paleta
(DEV/MNT/CFG), para reconocer el tema sin aplicarlo. El modal se dibuja con
`Clear` encima del menú, y el nombre del tema activo se muestra en el pie, a la
izquierda (`Ctrl+T temas: <nombre>`, en color `brand`); a la derecha va la
**ruta completa** del proyecto (ver "Título de ventana y pie").

### Título de ventana y rutas

- **Barra de título**: cada proyecto arranca su **propio** `compass.exe` y por
  tanto su propia ventana; el título las distingue con
  `ProjectContext::window_title()` = **nombre del directorio + tipo de
  proyecto** (`School Tracker · Tauri`), no la ruta. Lo fija el propio proceso
  al arrancar con `cmd::set_console_title()` (crossterm `SetTitle` →
  `SetConsoleTitleW` en Windows) desde la TUI (`menu::run`), las tareas
  (`run_task_direct`) y los submenús (`git`, `signer`). Las ventanas de
  PowerShell fijan además el título del script (`[Console]::Title = …`) para
  que se vea bien durante el arranque, pero el valor definitivo lo pone el
  proceso hijo.
- **Encabezado del TUI** (`render_header`): `emoji  Compass  v<versión>` en
  `brand` + BOLD y, debajo, `Proyecto: <directorio>` con el badge del tipo
  (`TAURI`, `FLUTTER`, …). La versión sale de `env!("CARGO_PKG_VERSION")` —no
  de un literal— y `clap` usa el mismo `#[command(version)]`, así que
  `compass --version` y la TUI nunca se desincronizan de `Cargo.toml`. El tipo
  de proyecto va **sólo** en el badge; el título no repite "Compass for …".
- **Pie del TUI**: a la izquierda, los atajos (`Tab pestañas`,
  `←→↑↓ navegar`, `Enter ejecutar`, `Esc salir`) y el tema activo (`Ctrl+T
  temas: <nombre>`, en color `brand`). A la derecha, la **ruta completa** del
  proyecto en color `muted`; si no cabe, se recorta por la izquierda con `…`.

Estado: `MenuState.picker: Option<ThemePicker>` (`index` en `theme::THEMES` +
`original`, para el cancelar). `MenuState::preview_theme()` mueve el resaltado y
aplica el tema; `render_theme_picker()` dibuja la capa.

- Los temas están **basados en OpenCode** (`packages/tui/src/theme/assets/*.json`
  del repo de OpenCode). El por defecto, `Compass`, reproduce el aspecto previo
  (Cyan/Yellow/Magenta + el tinte de vidrio original) para no cambiar nada a
  quien no toque el tema.
- Un `Theme` son **roles semánticos**, no colores: `menu.rs` no tiene un solo
  literal de color. Ambos fondos de card salen de la paleta del tema.
- **Acento por card**: `theme::CARD_KEYS` fija las 18 claves semánticas (una por
  `MenuItem`, en orden canónico) y `Theme.card_hexes: [u32; 18]` guarda un hex
  por clave **y por tema**. `card_accent(key)` es ese color y `card_bg(key)` el
  mismo con `dark(hex, 27)`; por tema sólo se eligen 18 hex, no 36. La barra de
  pestañas sigue usando `dev`/`cfg` (el acento de *su* sección) para conservar
  la identidad cromática. `theme::tests` verifica que dentro de una pestaña no
  hay dos acentos iguales en ningún tema, que los fondos no chocan con el hover
  ni con `background`, y que `Compass` conserva los colores actuales.
- `dev`/`mnt`/`cfg`/`hover_bg` se **conservan** porque los consumen el menú y
  los submenús (pestañas y fila de sección), y `hover_bg` va **explícito por
  tema** (no se deriva) porque el "vidrio esmerilado" es una decisión estética.
  Los antiguos `dev_bg`/`mnt_bg`/`cfg_bg` (fondo de la card seleccionada por
  sección) **se eliminaron** cuando Firmas y Git pasaron a usar el mismo
  `card_bg` por card que el menú: ya no tenían consumidor. El helper
  `const fn dark()` sigue usándose para `card_bg`.
- El fondo de **toda la pantalla** es `background` (a lo OpenCode: cada tema pinta
  su propio fondo con un `Block` que cubre `frame.area()` al principio de
  `draw_menu`; el resto de widgets no lo pisan porque sus `Style` no definen
  `bg`). El modal de temas también lo pinta (su `Clear` dejaría el fondo nativo
  de la terminal). Los badges de tipo de proyecto se mapean a
  `success`/`info`/`accent`/`warning`.
- **Persistencia**: el tema elegido se guarda como una línea de texto en
  `<data_dir>/compass/theme` (`%LOCALAPPDATA%\compass\theme` en Windows) con `theme::save()`, y se recupera en
  `menu::run` con `theme::load()`. Un fichero ausente o con un nombre desconocido
  cae al tema por defecto sin error.
- Añadir un tema = añadir una entrada a `theme::THEMES`. `next()`/`prev()` operan
  sobre el índice, así que no hay que tocar nada más.

### Selector de proyecto (`context::pick_project_root`)

Navegador de directorios que se usa cuando `--pick` está activo o cuando
`detect()` falla y no hay `--task`. `main.rs` cae ahí automáticamente: es lo que
hace utilizable la entrada del launcher.

- Mantiene `options` y `actions` en vectores paralelos; despacha por acción, no
  por el prefijo del texto (un `starts_with("2.")` ya causó un bug con "2. Volver").
- Las subcarpetas se marcan con su tipo (`classify_dir`), para que se sepa qué
  es un proyecto antes de entrar.
- Los ocultos se listan al final, no se ocultan.
- Esc / Ctrl-C devuelven `Ok(None)`; `main.rs` sale sin error.

## ProjectContext

Estructura inyectada en todas las funciones de tarea:

```
ProjectContext
  script_dir: PathBuf           # Raíz del proyecto detectado
  project_type: ProjectType     # Tauri | Flutter | Kotlin | Go | GoWails | Python | Rust
  android_home: Option<PathBuf>
  ndk_home: Option<PathBuf>
  adb_path: Option<PathBuf>
  sdk_manager: Option<PathBuf>
  has_node_modules: bool
  has_android_project: bool     # false para Rust
  has_whatsapp_engine: bool     # engine_dir existe
```

## Module Map

```
main.rs
  └─ Cli (clap) → --task, --path
      ├─ sin args → menu::run()
      └─ con args → run_task_direct() dispatches by task name
          └─ run_global_task() para context-menu y opencode

container.rs
  ├─ PtySession — una sesión ConPTY (`portable-pty`) que corre el shell activo
  │   con `-Command <script>`; lector en hilo → `vt100::Parser`; writer, resize,
  │   `is_exited()` y `close()` (taskkill /T del árbol + kill del hijo)
  ├─ Container — pestañas: `active: Option<usize>` (None = pestaña Compass),
  │   `last` para F12, `bar` y `closes` con las zonas clicables;
  │   open/show_menu/show_last/select/next/close/close_all, poll_input() y
  │   handle_event() (teclas reservadas: F12, Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+W,
  │   Alt+1..9, Alt+M; el resto se codifica a xterm con `encode_key` y se
  │   escribe en la ConPTY)
  └─ draw_bar() (cajas redondeadas separadas, "Compass" + una por tarea con
      `×` y ✓ al terminar) y draw_body() (PseudoTerminal de `tui-term` + pie)

theme.rs
  ├─ Theme — roles semánticos (text/muted/border/brand, dev/mnt/cfg + *_bg,
  │          hover_bg, success/info/accent/warning) **+ card_hexes: [u32; 18]**
  │          (acento por card, orden de CARD_KEYS). Ningún literal de color en
  │          menu.rs: cambiar de tema sólo cambia esta tabla.
  ├─ CARD_KEYS — 18 claves semánticas (run, android, …, git, whatsapp, terminal,
  │              environment);
  │              card_accent(key) / card_bg(key) las resuelven contra el tema
  ├─ THEMES — 14 temas basados en OpenCode (Compass, OpenCode, Tokyo Night,
  │           Catppuccin, Dracula, Gruvbox, Nord, One Dark, Monokai, Rosé Pine,
  │           Kanagawa, Matrix, Synthwave 84, Solarized)
  ├─ next()/prev()/by_name() — ciclo de Ctrl+T
  ├─ mix(a, b, t) — interpola colores RGB (animación de hover); con colores
  │                 no-RGB devuelve `b` para no romper temas ANSI
  └─ load()/save() — persiste el nombre en <data_dir>/compass/theme

menu.rs
  ├─ MenuItem (18 variants: WindowsDev, AndroidDev, ..., Signing, Version,
  │   Environment)
  │   — 🚫 **ya no existe la variante `ContextMenu`**: `install.bat` instala el menú
  │     contextual al arrancar, la tarjeta era redundante (las tareas `context-menu*`
  │     por CLI siguen disponibles)
  ├─ ratatui TUI en **dos pestañas** (`DESARROLLO · MNT` | `CONFIGURACIÓN`) con las
  │   cards en **rejilla de hasta 4 columnas** (3 líneas por card, separadas por
  │   una celda) que se adapta al ancho, **scroll por filas** y un color por card;
  │   pestañas clicables con el ratón
  ├─ Filtro por tipo de proyecto (Rust oculta Android/Signing)
  ├─ grid_cols()/grid_card_w()/visible_rows() — geometría pura de la rejilla,
  │   compartida por el dibujo, el scroll y el hit-testing
  ├─ layout(area, warn) — función pura (header/tabs/list/scrollbar/footer) que
  │   comparten el dibujo y el hit-testing del ratón (incluye `cols`)
  ├─ handle_event(): devuelve MenuAction (Run | Quit); teclado (←→ card a card,
  │   ↑↓ por filas dentro de la pestaña, Tab/Shift+Tab pestañas) + mouse (rueda
  │   mueve una fila, clic en pestaña cambia de pestaña, clic en card ejecuta);
  │   poll temporizado (33 ms con animación de hover, 1 s en reposo)
  ├─ render_hover_glow() — halo luminoso circular en bucle sobre la card
  │   hovereada (borde + resplandor interior); perimeter_point() ubica la cabeza
  ├─ draw_ui() — frame completo: barra de pestañas del `Container` (1 línea) +
  │   menú o sesión activa; `draw_menu_in(area)` permite pintar el menú debajo
  ├─ Sin botón Salir: `Esc` cierra el programa (`MenuAction::Quit`); si hay
  │   pestañas de tarea abiertas, antes pide confirmación (`quit_confirm`)
  └─ run(): loop draw_ui → (pestaña activa → `panes.poll_input()`) o
      handle_event → Run abre una **pestaña** con el CLI de la tarea (fallback a
      ventana externa si la ConPTY falla o con `COMPASS_EXTERNAL_WINDOWS=1`)

utils/
  ├─ context.rs — detect(), ProjectType, ProjectContext, whatsapp_path()
  ├─ platform.rs — helpers de portabilidad: IS_WINDOWS, exe_name(), gradlew_name(), home_dir(), data_dir(), find_gradlew(), NDK_VERSION, tauri_native_pkg()
  ├─ cmd.rs — shell()/shell_path()/ps_quote/embedded(), run*, run_ps*, run_ps_file_status, set_console_title, spawn_new_window_autoexit, spawn_new_window_interactive [en modo embebido ejecutan en la consola actual], run_build_and_reveal, run_in_current_window, run_shell_status, run_npm_install, clear_screen, remove_dir
  ├─ tauri.rs — command(), ensure(), cli_entry() [CLI sin shim de .bin + reinstalación si falta el binding nativo]
  ├─ ui.rs — ask_text(), ask_password(), success_banner(), info_banner(), pause()
  ├─ console_guard.rs — guarda del cierre de la ventana anfitriona: install() [SetConsoleCtrlHandler], set_open_tabs(), register_pane()/unregister_pane(); en conhost deshabilita SC_CLOSE con pestañas abiertas y en el cierre mata los árboles + aviso nativo
  └─ android.rs — select_device(), wait_for_device_boot(), list_physical_devices() [sólo dispositivos físicos]

tasks/
  ├─ windows.rs — dev(), build(), build_arch()           [Tauri only; build_arch = target por arquitectura + NSIS]
  ├─ android.rs — dev(), build_arch(), build_universal(), build_aab(), debug(), ensure_package_consistent() [verifica identifier vs directorio paquete Android]; build_arm64()/build_armv7()/build_all() quedan como alias CLI
  ├─ flutter.rs — run_windows(), run_android(), profile_windows(), profile_android(), build_apk_arch(), build_apk_universal(), build_apks_all(), build_*(), clean()
  ├─ kotlin.rs  — run_windows() [con hot reload], run_android(), install_debug() (`:<modulo>:installDebug` + lanza con `am start`), build_apk_arch(), build_apk_universal(), build_apks_all() [ABI splits por init script], build_aab(), has_native_libs(), clean(); deteccion de modulo Gradle `:app`/`:composeApp` via `gradle_module()`; release_signing_env() [exporta RELEASE_* vars, parseando con `signer::Props`]
  ├─ gowails.rs — dev(), build_desktop() [lote x86/x64/ARM64], build_desktop_arch(), package_desktop(), build_apk_arch(), build_apk_universal(), build_release_apks(), build_aab(), install_debug(), debug_android(), clean()  [Go + Wails v3: pipeline Android propio, ver "Go + Wails v3"]
  ├─ go.rs      — run(), build(), build_arch() [GOARCH + CGO_ENABLED=0, icono sólo amd64], test(), clean()  [Go puro]
  ├─ rust.rs    — run(), build(), build_arch() [target triple], clean()  [Rust only]
  ├─ release.rs — manage_windows()/manage_android() [submenús TUI de release por arquitectura: cards x86/x64/ARM64 y arm64/armv7/universal + Todos, con `tasks/submenu.rs`; ver "Release por arquitectura"]
  ├─ cleanup.rs — manage() [submenú unificado: rápida / completa / reporte de tamaños], quick_targets()/full_targets(), clean_project() [Tauri: portabilidad]
  ├─ git.rs     — manage() [submenú unificado: init / .gitignore / conectar con remoto / estado, con `tasks/submenu.rs`]; templates de .gitignore por tipo
  ├─ signer.rs  — dominio único de firmas: manage() [submenú con `tasks/submenu.rs`], android_layout(), detect()/SigningInfo [autodetección], Props (parseo de properties), crear/importar/manual, patch_build_gradle(), huellas (keytool), respaldos, base64 + secretos de GitHub Actions, verify_apk() [apksigner]
  ├─ submenu.rs — TUI compartida de los submenús (Firmas, Git, Entorno, Release): reutiliza la cabecera, la fila de sección, la rejilla adaptativa de cards de 3 líneas, el halo de hover y el pie del menú principal; `Submenu<T>` devuelve `SubmenuAction::{Run(T), Back, Continue}` y `with_android_warning(false)` omite la fila de aviso del SDK
  ├─ terminal.rs — configure() [perfil PSReadLine según la versión instalada]
  ├─ context_menu.rs — install/uninstall/toggle para Compass + Vibe Coding (menú cascada con ExtendedSubCommandsKey: OpenCode, Kilo Code, Cline, Antigravity, Copilot CLI, Kiro CLI, Qoder CLI, Hermes)
  ├─ environment.rs — manage() [submenú: backup completo / inventario / restaurar / validar / abrir respaldos]; scripts `Backup-Variables.ps1` y `Restore-Variables.ps1` embebidos con `include_str!` y materializados en `<data_dir>/compass/env-backup`
  ├─ whatsapp.rs — build()
  └─ version.rs — bump() [port de versionar.bat: Tauri/Rust, todo-o-nada]
```

## Task Routing

### Por tipo de proyecto

| Menu Item | Tauri | Flutter | Kotlin | Go + Wails | Rust |
|-----------|-------|---------|--------|-----------|------|
| WindowsDev | `windows::dev` | `flutter::run_windows` | `kotlin::run_windows` | `gowails::dev` (`wails3 task dev`) | `rust::run` |
| AndroidDev | `android::dev` | `flutter::run_android` | `kotlin::run_android` | `gowails::install_debug` | oculto |
| FlutterProfile | oculto | `flutter::profile_windows` / `flutter::profile_android` (pregunta) | oculto | oculto | oculto |
| WindowsBuild | `release::manage_windows` (submenú x86/x64/ARM64) | `flutter::build_windows` | `kotlin::build_windows` | `release::manage_windows` (submenú) | `release::manage_windows` (submenú) |
| AndroidBuildArm64 | `release::manage_android` (submenú arm64/armv7/universal) | `release::manage_android` | `release::manage_android` (una card si no hay `jniLibs`) | `release::manage_android` | oculto |
| AndroidBuildAll | `android::build_aab` | `flutter::build_aab` | `kotlin::build_aab` (bundleRelease) | `gowails::build_aab` | oculto |
| AndroidBuildArmv7 | oculto | oculto | oculto | oculto | oculto |
| InstallDebug | oculto | oculto | `kotlin::install_debug` (`:<modulo>:installDebug` + lanza) | `gowails::install_debug` | oculto |
| Clean | `cleanup::manage` | `cleanup::manage` | `cleanup::manage` | `cleanup::manage` → `gowails::clean` | `cleanup::manage` |
| DebugAndroid | `android::debug` | mensaje info | mensaje info | `gowails::debug_android` (logcat por PID) | oculto |
| Signing | `signer::manage` | `signer::manage` | `signer::manage` | `signer::manage` (variables de entorno) | oculto |
| Version | `version::bump` (pkg/cargo/tauri) | mensaje info | mensaje info | `version::bump` (`build/config.yml`) | `version::bump` (Cargo.toml) |
| Git | `git::manage` | `git::manage` | `git::manage` | `git::manage` | `git::manage` |
| BuildWhatsApp | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` | `whatsapp::build` |
| ConfigureTerminal | `terminal::configure` | `terminal::configure` | `terminal::configure` | `terminal::configure` | `terminal::configure` |
| Environment | `environment::manage` | `environment::manage` | `environment::manage` | `environment::manage` | `environment::manage` |
| Salir | `Esc` → `process::exit(0)` | ídem | ídem | ídem | ídem |

Las cards **Win Release** (Tauri/Rust/Go/Go+Wails) y **APK Release** abren los
submenús de `release.rs`; en Flutter/Kotlin/Python la card de Windows sigue
compilando sólo el host (`windows-build`). La variante **AndroidBuildArmv7**
queda oculta en **todos** los tipos (las arquitecturas viven ya en el submenú),
pero se conserva para no reindexar `theme::card_hexes`; la tarea CLI
`--task android-armv7` sigue existiendo como alias.

### Go puro

- `windows-dev` → `go run .`
- `windows-build` → `go build -o bin/<nombre>.exe .`
- `go-test` → `go test ./...`
- `clean` → `go clean` + `bin/`

### Tareas globales (sin proyecto)

| Task | Ruta |
|------|------|
| `context-menu` / `opencode` | `context_menu::toggle()` (menú interactivo) |
| `vibe-setup` | `context_menu::setup()` → `ensure_tools_installed()` (pregunta por las que falten) + `install_vibe_coding()` |
| `context-menu-install` | `context_menu::install()` |
| `context-menu-uninstall` | `context_menu::uninstall()` |
| `opencode-install` | `context_menu::install_opencode()` → `install_vibe_coding()` |
| `opencode-uninstall` | `context_menu::uninstall_opencode()` → `uninstall_vibe_coding()` |
| `cline-install` | `context_menu::install_cline()` → `install_vibe_coding()` |
| `cline-uninstall` | `context_menu::uninstall_cline()` → `uninstall_vibe_coding()` |
| `kilo-install` | `context_menu::install_kilocode()` → `install_vibe_coding()` |
| `kilo-uninstall` | `context_menu::uninstall_kilocode()` → `uninstall_vibe_coding()` |
| `antigravity-install` | `context_menu::install_antigravity()` → `install_vibe_coding()` |
| `antigravity-uninstall` | `context_menu::uninstall_antigravity()` → `uninstall_vibe_coding()` |

## Path Conventions

### Android signing

| Type | Android dir | Props file |
|------|------------|------------|
| Tauri | `src-tauri/gen/android/app/` | `keystore.properties` |
| Flutter | `android/app/` | `key.properties` |
| Kotlin | `app/` o `composeApp/` (detección automática) | `keystore.properties` |
| Rust | N/A | N/A |

`signer::android_layout()` es el **único** sitio que consume esta tabla; ningún otro
módulo recalcula estas rutas (antes estaban copiadas en cuatro sitios).

### WhatsApp engine

| Type | Engine dir | Bundle output |
|------|-----------|--------------|
| Tauri | `src-tauri/whatsapp-engine/` | `src-tauri/resources/whatsapp-engine-bundle/` |
| Flutter | `whatsapp_engine/` | `resources/whatsapp-engine-bundle/` |
| Kotlin | `whatsapp_engine/` | `composeApp/resources/whatsapp-engine-bundle/` |
| Rust | `whatsapp_engine/` | `resources/whatsapp-engine-bundle/` |

### Android project detection

| Type | Path checked |
|------|-------------|
| Tauri | `src-tauri/gen/android/gradlew.bat` |
| Flutter | `android/gradlew.bat` |
| Kotlin | `gradlew.bat` (raíz) |
| Rust | N/A (siempre `false`) |

## Context Menu (Windows Registry)

### Compass (entrada simple)

| Entrada | Comando | Clave Registro |
|---------|---------|----------------|
| **Compass** | `"<pwsh>" -NoLogo -NoProfile -Command "& '<exe>' -p '%1'"` | `HKCR\Directory\shell\zz_Compass` |
| **Compass: Add .gitignore** | `"<pwsh>" -NoLogo -NoProfile -NoExit -Command "& '<exe>' --task context-gitignore --path '%1'"` | `HKCR\Directory\shell\zz_CompassGitignore` |

`zz_Compass` es prefijo de `zz_CompassGitignore`, así que la acción `.gitignore`
aparece inmediatamente después de **Compass** y antes de **Vibe Coding** en el
menú (Windows ordena las claves alfabéticamente). La clave anterior
(`zz_VibeGitignore`) se borra en cada instalación para no dejar dos acciones.

La TUI también se aloja en PowerShell: Windows atribuye la consola al perfil de
`pwsh` (icono de PowerShell 7 en Windows Terminal) en vez del icono genérico de
consola que muestra un `.exe` lanzado directo. El título lo sigue fijando
Compass con el nombre del proyecto. El comando lo construye
`compass_context_command()`; `%1` (carpeta) y `%W` (fondo) van entre comillas
simples por la misma razón que en Vibe Coding.

`compass.exe` **incrusta** `assets/icon/Icon.ico` y un bloque `VERSIONINFO`
(`FileDescription`/`ProductName` = "Compass") vía `build.rs` + `embed-resource`.
Sin esos recursos Explorer y Windows Terminal enseñan el icono genérico de
consola (idéntico al de cmd) y el perfil de la pestaña queda sin nombre.

### Vibe Coding (menú cascada: 9 agentes)

Usa el método **`ExtendedSubCommandsKey`** (el oficial de Windows para menús cascada, mismo que usa Command Prompt y PowerShell).

**Estructura de registro:**
```
HKCU\Software\Classes\Directory\shell\zz_VibeCoding
  MUIVerb = "Vibe Coding"
  Icon = "path\to\opencode.ico"
  ExtendedSubCommandsKey = "Directory\ContextMenus\VibeCoding"

HKCU\Software\Classes\Directory\ContextMenus\VibeCoding\shell
  01OpenCode     → command
  02KiloCode     → command
  03Cline        → command
  04Antigravity  → command
  05Copilot      → command
  06KiroCli      → command
  07Freebuff     → command
  08QoderCli     → command
  09Hermes       → command
```

⚠️ Las sub-entradas **no** cuelgan de `zz_VibeCoding\shell`: cuelgan de `Directory\ContextMenus\VibeCoding\shell` (ruta compartida con `Directory\Background`), y `zz_VibeCoding` sólo lleva el puntero `ExtendedSubCommandsKey`. Buscar en el sitio equivocado es lo que hacía que `install.bat` reinstallara el menú en cada ejecución.

| # | Sub-entrada | Comando | Descripción |
|---|-------------|---------|-------------|
| 01 | **OpenCode** | `"<pwsh>" -NoLogo -NoProfile -Command "Set-Location -LiteralPath '%V'; [Console]::Title = 'OpenCode'; & opencode --auto"` | Abre OpenCode en modo auto (sin prompts) |
| 02 | **Kilo Code** | `… & kilo --auto` | Abre Kilo Code en YOLO mode (aprueba todo automáticamente) |
| 03 | **Cline** | `… & cline --auto-approve true` | Abre Cline con aprobación global de herramientas sin prompts |
| 04 | **Antigravity** | `… & agy --dangerously-skip-permissions` | Abre Antigravity omitiendo todos los prompts de permisos |
| 05 | **Copilot CLI** | `… & copilot --yolo` | Abre GitHub Copilot CLI con todos los permisos (`--yolo` = `--allow-all-tools --allow-all-paths --allow-all-urls`) |
| 06 | **Kiro CLI** | `… & kiro-cli chat --trust-all-tools` | Abre Kiro CLI con confianza total de sesión (YOLO mode). El binario es **`kiro-cli`**, no `kiro` |
| 07 | **Freebuff** | `… & freebuff` | Abre Freebuff (Codebuff), sin flags: su TUI arranca a pelo en el directorio del proyecto |
| 08 | **Qoder CLI** | `… & qodercli --yolo` | Abre Qoder CLI en YOLO mode (`--yolo` = `--permission-mode bypass_permissions`). El binario real es **`qodercli`**; `qoder` es el dispatcher que instala el script |
| 09 | **Hermes** | `… & hermes` | Abre Hermes (Nous Research), sin flags: su TUI arranca a pelo |

`<pwsh>` es `cmd::shell_path()` (ruta completa, porque Explorer no hereda el
PATH de la terminal). El valor se construye en `vibe_context_command()`:
`&` ejecuta el comando —obligatorio cuando empieza por una ruta entre
comillas— y `Set-Location -LiteralPath '%V'` cambia al directorio seleccionado.
`%V` va entre comillas **simples** a propósito: Explorer lo sustituye antes de
que PowerShell parsee, así un `$` en la ruta no se interpola. Limitación
aceptada: una ruta con apóstrofo rompe `Set-Location`.

La acción `.gitignore` usa la misma forma, con `-NoExit` en vez de `cmd /k`:

```
"<pwsh>" -NoLogo -NoProfile -NoExit -Command "& '<exe>' --task context-gitignore --path '%1'"
```

⚠️ **Kiro CLI no tiene comando de instalación**: se instala con su `.exe` (o `winget install Kiro-CLI`), no desde la terminal. Por eso `install` va **vacío** en su entrada de `VIBE_TOOLS`, y eso significa explícitamente **"detectar e instalar el submenú, pero no preguntar por instalarlo"**. `ensure_tools_installed()` salta el bucle de preguntas cuando `install` está vacío y sólo avisa. Preguntar algo a lo que no se puede responder con un Enter sería un bucle sin salida. Cubierto por `a_empty_install_means_detect_only_never_ask`.

⚠️ Kiro rompe el patrón de rutas del resto: su instalador lo deja en `%LOCALAPPDATA%\Kiro-Cli\kiro-cli.exe`, **plano y sin subcarpeta `bin`**, al contrario que `%LOCALAPPDATA%\agy\bin` y `%LOCALAPPDATA%\hermes\bin`. La sonda de `install.bat` lo tiene en un mapa `$nombres` por eso.

⚠️ Qoder también rompe el patrón, y por partida doble: el instalador (`irm https://qoder.com/install.ps1 | iex`) **delega en `qodercli install`**, que coloca el ejecutable bajo `%USERPROFILE%\.qoder` (`…\.qoder\bin\qodercli\qodercli.exe`), no bajo `%LOCALAPPDATA%`. Por eso sus fallbacks usan el prefijo `HOME/` (que `fallback_candidates()` expande con `USERPROFILE`), y la sonda de `install.bat` lo comprueba como caso especial. El comando documentado es `qoder`, pero es un dispatcher `.cmd` que arranca PowerShell para delegar en `qodercli`: el menú invoca el binario directo.

⚠️ **Hermes, Antigravity, Copilot CLI y Qoder CLI no se detectan sólo por el PATH.** Los instaladores nativos de Hermes y Antigravity dejan el binario en su propia carpeta (`%LOCALAPPDATA%\hermes\bin`, `%LOCALAPPDATA%\agy\bin`) y no siempre lo agregan al PATH del usuario; winget lo deja en `%LOCALAPPDATA%\Microsoft\WinGet\Links`, y el de Qoder en `%USERPROFILE%\.qoder`. Por eso `VibeTool::fallback` declara rutas alternativas con prefijo `LOCALAPPDATA/` o `HOME/`, que `fallback_candidates()` expande. **Los npm globales no necesitan nada de esto** (npm sí modifica el PATH), y hay un test que lo comprueba (`native_installers_declare_a_fallback_outside_the_path`). Al resolver por fallback, `resolve_tool_command()` **conserva los argumentos** y entrecomilla la ruta con comillas simples (`'<ruta>' --yolo`), así el modo autónomo no se pierde y el valor no lleva comillas dobles internas que rompan el script de PowerShell.

### Instalación de las herramientas (`vibe-setup`)

`ensure_tools_installed()` detecta cuáles de las 9 faltan. **Sólo pregunta por las que tienen instalador**: Kiro CLI se avisa pero no se pregunta, porque va con `.exe`. El comando de instalación vive en `VibeTool::install`, verificado en la documentación oficial de cada producto:

| Herramienta | `install` | `install_via` |
|---|---|---|
| OpenCode | `npm install -g @opencode/cli` | `Cmd` |
| Kilo Code | `npm install -g @kilocode/cli` | `Cmd` |
| Cline | `npm install -g cline` | `Cmd` |
| Antigravity | `irm https://antigravity.google/cli/install.ps1 \| iex` | `PowerShell` |
| Kiro CLI | *(ninguno: `.exe`)* | — |
| Freebuff | `npm install -g freebuff` | `Cmd` |
| Qoder CLI | `irm https://qoder.com/install.ps1 \| iex` | `PowerShell` |
| Hermes | `iex (irm https://hermes-agent.nousresearch.com/install.ps1)` | `PowerShell` |
| Copilot CLI | `winget install --id GitHub.Copilot --exact --accept-source-agreements --accept-package-agreements` | `Cmd` |

⚠️ **`install_via` distingue familias de instalador, no shells**: todo se
ejecuta ya por PowerShell (`cmd::run_ps_in_dir_status`). Los npm se reescriben
`npm ` → `npm.cmd ` antes de lanzarlos, porque el shim `npm.ps1` lo bloquea la
ExecutionPolicy por defecto. `winget` es un `.exe` y funciona tal cual; sus
`--accept-*` evitan los prompts interactivos (en esta ejecución no hay a quién
preguntar). Los `irm | iex` ya eran sintaxis de PowerShell. `InstallVia::Cmd`
**no** usa `cmd.exe`.

⚠️ El default de la pregunta es **no**. `ask_text` imprime el default entre corchetes, así que el prompt no debe llevar su propio `[s/N]`. Con `None` no habría default y Enter devolvería "" (igual de no, pero la pregunta no se responde con la barra espaciadora). `es_si` acepta `s`/`si`/`sí`/`y`/`yes`.

⚠️ **La verificación de "ha instalado" usa `resolve_tool_command()`, no `command_exists()`**: el instalador de Google deja `agy` en `%LOCALAPPDATA%\agy\bin`, fuera del PATH, así que en el proceso actual `Get-Command agy` sigue dando falso y se reportaría como fallo siendo un éxito. Es el mismo fallback que hace que el menú funcione para Antigravity.

`setup()` reinstala el menú al final aunque ya estuviera al día, porque una herramienta recién instalada tiene que aparecer como sub-entrada nueva.

`install.bat` lanza `vibe-setup` cuando el menú falta **o** cuando la sonda detecta herramientas ausentes, así que si instalas Cline a mano y relanzas el instalador aparece sola su sub-entrada.

**Notas técnicas:**
- **Orden del submenú**: Windows ordena las sub-claves de `shell` **alfabéticamente**. Por eso las claves llevan prefijo numérico (`01OpenCode`, `02KiloCode`, ...) para forzar el orden requerido en `VIBE_TOOLS` (src/tasks/context_menu.rs). Agregar una herramienta = agregar una entrada al final de `VIBE_TOOLS` con prefijo incremental. Hay un test que verifica que los prefijos son `01, 02, 03…` sin huecos ni repeticiones y en orden, porque un prefijo duplicado pierde el orden en silencio.
- `%V` = directorio de la carpeta seleccionada (funciona con `ExtendedSubCommandsKey`; `%1` NO funciona).
- La entrada padre usa `MUIVerb` en vez de `(default)` para el texto visible.
- Las sub-entradas están en `Directory\ContextMenus\VibeCoding\shell\` (ruta compartida para `Directory` y `Directory\Background`).
- Se registran para **carpeta** (`Directory`) y **fondo** (`Directory\Background`).
- `remove_vibe_ctx_tree()` borra el árbol `ContextMenus\VibeCoding` completo en cada instalación — evita claves huérfanas de versiones previas que alteraban el orden.
- Se limpian entradas viejas individuales (`zz_OpenCode`, `zz_pCline`, `zz_pKilo`) al instalar.
- **Antigravity CLI**: binario nativo (no npm) instalado por `irm https://antigravity.google/cli/install.ps1 | iex` → `%LOCALAPPDATA%\agy\bin\agy.exe`. No siempre está en PATH; `resolve_tool_command()` hace fallback a esa ruta si `Get-Command agy` falla.
- **Hermes CLI** (Nous Research): instalador nativo `iex (irm https://hermes-agent.nousresearch.com/install.ps1)`. En Windows nativo vive bajo `%LOCALAPPDATA%\hermes\bin` (el README lo dice explícitamente). Mismo caso de fallback que Antigravity. `hermes` arranca su TUI sin flags; `hermes-acp` es el servidor ACP para editores y no va al menú.
- **Kiro CLI** (AWS): se instala con su `.exe` (`winget install Kiro-CLI` o el instalador de kiro.dev), **no hay comando de terminal**. El ejecutable queda en `%LOCALAPPDATA%\Kiro-Cli\kiro-cli.exe` — nótese que **no** sigue el patrón `…\<tool>\bin` del resto, por eso la sonda de `install.bat` lo lleva en un mapa. El subcomando para chatear es `kiro-cli chat`; `--tui` fuerza la interfaz nueva y `--legacy-ui` la antigua. `install` va vacío en `VIBE_TOOLS`, que significa "detectar, nunca preguntar por instalarlo".
- **Qoder CLI** (Qoder): instalador nativo recomendado `irm https://qoder.com/install.ps1 | iex`; existe un paquete npm de compatibilidad (`npm install -g @qoder-ai/qodercli`), pero la propia documentación lo desaconseja para instalaciones nuevas. `--yolo` equivale a `--permission-mode bypass_permissions` (aprueba todas las herramientas sin popups); es un flag oculto en `--help`, pero documentado en docs.qoder.com/cli/cli-reference. La documentación invoca `qoder --version`, pero ese `qoder` es un dispatcher `.cmd` que arranca PowerShell y delega en **`qodercli`** (el nombre del binario que descarga el instalador); el menú usa `qodercli --yolo` directo. El instalador delega en `qodercli install`, que coloca el ejecutable bajo `%USERPROFILE%\.qoder\bin\qodercli\` y configura el PATH para terminales nuevas, así que en el proceso actual hace falta el fallback `HOME/`.
- **Copilot CLI** (GitHub): se instala con `winget install --id GitHub.Copilot --exact` (paquete `GitHub.Copilot`), **no por npm**. Con `--yolo` arranca con todos los permisos concedidos (`--allow-all` = `--allow-all-tools --allow-all-paths --allow-all-urls`); sin flags la TUI pide confirmación por herramienta. ⚠️ winget **no** sigue el patrón `%LOCALAPPDATA%\<tool>\bin`: crea el enlace en `%LOCALAPPDATA%\Microsoft\WinGet\Links\copilot.exe`, y ésa es la ruta que declara `VibeTool::fallback` y la que comprueba la sonda de `install.bat`. Ojo también con el shim que instala VS Code en `%ProgramFiles%\Microsoft VS Code\bin\copilot-shim\`, que está antes en el PATH y responde igual, así que `Get-Command copilot` puede acertar aunque winget aún no haya refrescado el PATH.
- `install.bat` instala/actualiza Vibe Coding con detección de "ya instalado".
- El menú interactivo `toggle()` gestiona Compass + Vibe Coding + "Instalar todos" / "Desinstalar todos".
- **Idempotencia**: `install()` compara tanto el comando (`command_is_current()`) como el texto visible (`label_is()`), así un cambio de nombre fuerza el reinstall aunque el comando ya sea correcto.
- **Menú desactualizado por binario nuevo**: `install_vibe_coding()` escribe `CompassStamp` (mtime del exe en segundos) en `Directory\ContextMenus\VibeCoding`, y la sonda de `install.bat` lo compara con `%USERPROFILE%\.cargo\bin\compass.exe`. Si no coincide —o si falta, como en instalaciones anteriores— el menú se reinstala. Sin esto, recompilar no actualizaba nada: la sonda sólo miraba que `01OpenCode` existiera, así que una herramienta nueva (Qoder) no aparecía. Cubierto por `probe_reads_the_stamp_written_by_the_installer` y `menu_stamp_is_unix_seconds`.
- **Los cambios del registro requieren reiniciar Explorer**: `taskkill /f /im explorer.exe & start explorer.exe`.

## Key Patterns

### Contenedor de terminal (pestañas propias)

Compass es su **propia aplicación de terminal**: el menú y las tareas viven en
un único `compass.exe` y las cards abren **pestañas dentro de esa ventana**, no
consolas externas. Cada pestaña es una sesión **ConPTY** (`portable-pty`) que
corre el shell activo con `-Command`:

```
pwsh -NoLogo -NoProfile -Command "& 'C:\…\compass.exe' --task <task> --path '<proyecto>'"
```

- `container.rs::PtySession` es dueña de la sesión: un hilo lee la salida y la
  alimenta a `vt100::Parser`; `Container` guarda las pestañas, cuál está activa
  (`active: None` = la pestaña **Compass**, o sea el menú) y la última visitada
  (`last`).
- El hijo hereda **`COMPASS_EMBEDDED=1`** (`cmd::embedded()`). Con esa marca,
  `spawn_new_window_autoexit` / `spawn_new_window_interactive` ejecutan el
  comando **en la consola actual** (`shell_status`) en vez de abrir otra
  ventana. Es lo que hace que `flutter run`, `gradlew :composeApp:run`, etc.
  corran dentro de la pestaña tal cual, sin cambiar ninguna llamada de tarea.
- El dibujo usa `PseudoTerminal` de `tui-term` sobre el `vt100::Screen`; el
  resize del frame se propaga a la ConPTY (`master.resize` + `parser.set_size`).
- El pie muestra los atajos y el shell activo; al terminar el proceso, la
  pestaña conserva su última pantalla y se marca con `✓` / "proceso finalizado".
- **Barra de pestañas** (`draw_bar`, 3 líneas: `BAR_HEIGHT`): una caja
  redondeada por pestaña (`Block` + `BorderType::Rounded`), separadas por una
  columna vacía. La activa va en `brand` + BOLD con `hover_bg` de fondo; las
  inactivas llevan borde `border` y texto normal; las terminadas, tonos
  `muted`. La primera pestaña se llama **Compass** (el menú); las de tarea
  muestran `N título`.
- **Botón de cierre**: cada pestaña de tarea lleva `×` a la derecha
  (`self.closes` guarda su zona clicable: el glifo y los dos huecos que lo
  rodean, a todo el alto de la barra). Un clic lo cierra sin exigir que la
  pestaña esté activa; cerrar una pestaña que no es la activa **no mueve el
  foco** (sólo se corrigen los índices `active`/`last`), y cerrar la activa
  pasa al vecino. La zona se calcula en `draw_tab_bar` y tiene que cubrir la
  celda donde el `Paragraph` dibuja el glifo: un desfase de una columna deja el
  `×` visible pero muerto al clic (lo cubre
  `container::tests::close_zone_covers_the_rendered_glyph`).

| Tecla | Acción |
|-------|--------|
| `F12` / `Alt+M` | menú ↔ última pestaña (no cierra nada) |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | pestaña siguiente / anterior |
| `Ctrl+W` | cierra la pestaña activa (y su árbol de procesos) |
| `Alt+1..9` | salta a la pestaña N |
| resto | se codifica a xterm (`encode_key`) y se escribe en la ConPTY |

Detalles que importan:

- **`close()` mata el árbol**: `taskkill /PID <pid> /T /F` + `child.kill()`;
  matar sólo al `pwsh` de la pestaña dejaría vivos a `cargo`/`gradlew`/`flutter`
  escribiendo en la ConPTY. `process::exit` no ejecuta destructores, así que
  `run()` llama a `panes.close_all()` antes de salir.
- **Salir con pestañas abiertas pide confirmación**: `Esc` en el menú no sale
  directamente si el contenedor tiene pestañas de tarea (`Container::tab_count`)
  —se matarían sus árboles—. `MenuState.quit_confirm` abre un aviso modal con la
  cuenta y cuántas siguen con un proceso vivo (`running_count`), y exige
  `Enter` para salir; `Esc` (o `n`) cancela y deja el menú como estaba. Sin
  pestañas, `Esc` sale de inmediato. El modal reutiliza el estilo del de temas
  (`Clear` + caja redondeada, borde en `warning`) y mientras está abierto el
  teclado es suyo. Cubierto por `menu::tests::quit_confirm_warns_about_open_tabs`.
- **Cierre de la ventana anfitriona** (`utils/console_guard.rs`): la `×` del
  terminal no pasa por el menú, así que el proceso moriría sin cerrar las
  pestañas. `install()` registra `SetConsoleCtrlHandler` y el contenedor
  sincroniza la cuenta con `set_open_tabs()` y los PIDs con
  `register_pane()`/`unregister_pane()`.
  - **conhost**: con pestañas abiertas se deshabilita `SC_CLOSE` del menú del
    sistema (`GetSystemMenu` + `EnableMenuItem(MF_GRAYED)` + `DrawMenuBar`), así
    que la `×` no cierra; se rehabilita al cerrar la última. La salida segura es
    `Esc` → modal → `Enter`.
  - **Windows Terminal**: el host cierra la ConPTY y la app **no puede vetarlo**
    (microsoft/terminal#6549; `EnableMenuItem` no afecta a la `×` de su
    pestaña). Al recibir `CTRL_CLOSE_EVENT` el manejador mata los árboles de las
    pestañas (`taskkill /T`) y muestra un aviso nativo (`MessageBoxW`) con la
    cuenta; el cierre sigue adelante. Es lo máximo que permite el host.
  - `CTRL_LOGOFF_EVENT`/`CTRL_SHUTDOWN_EVENT` solo matan los árboles, sin
    diálogo (no se debe bloquear un apagado).
  - Sin `install()` (tests, tareas CLI) el módulo es inerte: `set_open_tabs()`
    solo guarda el contador y no toca Win32.
- **El ratón se reenvía a la pestaña activa**: un clic en la barra lo consume el
  contenedor (selecciona/cierra), y el resto se codifica como **SGR 1006** con
  `encode_mouse_sgr` y se escribe en la ConPTY, con la fila **relativa al
  cuerpo** (`row - BAR_HEIGHT`). ConPTY traduce esas secuencias a
  `MOUSE_EVENT_RECORD`s para el hijo si éste habilitó `ENABLE_MOUSE_INPUT`
  (crossterm lo hace con `EnableMouseCapture`), así que el hover y los clics
  funcionan en los submenús (Firmas, Git) que corren dentro de una pestaña. Se
  reenvía **siempre** (sin mirar el modo de ratón de vt100) porque una shell
  como `pwsh` ignora las secuencias: no hay basura en pantalla. Cubierto por
  `container::tests` (`mouse_is_forwarded_relative_to_the_body`,
  `mouse_drag_and_wheel_use_the_extended_codes`,
  `mouse_on_the_bar_is_not_forwarded`).
- **Límite del e2e**: crossterm lee el ratón de los registros de consola de
  Windows, así que una ConPTY de prueba no puede inyectarlo y el hover de los
  submenús se verifica en una ventana real.
- **Plan B**: si abrir la ConPTY falla, la card cae a la ventana externa de
  siempre (`spawn_in_window`); `COMPASS_EXTERNAL_WINDOWS=1` recupera ese
  comportamiento para todas las cards sin recompilar.
- El bucle de la pestaña usa `poll_input()` con `FRAME` (33 ms) y vacía la cola
  de eventos completa por fotograma; leer uno por iteración limitaría la
  escritura rápida a ~30 teclas/s.

Cubierto por `container::tests` (codificación de teclas, la zona clicable del
`×` contra el glifo realmente dibujado en un `TestBackend`, y
`session_renders_its_output`, que abre una ConPTY real) y por
`tests/container_e2e.rs` (ver "Testing").

### Ventanas de terminal (fallback)
Dos lanzadores de ventana, ambos con `CREATE_NEW_CONSOLE`:

| Variante | Interno | Uso |
|----------|---------|-----|
| `spawn_new_window_autoexit()` | `[Console]::Title = '<título>'; & <command>` | Ventana externa para `compass --task …` que se cierra al terminar |
| `spawn_new_window_interactive()` | lo anterior + `; Read-Host 'Presiona Enter para cerrar' \| Out-Null` | Dev servers interactivos cuando no hay contenedor |

⚠️ **Con una pestaña del contenedor de por medio, ninguno abre ventana**: si
`COMPASS_EMBEDDED` está definido, ambos ejecutan el comando en la consola actual
y vuelven. Las ventanas externas quedan para el fallback y para
`COMPASS_EXTERNAL_WINDOWS=1`.

Ambos usan **PowerShell** (`cmd::shell()`): `pwsh` si está instalado, con
fallback a `powershell` 5.1. El icono de una consola lo aporta el ejecutable que
la crea, así que alojar la ventana en `pwsh.exe` es lo que cambia el icono
clásico de cmd por el de PowerShell 7 (en conhost; con Windows Terminal como
terminal por defecto se ve el de WT).

Detalles que importan:

- **`-NoLogo -NoProfile -Command`** (`window_args`): arranque determinista y
  rápido, y el perfil de PSReadLine del usuario no interfiere con el teclado de
  los procesos hijos (`flutter run` responde a `r`/`q`, gradle run también).
- **El `&` (operador de llamada) es obligatorio**: los comandos que empiezan por
  una ruta entre comillas (`"C:\…\compass.exe" --task …`) se parsean como una
  cadena y no se ejecutan sin él. Lo añade `window_script()`, así que los
  llamadores siguen pasando `flutter run -d windows`, `.\gradlew.bat …`, etc.
- **Sin `raw_arg` ni sintaxis de cmd**: el script va como **un solo argumento**
  (Rust lo entrecomilla) y PowerShell lo recibe íntegro. Desaparecen `title`,
  `&` de cmd y `& pause`; el `pause` se sustituye por `Read-Host`.
- `[Console]::Title` funciona igual en conhost y Windows Terminal, al contrario
  que `$Host.UI.RawUI.WindowTitle`.

### Builds: corren en la ventana actual (`cmd::run_build_and_reveal`)
Los builds **no abren una segunda ventana**. Llaman a
`cmd::run_build_and_reveal(command, dir, folder, exts, rename)`:

1. Ejecuta `command` con el **shell activo** (`shell_status` antepone `& ` y
   lanza `pwsh -NoProfile -Command`), heredando stdio, **en la ventana que el
   TUI ya abrió** (la de `spawn_new_window_autoexit`).
2. Si termina bien, busca el artefacto más reciente de `folder` en Rust
   (`newest_artifact`, recursivo y filtrando por extensión), lo renombra a
   `<rename><ext>` si `rename` viene, y lo abre en el explorador.
3. Espera un Enter (`ui::pause()`) para que se pueda leer la salida.

Así sólo hay **una** ventana y es la rápida. Antes se generaba un script de
PowerShell con la lógica de revelado incrustada y se abría con `powershell -NoExit -Command`:
lento y con la lógica de
"artefacto más reciente" duplicada en dos sintaxis.

`cmd::run_in_current_window(command, dir)` es la variante sin revelado, para
procesos largos que no producen artefacto (`rust::run` → `cargo run`).

El revelado usa `explorer.exe /select,"<ruta>"` (con `raw_arg`, para que Explorer
reciba las comillas alrededor de la ruta). Si no encuentra artefacto, abre `folder`.

⚠️ **Los separadores se normalizan a `\` antes del Explorer** (helper
`windows_path_for_explorer`): los literales `join("a/b/c")` conservan las `/` del
literal, y un path con separadores mezclados (`D:\P\src-tauri/gen/android/...`)
hace que `explorer /select` **ignora el argumento y abre el Escritorio**. Con
backslashes puros sí selecciona. La ruta impresa en consola (`[INFO] Artefacto`)
se veía bien porque solo era el display; el parseo de Explorer era lo fallido.

| Task | folder | exts |
|------|--------|------|
| `windows::build` (Tauri) | `src-tauri/target/release/bundle` | `desktop_artifact_exts()` |
| `android::{build_arm64,build_armv7,build_universal}` | `src-tauri/gen/android/app/build/outputs/apk` | apk |
| `android::build_aab` | `src-tauri/gen/android/app/build/outputs/bundle/release` | aab |
| `flutter::build_windows` | `build` | `desktop_artifact_exts()` |
| `flutter::build_apk` | `build/app/outputs/flutter-apk` | apk |
| `flutter::build_aab` | `build/app/outputs/bundle/release` | aab |
| `kotlin::build_windows` | `<modulo>/build/compose/binaries/main` | `desktop_artifact_exts()` |
| `kotlin::build_apk` | `<modulo>/build/outputs/apk/release` | apk |
| `kotlin::build_aab` | `<modulo>/build/outputs/bundle/release` | aab |
| `rust::build` | `target/release` | `desktop_artifact_exts()` |

### Rename build artifact (nombre + versión)
Todos los builds **release** de Android (APK/AAB) renombran el artefacto resultante a `<AppName>-v<version>.<ext>` usando `run_build_and_reveal(..., Some("AppName-v1.0.0"))`.

| Fuente | Nombre | Versión | Ejemplo |
|--------|--------|---------|---------|
| Tauri | `tauri.conf.json` → `productName` | `version` | `School_Tracker_Pro-v1.1.0.apk` |
| Flutter | `pubspec.yaml` → `name` | `version` (sin `+build`) | `my_app-v2.0.0.aab` |
| Kotlin | `build.gradle.kts` → `versionName` | `res/values/strings.xml` → `app_name` | `SongSync-v1.0.0.apk` |

El sanitizado reemplaza espacios y caracteres especiales por `_`. Los builds de **Android** (APK/AAB) renombran a `<AppName>-v<version>.<ext>` (sanitizado). A partir de ahora los builds de **escritorio** (Tauri, Flutter, Kotlin, Rust y Go+Wails) también renombran el artefacto revelado a `<nombre real> v<version>.<ext>` (sin sanitizar, con espacios), p. ej. `School Tracker v1.0.1.exe`. En Go+Wails el nombre sale de `build/config.yml` (`info.productName` + `info.version`).

**Importante**: la llamada a Explorer es `explorer.exe ("/select,`"" + $f.FullName + "`"")` — el path debe ir
**entre comillas dobles embebidas** (`/select,"ruta con espacios"`). `explorer.exe /select,<path>` sin esas comillas
falla el parser de Explorer cuando la ruta tiene espacios y abre **Documentos** en su lugar.

### Release por arquitectura (`tasks/release.rs`)

La card **Win Release** y la card **APK Release** no compilan directamente: abren
un **submenú TUI** con `tasks/submenu.rs` (mismo header, fila de sección, rejilla
adaptativa de cards de 3 líneas, halo de hover, scroll y pie que el menú padre).
Título de sección `RELEASE · WINDOWS` / `RELEASE · ANDROID`, acento `DEV` y
`warn_android = false` en el de Windows.

| Submenú | Cards (en orden) |
|---------|------------------|
| `manage_windows` | 🖥 Windows x86 (`32 bits · PCs y VMs antiguas`) · 🖥 Windows x64 (`64 bits · uso general`) · 🖥 Windows ARM64 (`Snapdragon X · bajo consumo`) · ✅ Todos · ↩ Volver |
| `manage_android` | 📱 Android arm64 (`arm64-v8a · moviles modernos`) · 📱 Android armv7 (`armeabi-v7a · equipos antiguos`) · 📦 Android universal (`Una APK con todas las ABIs`) · ✅ Todos · ↩ Volver |

- **Kotlin sin `jniLibs`** (sin `.so` bajo `<modulo>/src/main/jniLibs`): las tres
  APK serían idénticas, así que el submenú muestra **una sola** card universal
  (`Sin libs nativas · sirve para todas las ABIs`) + Volver.
- **Todos** compila en serie y **corta en el primer fallo**; revela y pausa una
  sola vez al final (Go+Wails abre `bin/`, el resto resalta el último artefacto).
- Los builds individuales devuelven `Result<Vec<PathBuf>>` (o `Result<PathBuf>`)
  y **no** revelan ni pausan; eso lo hace `finish()` una vez por ejecución.
- Un fallo de build se imprime, `ui::pause()` y **se vuelve al submenú** (no se
  cierra). `Esc`/`q` vuelve al menú principal. El loop es el canónico de
  `signer::manage`: `restore_terminal` → ejecutar → `setup_terminal` →
  `set_console_title`.

**Soporte:**

| Tipo | Windows | Android |
|------|---------|---------|
| Tauri | x86/x64/ARM64 (`build --target <triple> --bundles nsis`) | arm64/armv7/universal (`android build --target … --apk`) |
| Rust | x86/x64/ARM64 (`cargo build --release --target`) | — |
| Go puro | x86/x64/ARM64 (`GOOS=windows` + `GOARCH`) | — |
| Go + Wails | x86/x64/ARM64 (`wails3 task build/package ARCH=…`) | arm64/armv7/universal (`build_release_apks`) |
| Flutter | — (sólo host, `windows-build`) | arm64/armv7/universal |
| Kotlin | — (sólo host, `windows-build`) | universal, o las tres si hay `jniLibs` |
| Python | — (`windows-build`) | — |

**Nombres de artefacto:**

| Salida | Nombre |
|---|---|
| Tauri/Rust Windows x64 | `<name> v<ver>.exe` |
| Tauri/Rust Windows x86/ARM64 | `<name> v<ver> x86.exe` / `<name> v<ver> arm64.exe` |
| Go | `bin/<exe>-x86.exe` / `-x64.exe` / `-arm64.exe` |
| GoWails | `<Product> v<ver>.exe`, `... x86.exe`, `... arm64.exe`, `... [x64\|arm64] installer.exe` |
| Tauri Android | `<Name>-v<ver>.apk`, `...-arm64.apk`, `...-armv7.apk` |
| Flutter Android | `bin/<name>-v<ver>.apk`, `...-arm64.apk`, `...-armv7.apk` |
| Kotlin Android | `<name>-v<ver>.apk`, `...-arm64.apk`, `...-armv7.apk` |
| GoWails Android | `<Product>-v<ver>-arm64.apk`, `-armv7.apk`, `-universal.apk` |
| AAB (todos) | `<name>-v<ver>.aab` |

**Limitaciones y decisiones:**

- **Flutter/Kotlin no tienen submenú de Windows**: `flutter build windows` y
  `gradlew …packageDistributionForCurrentOS` sólo compilan el host, así que su
  card delega en el build de siempre.
- **Tauri x86/ARM64 puede no tener NSIS**: si el bundle falla se entrega el
  portable más reciente de `src-tauri/target/<triple>/release/*.exe` con un
  aviso. ARM64 necesita las herramientas ARM64 de Visual Studio; si faltan, el
  mensaje lo dice (vale igual para Rust).
- **Go cruzado usa `CGO_ENABLED=0`**: el `.syso` del icono sólo se embebe para
  `amd64` (`icon_for_arch`), porque un `.syso` es específico de arquitectura.
- **Tauri Android pasa las ABIs por entorno del proceso de Gradle**
  (`ORG_GRADLE_PROJECT_abiList/archList/targetList`) vía `run_shell_status`, sin
  `env::set_var` global; sólo `android::dev` sigue fijando arm64 en el entorno
  porque la ventana hija lo hereda.
- **Flutter reescribe `app-release.apk` en cada build**, así que cada APK se
  copia a `bin/` **inmediatamente**; el lote universal compila primero el
  universal y luego `--split-per-abi` (ignora `x86_64`).
- **Kotlin usa un init script de Gradle** (`beforeProject` → `afterEvaluate`)
  para fijar `abiFilters` o habilitar splits + `universalApk` sin editar el
  `build.gradle.kts` del proyecto; el instalador sólo instala la universal.
- **`release.rs` no usa `run_build_and_reveal*`**: esos helpers revelan y pausan
  por build. Los builders por arquitectura resuelven la ruta exacta
  (`newest_file_in`, no recursivo en `target/<triple>/release`, para no coger
  exes de build-scripts) y `finish()` hace `cmd::show_artifact(...)` +
  `ui::pause_for(10)` una sola vez.

CLI: `--task windows-release`, `--task android-release` y `--task android-aab`;
`windows-build`, `android-arm64`, `android-armv7` y `android-all` se conservan
como alias de los builds antiguos.

Cubierto por `tasks::release::tests`: entradas por tipo (Todos al final),
Kotlin sin `jniLibs` = una sola card, claves de `theme::CARD_KEYS` válidas y
distintas, triples/GOARCH reales, `icon_for_arch` sólo amd64, `has_native_libs`
y el contenido del init script de ABIs.

### npm en Windows
`cmd::run_npm_install()` usa `cmd::run_ps_status("npm.cmd install", …)`. El
`.cmd` es obligatorio: en PowerShell `npm` resolvería al shim `npm.ps1`, que la
ExecutionPolicy bloquea por defecto.

### Gradle en Windows (KMP)
Los comandos de Gradle se ejecutan como `.\gradlew.bat <args>` dentro del script
de PowerShell. PowerShell **no busca en el cwd**, así que `gradlew.bat` a secas
no resuelve: hace falta el prefijo `.\` (y `& .\gradlew.bat …` cuando el
comando va tras un encadenamiento).

### CLI de Tauri (`utils/tauri.rs`)
Un `node_modules` compartido entre máquinas puede romper el CLI de Tauri por partida doble, y las dos cosas se evitan así:

| Síntoma | Causa |
|---------|-------|
| `'tauri' is not recognized…` | `node_modules/.bin/tauri` depende del shim que generó npm; PowerShell no lo resuelve desde el cwd. |
| `Cannot find native binding` | `@tauri-apps/cli` delega en una *optional dependency* por plataforma; el binding de Windows puede faltar ([npm/cli#4828]). |

- `tauri::command(ctx, args)` emite `node node_modules/@tauri-apps/cli/tauri.js <args>`, nunca `tauri` a secas. Cae a `npx.cmd tauri` si el paquete no está (el `.cmd` explícito evita el shim `npx.ps1`, bloqueado por ExecutionPolicy).
- `tauri::ensure(ctx)` llama a `npm install` si falta `node_modules`, el paquete, o el binding de `platform::tauri_native_pkg()`. Se llama **antes** de `command()`.
- **`npm install` reemplaza** el binding del otro sistema, no lo añade: un `node_modules` en ping-pong entre sistemas pide un install por uso. Inherente al modelo de optional deps.

⚠️ **La ruta va relativa y sin comillas**: el comando se incrusta en el script
de PowerShell como `& node node_modules/…` y todos los lanzadores ponen el cwd
en la raíz del proyecto, así que resuelve. Una ruta absoluta sólo añadiría
comillas y espacios que escapar.

Cubierto por `utils::tauri::tests` (los tres casos: fallback a `npx.cmd`, invocación por `node`, y detección del binding de otra plataforma).

### Configurar Terminal (perfil PSReadLine)
`terminal::configure()` configura el **shell activo** (`cmd::shell()`): `pwsh`
7 si está instalado, Windows PowerShell 5.1 si no. La tarjeta muestra cuál es y
su ruta completa. El perfil se escribe donde el propio shell diga
(`<shell> -NoProfile -Command '$PROFILE'`):

| Shell | Perfil |
|-------|--------|
| `pwsh` | `Documents\PowerShell\Microsoft.PowerShell_profile.ps1` |
| `powershell` | `Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1` |

Si la sonda de `$PROFILE` falla, cae a la carpeta estándar de esa versión
(`profile_subdir`), que es lo único que distingue a las dos.

Orden del flujo: detectar versión **con el shell activo** → si es 5.1 y no hay
predictivas, `Install-Module` → volver a detectar → escribir el perfil que
corresponda.

| Versión tras el paso anterior | Perfil | Qué da |
|---------|--------|--------|
| ≥ 2.2.0 | `PROFILE_PREDICTIVE` | Sugerencias inline (`-PredictionSource History`), Tab acepta sugerencia, → acepta palabra |
| < 2.2.0 | `PROFILE_LEGACY` | Tab = `MenuComplete`, flechas = búsqueda en historial, sin campana, ToolTips |
| no instalado | `PROFILE_SIN_MODULO` | Sólo explica cómo instalarlo, sin emitir comandos |

⚠️ **En pwsh no se llama a `Install-Module`**. PSReadLine viene incluido
(≥ 2.3) y una copia de PSGallery puede sombrear o romper la bundled, sobre todo
cuando el perfil se carga en cada arranque. Con 5.1 se mantiene el flujo (la
2.0.0 de serie no sirve) y por eso el que decide es el shell, no la versión.

⚠️ **2.2.0 es el corte**: es la primera versión con *predictive intellisense*. Windows PowerShell 5.1 trae la **2.0.0** de serie, que no conoce ni `-PredictionSource` ni las Functions `AcceptSuggestion` / `AcceptNextSuggestionWord`. Emitir el perfil moderno ahí llenaba cada terminal nueva de `NamedParameterNotFound` y `ParameterArgumentValidationError`. La versión se lee con `Get-Module -ListAvailable` en `<shell> -NoProfile`, y se compara como `major*100 + minor`: `2.11.4` (211) es **más nueva** que `2.2.0` (202), cosa que un `>=` sobre el texto daría al revés.

⚠️ **La sonda se ejecuta con el MISMO shell que usa Compass**, nunca con el
otro: el `PSModulePath` de PowerShell 7 y 5.1 no coincide, y una instalación en
`CurrentUser` (`Documents\PowerShell\Modules`) puede existir para uno y no
aparecer desde el otro. Diagnosticar con el shell equivocado hace creer que el
módulo falta.

### La actualización (`upgrade_psreadline`)
Sólo se usa con Windows PowerShell 5.1. `Install-Module PSReadLine -Scope
CurrentUser -Force -AllowClobber -Confirm:$false`, con tres detalles que hacen
falta en esa versión:

- **TLS 1.2** en el mismo `-Command`: PSGallery responde *"Could not create SSL/TLS secure channel"* porque 5.1 negocia TLS 1.0. `SecurityProtocol` es de proceso, así que tiene que ir en la misma invocación.
- **`-Force -AllowClobber -Confirm:$false`**: sin ellos pregunta por consola al reemplazar la versión instalada.
- **`cmd::run_timeout`** (stdin a `/dev/null` + techo de 300 s): la primera vez descarga el proveedor NuGet. Con el stdin cerrado, cualquier prompt lee EOF y aborta en vez de colgar la TUI.

Sólo se llama cuando el shell es 5.1 y `is_predictive()` es falso, así que en una
máquina con pwsh —o al día— no se toca la red.

⚠️ **`-PredictionSource` va dentro de un `try`/`catch`**. PSReadLine lanza `ArgumentException` si se activa sin consola interactiva con soporte de terminal virtual — o sea, con la salida redirigida (`powershell -Command`, scripts, o cualquier herramienta que lance PowerShell capturando stdout). Como el perfil se carga también en esos casos, sin el `catch` el error reaparece, sólo que cambiado. Verificado: con el `catch`, `powershell -Command 'exit 0'` deja **0 bytes en stderr**.

⚠️ **El perfil se escribe con BOM UTF-8** (`write_utf8_bom`). PS 5.1 lee un `.ps1` sin BOM como ANSI/CP1252, así que cada acento se parte en dos caracteres (`ú` = bytes `C3 BA` → 195 y 186). Hoy son sólo comentarios, pero cualquier ruta con acento en un alias dejaría de funcionar. El BOM forma parte de la comparación de idempotencia: un perfil correcto *sin* BOM tiene que reescribirse.

Antes de sobrescribir se copia el perfil previo a `Microsoft.PowerShell_profile.ps1.compass-bak` . Si el contenido ya coincide, no se toca el archivo.

Cubierto por `tasks::terminal::tests` (9 tests): parseo de versión, decisión de actualizar, elección de perfil, **los nombres del perfil legacy existen todos en la 2.0.0** (lista real sacada con `Get-Command`), ausencia de nombres predictivos, `-PredictionSource` protegido por `try`/`catch`, BOM (incluido que su presencia forma parte de la comparación) y la carpeta de perfil de cada shell.

### Entorno — respaldo y restauración (`tasks/environment.rs`)

La tarjeta **Entorno** de `CONFIGURACIÓN` migra el proyecto *Variables
Backupper* (PowerShell). En vez de reescribir 110 KB de PowerShell en Rust, los
scripts se **incrustan en el binario** con `include_str!` y se escriben en
`<data_dir>/compass/env-backup/` (`%LOCALAPPDATA%\compass\env-backup`) antes de
usarlos. `materialize()` sólo reescribe un script si falta o si su contenido
cambió, así que una recompilación de Compass actualiza los scripts sin dejar
versiones viejas.

| Card | Args al script | Qué hace |
|------|----------------|----------|
| 💾 Backup completo | `-Destino <dir> -ConservarN <n>` | Variables de usuario/sistema, inventario de herramientas y archivos de configuración (`.gitconfig`, SSH, VS Code, keystores…) |
| 📋 Solo inventario | `… -SoloInventario` | Herramientas y versiones, **sin copiar archivos** |
| ♻ Restaurar entorno | `-CarpetaBackup <dir> [-RestaurarArchivos]` | Restaura variables, PATH y archivos; genera `pre-restore_*` para el rollback |
| 🔍 Validar backup | `-CarpetaBackup <dir> -SoloValidar` | Diff de lo que cambiaría, sin tocar nada |
| 📂 Abrir respaldos | — | Abre `…\env-backup\Backup` en el explorador |
| ↩ Volver | — | Regresa al menú |

- Las preguntas que el script ya hace (`Read-Host`: destino alternativo,
  confirmar unidad `C:`, borrar respaldos antiguos, restaurar archivos) siguen
  funcionando porque el proceso hereda stdio de la pestaña. Compass sólo
  adelanta **destino** (default `…\env-backup\Backup`) y **cuántos conservar**
  (default 1, acotado a `1..=99`) antes de lanzarlo.
- La restauración propone como default **el respaldo más reciente** de
  `…\env-backup\Backup` (`latest_backup()`: orden lexicográfico, que sirve
  porque el sufijo es `_yyyy-MM-dd_HHmmss`); vacío = el script lista lo que
  encuentre.
- Se ejecutan con `cmd::run_ps_file_status` → `pwsh -NoLogo -NoProfile
  -ExecutionPolicy Bypass -File <script> <args…>`, con cwd en
  `…\env-backup`: `-ExecutionPolicy Bypass` es obligatorio porque el perfil del
  usuario o una política de máquina puede bloquear scripts sueltos. Los
  argumentos van uno a uno (sin shell), así que las rutas con espacios no se
  parten.
- ⚠️ **Variables de sistema requieren Administrador**: sin elevación el script
  avisa y respalda/restaura sólo las de usuario. Compass lo informa en la
  cabecera de la acción; no relanza con UAC.
- ⚠️ **Los respaldos son sensibles** (claves SSH privadas, tokens de Git/npm,
  contraseñas de Maven, export del registro). Por eso el destino por defecto
  vive **fuera del proyecto** (`%LOCALAPPDATA%`), y conviene copiarlo a otra
  unidad o cifrarlo antes de subirlo a la nube.
- El submenú usa `tasks/submenu.rs`, igual que Firmas y Git (restaurar la
  consola → ejecutar → volver a montar el TUI), y el título de ventana se fija
  de nuevo tras cada acción.

Cubierto por `tasks::environment::tests`: los scripts embebidos contienen los
marcadores reales (`Invoke-Native`, `-Destino`, `-CarpetaBackup`,
`pre-restore_`), `materialize()` es idempotente y corrige un script corrupto,
`latest_backup()` elige el más nuevo, las entradas cubren cada acción una sola
vez con `Volver` al final y claves de paleta distintas, y `is_yes()` acepta
`s`/`si`/`sí`/`y`/`yes`.

### KMP Desktop (Hot Reload)
`kotlin::run_windows()` ejecuta `:composeApp:run` con Hot Reload incorporado.

### Firmas Android — tarjeta unificada (`signer.rs`)

Las antiguas tarjetas **Configure** + **Keystore** + **SHA1** eran tres vistas del
mismo dominio: el layout del módulo Android estaba calculado en **cuatro** sitios y
el parseo de `.properties` duplicado en dos. En **Flutter/KMP** además "Configure" y
"Keystore" llamaban a la **misma** función, y `keystore.rs` asumía `composeApp/` para
Kotlin (el respaldo miraba en la carpeta equivocada en un KMP con módulo `app/`).
Todo eso vive ahora en **`signer.rs`**, con una sola tarjeta **Signing** y un submenú:

| Opción | Qué hace |
|--------|----------|
| 1. Estado de la firma | **Autodetección**: módulo, `.properties`, keystore (existe/tamaño), alias, si el Gradle está parcheado y las huellas debug+release |
| 2/3/4. Crear / usar / manual | Crean el keystore, escriben el `.properties` y parchean Gradle |
| 5. Huellas | SHA-1/SHA-256/MD5 de debug y release (`keytool -list -v`) |
| 6. Secretos de GitHub Actions | Escanea `.github/workflows/**/*.yml` buscando `secrets.NOMBRE` y propone cada valor; ofrece guardar el reporte |
| 7/8/9. Respaldar / restaurar / listar | Copia keystore + `.properties` a `keystore-backups/<timestamp>/` |
| 10. Verificar APK | `apksigner verify --print-certs` del APK más reciente, comparado con el keystore configurado |

**Piezas clave** (todas `pub` en `signer.rs`, que es la fuente de verdad):

- `android_layout(ctx) -> Option<AndroidLayout>` — **único** sitio que conoce
  `android/app` (Flutter), `src-tauri/gen/android/app` (Tauri) y `kotlin_android_dir`
  (KMP, distingue `app`/`composeApp`). Reemplaza las cuatro copias previas.
- `Props::parse`/`render`/`read` — parser tolerante de `storeFile`/`storePassword`/
  `keyPassword`/`keyAlias` y sus alias (`storepass`/`keypass`/`alias`). Lo comparte
  `kotlin::release_signing_env`.
- `detect(ctx) -> Option<SigningInfo>` y `gradle_signing_status(dir)` —
  autodetección; `resolve_keystore` acepta `storeFile` absoluto o relativo.
- `base64_encode` — implementado a mano (sin dependencias) para el secreto
  `ANDROID_KEYSTORE_BASE64`.
- ⚠️ `guess_secret` comprueba **las contraseñas antes que el keystore**: el nombre
  `ANDROID_KEYSTORE_PASSWORD` contiene `KEYSTORE`, así que mirar el keystore primero
  mandaría el base64 en lugar de la contraseña. Cubierto por
  `secret_guessing_prefers_password_over_keystore`.
- `find_apksigner`/`find_newest_apk` — `apksigner` de la build-tools más reciente;
  el APK más nuevo entre `build/app/outputs` (Flutter) y `<módulo>/build/outputs/apk`.

**Reporte de secretos**: tras mostrar los valores (con aviso de que son secretos),
ofrece guardarlos en `github-secrets.txt`; si el proyecto tiene `.gitignore` y no lo
menciona, lo añade para no commitearlo por accidente.

`signer::patch_build_gradle()` soporta tanto `build.gradle.kts` (Kotlin Script) como
`build.gradle` (Groovy), inyectando:
1. Properties loader (variables desde `keystore.properties`/`key.properties`)
2. `signingConfigs { release { ... } }`
3. `signingConfig` en build type release

Para Kotlin release builds (`build_apk`/`build_aab`), el tool también exporta las variables de entorno `RELEASE_*` que many `build.gradle.kts` usan en su `signingConfigs`, permitiendo que la firma funcione sin modificar el archivo del proyecto.

CLI: `--task sign` (y los alias `signing`/`sha1`/`keystore`) abren el mismo submenú.

### Limpieza — tarjeta unificada (`cleanup.rs`)

Las tarjetas **Clean Cache** y **Clean Project** se solapaban: la segunda era la
variante "agresiva" de la primera y en Flutter/KMP borraban casi lo mismo. Al
auditarlas aparecieron fallos de verdad:

- La "agresiva" de **Kotlin** borraba **menos** que la rápida (no tocaba
  `build/`, `<módulo>/build/` ni `.kotlin/`).
- La "agresiva" de **Rust** no hacía nada extra: borraba `target/` y luego
  lanzaba `cargo clean`, ya vacío.
- `clean_project` tenía **pasos muertos por orden**: borraba `target/` y luego
  `target/.cargo-lock`; `node_modules/` y luego `node_modules/.vite`; y
  `gen/android/app/build` en el paso 2 para re-borrarlo en el paso 8 (el
  `ProgressBar` contaba 8 cuando sólo 5 hacían algo). El tamaño de `gen/` no se
  sumaba al total liberado.
- `remove_dir(...).ok()` se tragaba los errores: un `target/` en uso (típico en
  Windows) salía como `[OK]` sin haber borrado nada.

Ahora hay **una tarjeta Clean** con un submenú:

| Opción | Qué hace |
|--------|----------|
| 1. Rápida | Borra builds y caches baratos (`quick_targets`); **conserva** `node_modules/` y el `target/` completo |
| 2. Completa | Delega en el clean por tipo (`clean_project` / `flutter::clean` / `kotlin::clean` / `rust::clean`); borra dependencias |
| 3. Elegir qué borrar | **Dry-run interactivo**: lista lo que existe y deja marcar con `MultiSelect`; pide confirmación extra si hay dependencias |
| 4. Ver tamaños | Mide lo que liberaría cada opción sin borrar nada |

- `quick_targets(ctx)` / `full_targets(ctx)` son la **única** lista de "qué se
  borra"; el reporte y la rápida salen de ahí, así que no se desincronizan.
- La **completa reutiliza** los `clean()` por tipo, no reimplementa el borrado.
- `selectable_targets(ctx)` alimenta el modo interactivo con **candidatos sin
  solapamiento** (para no contar dos veces un tamaño): ofrece el `target/` entero
  en vez de `debug`+`release`, y no lista subcarpetas de `node_modules/`.
  `Candidate.deps` marca lo caro de restaurar. Cubierto por
  `interactive_candidates_do_not_overlap` y `dependency_candidates_are_flagged`.
- `purge(label, path)` centraliza el borrado y **reporta el fallo** en vez de
  silenciarlo.
- Se eliminó `npm cache clean --force`: limpiaba la caché **global** de npm (de
  toda la máquina), no la del proyecto, y sólo servía para que el siguiente
  `npm install` fuese más lento. Ninguno de los `clean()` por tipo lo hacía.
- ⚠️ La rápida **nunca** debe incluir `node_modules/` ni el `target/` completo:
  cubierto por `quick_clean_never_removes_installed_dependencies`.

CLI: `--task clean` (y los alias `clean-cache`/`clean-project`) abren el mismo
submenú.

### Git — tarjeta unificada (`git.rs`)

`Git Init` y `.gitignore` eran dos pasos del mismo flujo (creas el repo y
necesitas excluir `target/`, `node_modules/` y artefactos). Ahora hay **una
tarjeta Git** con submenú; `gitignore.rs` desapareció y sus plantillas viven en
`git.rs`:

| Opción | Qué hace |
|--------|----------|
| 1. Inicializar | `create_repo()`: `git init` + rama `main` + primer commit. Idempotente (si ya hay commits no toca nada) |
| 2. Crear/actualizar .gitignore | Escribe la plantilla del tipo de proyecto |
| 3. Conectar con remoto | `connect_remote()`: prepara el repo si falta y lo enlaza con GitHub |
| 4. Ver estado | Rama, remoto `origin` y cambios sin commitear |

**Conexión al remoto** (`connect_remote`), que es la opción nueva:

- Si el proyecto aún no es repo (o no tiene commits), ofrece inicializarlo antes
  reutilizando `create_repo()`.
- Detecta si ya existe `origin`: si es la misma URL no hace nada; si cambia,
  `set-url`; si no existe, `add`.
- Si `gh` está instalado, ofrece **crear el repo en GitHub** en un paso
  (`gh repo create <nombre> --source=. --remote=origin --push [--private|--public]`),
  con nombre por defecto = carpeta del proyecto y visibilidad a elegir.
- Sube la rama actual con `git push -u origin <rama>` y, si falla, imprime el
  error **con pistas** (auth, `gh auth login`, repo con contenido previo).
- `web_url()` convierte el remoto a URL navegable (`git@host:path` → `https://host/path`).

Detalles:

- `run_capture(program, args, dir)` es el único punto que lanza procesos (git y
  gh) y devuelve el error combinado stdout+stderr para mostrarlo; `git_capture` es
  el atajo para `git`.
- `looks_like_git_url()` **avisa pero no bloquea**: si la URL no parece de git se
  usa igual (por si es un host raro) en vez de impedir la conexión.
- Los tests cubren las partes puras (verificables sin red ni git real):
  `gitignore_never_ignores_sources`, `looks_like_git_url_accepts_https_and_ssh`
  y `web_url_normalizes_ssh_and_strips_git_suffix`.

CLI: `--task git` (y los alias `git-init`/`gitignore`) abren el mismo submenú.

### Submenús Firmas/Git — mismo estilo que el menú principal (`tasks/submenu.rs`)

Los submenús de **Signing** y **Git** corren en una pestaña del contenedor y usan
`tasks/submenu.rs`, que **reutiliza las piezas del menú principal** en vez de
tener su propia estética: `menu::render_header` (emoji, `Compass v<versión>` y
`Proyecto: … [BADGE]`), una fila de sección con la forma de las pestañas
(`" GIT 5 "` en el acento de la sección), la **misma rejilla adaptativa** de
cards de 3 líneas (`menu::card_widget` + `layout`/`grid_*`/`visible_rows`), el
**halo de hover** (`render_hover_glow`) y el pie con atajos + ruta completa.

- Cada entrada es un `SubmenuItem { icon, label, description, color_key }`; el
  acento sale de `theme::CARD_KEYS` (`card_accent`/`card_bg`), así que un
  submenú no repite colores entre sus cards ni inventa literales.
- `Submenu<T>` devuelve `SubmenuAction::{Run(T), Back, Continue}`; el módulo
  dueño sólo aporta `Vec<(T, SubmenuItem)>` y ejecuta la acción. `Esc` (o `q`)
  devuelve `Back`; la card "Volver" es una entrada normal que el llamador
  mapea a su propio `Back`.
- Teclado como el menú: `↑↓` por filas, `←→` por cards (h/j/k/l como alias),
  `Enter` ejecuta, rueda del ratón por filas, clic ejecuta. El **hover** llega
  porque el contenedor reenvía el ratón a la pestaña activa (ver "Contenedor de
  terminal").
- Título y sección: Firmas → `FIRMAS · KEYSTORE` / `"MNT"`; Git → `GIT` /
  `"CFG"` (la misma pestaña en la que vive su card en el menú principal).
- El grid ya no es fijo de 26 columnas: en una pestaña de 120 columnas caben 4
  cards adaptadas, y la descripción se recorta con `…` igual que en el menú.

Cubierto por `tasks::submenu::tests` (cada centro de card resuelve a su índice,
huecos y cabecera no son cards, cabecera con la versión real, scroll y Enter) y
por los tests de `signer.rs` sobre las entradas (todas las opciones una sola vez,
`Back` al final, claves de paleta distintas). El e2e comprueba que el submenú de
Git se pinta con `GIT 5` y `volver` dentro de la pestaña.

### PowerShell registry
- Todo el registro se toca con `cmd::run_ps()` (pwsh si está, si no 5.1).
- Usar `-Type` en lugar de `-PropertyType`: el segundo solo existe en PowerShell Core, y el script tiene que funcionar también en 5.1.
- `Test-Path -PathType Leaf` no funciona en claves de registro; se omite.
- **Todo valor interpolado va con `ps_quote()`** (comillas simples duplicadas): un `'` en el título o en la ruta rompería el literal del script instalador, que se compone como texto.

### Directory removal (Windows)
`cmd::remove_dir()` usa `Remove-Item -Recurse -Force` (con `-LiteralPath` entrecomillado) en el shell activo, para rutas largas que `std::fs::remove_dir_all` no puede manejar.

### TUI Menu — pestaña del contenedor

Cada tarea del menú TUI se lanza en una **pestaña del propio Compass** (ConPTY
con el shell activo, ver "Contenedor de terminal") ejecutando
`compass --task <task> --path '<proyecto>'`. El menú sigue vivo en la pestaña
`Inicio` y se recupera con `F12`; la pestaña de la tarea conserva su pantalla
cuando el proceso termina, que se cierra sola con `ui::pause_for` o tras un
Enter en las tareas que muestran salida.

Con `COMPASS_EXTERNAL_WINDOWS=1` (o si abrir la ConPTY falla) se recupera el
comportamiento antiguo: `cmd::spawn_new_window_autoexit` abre una ventana
externa de PowerShell.

### Android dev (Tauri)
`android::dev()` **no** invoca el CLI de Tauri: lanza el script `dev:android` de `package.json`. Antes ejecuta:
1. `ensure_node_modules()` — npm install si falta `node_modules/`
2. `ensure_android_project()` — `node node_modules/@tauri-apps/cli/tauri.js android init` si falta `src-tauri/gen/android/`
3. `ensure_package_consistent()` — verifica que el directorio del paquete Android (`app/src/main/java/<identifier>`) coincida con el `identifier` de `tauri.conf.json`. Si falta, ofrece eliminar `src-tauri/gen/android` y recrear con `tauri android init`.
4. `select_device()` — elige dispositivo (ver sección Device Selection)
5. NDK / Rust targets según sea necesario

⚠️ **`DevLaunch`**: si el proyecto declara `dev:android` en sus `scripts`, se usa `npm run dev:android`; si no, cae a `tauri::command(ctx, "android dev")` (`npx.cmd tauri …` si no hay CLI local). El script manda porque es el comando que el autor del proyecto usa a mano, con sus flags (`--host`) y sus overrides (`tauri.android.conf.json`) que dejan el puerto correcto; reconstruirlo aquí los pierde y el WebView acaba pidiendo `localhost` sin túnel detrás. En Windows se emite `npm.cmd` y no `npm` (shim `npm.ps1` bloqueado por ExecutionPolicy). El `Read-Host` final de la ventana interactiva lo añade el spawner, así que el comando no se preocupa por dejar la consola abierta.

`android::debug()` comparte `prepare_android_dev()` y **también** lanza la app por `DevLaunch`, antes de abrir `edge://inspect` (son pasos 4/5 y 5/5). ⚠️ El orden es load-bearing: `adb kill-server` es el paso **1** y el lanzamiento el **4**, nunca al revés — es el `beforeDevCommand` del script quien levanta el `adb reverse` del puerto del dev server, así que matar el servidor después borraría los túneles y devolvería el "connection refused" con la página en blanco. Por eso `prepare_android_dev()` (que puede acabar en `npm install`) va **antes** del bloque de adb.

Cubierto por `tasks::android::tests` (3 tests): usa el script si está declarado, cae al CLI si no, y no aborta con `package.json` ausente o roto.

### Kotlin release builds (APK/AAB)
`kotlin::build_apk()` ejecuta `:assembleRelease` (antes `:assembleDebug`) y `kotlin::build_aab()` ejecuta `:bundleRelease`. Ambos leen `app/keystore.properties` (o `composeApp/keystore.properties`) y exportan variables de entorno para Gradle:
- `RELEASE_STORE_FILE` (ruta absoluta al keystore)
- `RELEASE_STORE_PASSWORD`
- `RELEASE_KEY_ALIAS`
- `RELEASE_KEY_PASSWORD`

Esto permite que el `signingConfigs` existente en `build.gradle.kts` (que usa variables de entorno `RELEASE_*`) firme automáticamente sin modificar el archivo del proyecto. Si el archivo de propiedades no existe, avisa con `[WARN]` y continúa (el build puede fallar si no hay firma configurada).

### Instalación del APK release en Kotlin (igual que debug)

`kotlin::build_apk` (tarjeta "APK Release") **pregunta al terminar el build** si quiere
instalar el APK en el teléfono, y lo hace con `adb install -r` + `am start`, o sea el
mismo flujo que la tarjeta de debug (`:installDebug`).

- Gradle no tiene tarea para "instalar este APK release", así que se usa `adb` a mano
  con el artefacto **ya renombrado** (`<App>-v<ver>.apk`). Por eso `cmd::run_build_and_reveal`
  se descompuso en `run_build_and_reveal_with_env_outcome`, que devuelve
  `BuildOutcome { success, artifact }`: sin eso se instalaría el fichero viejo.
- ⚠️ **Choque de firmas**: si el dispositivo ya tiene la app instalada con la clave de
  **debug**, `adb install -r` falla con `INSTALL_FAILED_UPDATE_INCOMPATIBLE` (la release
  va firmada con otra clave). `install_release_apk` detecta ese caso y ofrece
  desinstalar la versión anterior (`adb uninstall <namespace>`) antes de reinstalar.
- El **AAB** no se puede instalar con `adb install` (es un bundle de Play Store): la
  tarjeta de AAB sólo avisa de que hay que usar el APK o `bundletool`.


### Go + Wails v3 (`tasks/gowails.rs`)

Tipo de proyecto nuevo: `ProjectType::GoWails`. Se detecta con **`go.mod` que
contenga `wailsapp/wails` + `Taskfile.yml`** (o `build/config.yml`): la v2
usaba `wails.json` y otro CLI, así que no se mezcla (`context::is_wails_v3`).

| Tarjeta | Qué hace |
|---------|----------|
| Windows | `wails3 task dev` en ventana interactiva (Vite en modo dev + hot reload) |
| Windows Build | `build_desktop`: **x86 portable** (`wails3 task build ARCH=386`) + **x64/ARM64 portable e instalador NSIS** (`wails3 task package ARCH=…`). Todo se copia a `bin/` con el nombre del producto y la versión |
| APK Release | Abre el submenú de `release.rs` (arm64 / armv7 / universal / Todos). Cada card compila una ABI o las tres con **ABI splits**, copia a `bin/` e instala la universal si hay teléfono |
| AAB (release) | El mismo pipeline (arm64 + armv7, sin splits) + `bundleRelease` → `bin/<Product>-v<ver>.aab` |
| Install (debug) | APK debug (arm64) + `adb install` + `am start`; la ventana se cierra sola a los 10 s tras compilar bien |
| Debug Andr | `adb logcat --pid=<pid de la app>` (sin `grep`, así que funciona en Windows) |
| Version | Bump de `version:` en `build/config.yml` (formato YAML, conserva indentación y comentario) |
| Clean | `bin/`, `frontend/dist`, `frontend/bindings`, `build/android/app/build`, `build/android/.gradle`, `jniLibs/` |

La card **APK armv7** ya no existe en el menú principal (la release por
arquitectura vive en el submenú de `APK Release`), pero la tarea
`--task android-armv7` existe y compila sólo esa ABI
(`gowails::build_apk_armv7`) para equipos antiguos.

**El build de escritorio produce los paquetes de x86, x64 y ARM64.** `build_desktop()`
encadena tres pasos y copia cada artefacto en cuanto termina:

| Arquitectura | Comando | Artefactos en `bin/` |
|---|---|---|
| x86 | `wails3 task build ARCH=386` | `<Product> v<ver> x86.exe` |
| x64 | `wails3 task package ARCH=amd64` | `<Product> v<ver>.exe` + `<Product> v<ver> x64 installer.exe` |
| ARM64 | `wails3 task package ARCH=arm64` | `<Product> v<ver> arm64.exe` + `<Product> v<ver> arm64 installer.exe` |

- **Windows/NSIS es el formato por defecto** del Taskfile (`FORMAT` default
  `nsis`). `FORMAT=msix` produce `bin/<APP>-<arch>.msix`, pero Compass no lo
  expone en el menú: para eso está la tarea del proyecto con su variable.
- ⚠️ **x86 no tiene instalador NSIS**: `wails_tools.nsh` sólo define
  `ARG_WAILS_AMD64_BINARY` / `ARG_WAILS_ARM64_BINARY`, y el Taskfile mapea
  cualquier `ARCH` que no sea `amd64` a ARM64. Empaquetar 386 con NSIS
  etiquetaría el binario equivocado, así que x86 se entrega **sólo como
  portable**. x64 y ARM64 llevan portable **e** instalador.
- **`package` recompila el portable como dependencia** en la misma ruta
  `bin/<APP_NAME>.exe`, sobrescribiéndolo. Por eso se copia a su nombre final
  **inmediatamente después de cada paso**: si se copiara al final, las tres
  arquitecturas acabarían siendo el mismo binario. Ya no hace falta filtrar
  "el artefacto más reciente": se conocen las rutas exactas del Taskfile.
- **`makensis` no suele estar en el PATH.** `winget install NSIS.NSIS` lo deja en
  `C:\Program Files (x86)\NSIS\` y **sin** tocar el PATH, así que `makensis` a
  secas falla. El Taskfile de Wails ya lo resuelve con un `MAKENSIS` que prueba
  `command -v`, `Program Files (x86)` y `Program Files`; no hay que parchear nada.
- **El binario de dev lleva el icono de la app.** `wails3 dev` no genera el
  `.syso` (icono/version de Windows) por defecto, así que el `.exe` salía con el
  icono genérico de Go. `gowails::dev()` ejecuta antes `wails3 generate syso`
  (best-effort, con las rutas de `build/windows/`) para que `go build` lo embeba.
  El `.syso` queda en la raíz del proyecto y está en el `.gitignore`.

⚠️ **Por qué el build de Android NO usa el Taskfile del proyecto**: las tareas
`android:*` de la plantilla de Wails v3 usan scripts POSIX, que no se ejecutan
en Windows. Aunque la NDK del host sí exista (`windows-x86_64`), el build del
Taskfile aborta. Por eso Compass replica el pipeline con el *host tag* y el
compilador correctos:

1. `wails3 android overlay:gen -out build/android/overlay.json -config build/config.yml`
2. `wails3 generate bindings -f "-tags android,debug" -clean=true -ts -i`
3. Frontend: `npm install` (si falta `node_modules/`) + `npm run build` en `frontend/`
4. `go build -buildmode=c-shared -overlay build/android/overlay.json … -o build/android/app/src/main/jniLibs/<abi>/libwails.so`, **una vez por ABI** (arm64-v8a y armeabi-v7a)
5. Gradle en `build/android/`: `assembleDebug` (debug, sin init script) o `--init-script <tmp> assembleRelease` / `bundleRelease` (versión + ABIs)

⚠️ **Los bindings se generan en el HOST, sin `GOOS=android`.** Es el punto
más fácil de romper: el generador de bindings **carga los paquetes de Go** para
analizarlos, y con `GOOS=android` + `CGO_ENABLED=0` el cargador no puede
resolver ni la biblioteca estándar:

```
WARNING -: android/amd64 requires external (cgo) linking, but cgo is not enabled
WARNING main.go:4:2: could not import embed (missing package: "embed")
INFO Processed: 1 Package, 0 Services, 0 Methods, ...
```

Con **0 servicios** generados, el frontend deja de compilar con un error
críptico: `Could not resolve "../../../bindings/<paquete>/service"`. El
Taskfile de Wails **sí** define una tarea `generate:android:bindings` con esas
variables, pero **no** forma parte del pipeline de build: la que se usa
(`common:generate:bindings`) corre en el host. En Compass el paso se captura
(`cmd::run_program_capture`) y si la salida contiene `could not import` se
aborta con un mensaje claro, en vez de dejar que reviente el frontend.

⚠️ **Al generador de bindings sólo se le pasan los `-tags`.** Reparte la
cadena `-f` por espacios y se la pasa tal cual a `go list`, así que cualquier
flag con valor compuesto lo rompe:

```
$ wails3 generate bindings -f '-tags production,android ... -ldflags="-w -s"'
ERROR unsupported version of go: exit status 2: flag provided but not defined: -s
```

Ni con comillas funciona: el reparte no las respeta. Para generar los bindings
sólo importan los **tags** (deciden qué ficheros de Go se analizan), así que
Compass pasa `-tags production,android` (release) o `-tags android,debug`
(debug) y nada más. Los flags deStripping (`-trimpath`, `-ldflags`) son para
el binario final, y ahí sí se pasan como argumentos sueltos a `go build`.

#### Versión, ABIs y APK por ABI (`write_init_script`)

La versión del APK/AAB sale de **`build/config.yml`** (`info.productName` +
`info.version`), la misma fuente con la que se nombra el `.exe`. El Gradle de
la plantilla tiene su propia `versionName`/`versionCode` fijas, así que Compass
las inyecta con un **init script** (`gradlew --init-script`) **sin editar**
`build/android/` (esa carpeta la regenera `wails3 update build-assets`):

- `versionName = <info.version>` y `versionCode = major*1_000_000 + minor*1_000 + patch`
  (Play exige un entero monótono; se ignoran sufijos `-beta`/`+build` y, si no
  cabe en el límite de Play, no se inyecta el código).
- `defaultConfig.ndk.abiFilters` se **vacía** y se rellena con las ABIs
  realmente compiladas (arm64-v8a y/o armeabi-v7a), así no se empaqueta la
  `x86_64` que trae la plantilla.
- `packagingOptions.doNotStrip '*/armeabi-v7a/libwails.so'`: la plantilla sólo
  protege arm64/x86_64 del *strip*, y el `.so` de Go de 32 bits no debe
  recortarse.
- Con **ABI splits** (`splits.abi.enable/reset/include` + `universalApk = true`)
  una sola pasada de `assembleRelease` deja **tres** APK:
  `app-arm64-v8a-release.apk`, `app-armeabi-v7a-release.apk` y
  `app-universal-release.apk`.

⚠️ **El init script modifica el DSL en el `afterEvaluate` registrado desde
`beforeProject`, no en `projectsEvaluated`.** AGP crea las variantes (y lee
versión, `abiFilters` y splits) en su **propio** `afterEvaluate`, que registra
al aplicar el plugin. `beforeProject` corre antes de evaluar el script, así que
nuestro callback queda primero en la cola: se ejecuta después del DSL del
proyecto pero **antes** que el de AGP. Con `projectsEvaluated` ya sería tarde y
el build saldría con la versión y las ABIs de la plantilla (era justo el bug:
el APK no tomaba la versión del código). Verificado con AGP 8.7.3 / Gradle
9.2.1: las tres APK salen con `versionName='1.0.1'`,
`versionCode='1000001'` y la universal con `arm64-v8a` **y** `armeabi-v7a`.

Los artefactos se copian a `bin/` con el nombre del producto:

| Archivo | Origen |
|---|---|
| `<Product>-v<ver>-arm64.apk` | split `app-arm64-v8a-release.apk` |
| `<Product>-v<ver>-armv7.apk` | split `app-armeabi-v7a-release.apk` |
| `<Product>-v<ver>-universal.apk` | `app-universal-release.apk` (el que se instala) |
| `<APP_NAME>.apk` | copia del universal, por compatibilidad con el Taskfile |
| `<Product>-v<ver>.aab` | `bundleRelease` (arm64 + armv7, sin splits) |

`build_release_apks()` elige cada split por **nombre** (`newest_matching` con
los fragmentos `arm64-v8a`, `armeabi-v7a` y `universal`): "la APK más reciente"
no sirve, porque las tres están en la misma carpeta. También ignora una
`app-release.apk` vieja de builds sin splits.

Detalles que importan:

- ⚠️ **La ventana del menú se cierra al terminar el proceso.** La tarea se
  lanza con `spawn_new_window_autoexit` (`pwsh -NoProfile -Command
  "[Console]::Title = …; & compass --task …"`), que **no** espera un Enter: si
  la tarea muere con un error, el mensaje
  desaparece en un segundo y el usuario ve "la terminal se cierra". Por eso
  las tareas de Android terminan con `ui::pause()`, y **la instalación no
  aborta el build** si no hay dispositivo (la APK ya está compilada: se avisa
  y se sigue). Lo mismo se aplicó a `kotlin::build_apk`.
- **La instalación es automática si hay un móvil conectado**: no se pregunta
  nada. `android_utils::install_apk_if_device()` comprueba
  `connected_devices()` y:
  - **Con dispositivo** → instala (`adb install -r`) y abre la app. Con varios,
    `select_device` pregunta cuál.
  - **Sin dispositivo** → omite el paso con un aviso y la ruta de la APK.

  Aplica igual al **debug** y al **release**, y en Kotlin y en Go+Wails
  (`kotlin::build_apk` y `gowails::build_apk`).
- ⚠️ **Nada de shell para el pipeline de Android.** `cmd::run_program()`
  invoca el programa **sin shell**, con los argumentos uno a uno. Un shell de
  por medio (era `cmd /c`) parte las rutas con espacios (`D:\Mis Proyectos\Go +
  Wails v3\...` → `D:\Mis` + `Proyectos/Go` + …) y `cmd.exe` además borra la
  primera y la última comilla de la línea, lo que rompe `-ldflags="-w -s"`. Con
  `Command::new(program).args(args)` cada argumento llega intacto.
- **Compilador de la NDK**: en Windows el NDK trae un wrapper `.cmd` (y un
  script bash sin extensión, que no es ejecutable ahí), así que se llama a
  **`clang.exe` con `--target` explícito**, que es justo lo que hace el wrapper:
  `aarch64-linux-android21` (arm64) o `armv7a-linux-androideabi21` (armv7). El
  `minSdk` (21) sale de `build/android/app/build.gradle`.
- **Variables de entorno por ABI**: `GOOS=android`, `GOARCH=arm64` (o `arm` +
  `GOARM=7`), `CGO_ENABLED=1`, `CC` y `CXX` se inyectan con `Command::env` (vía
  `cmd::run_program`), nunca prefijando el comando. Se compila `libwails.so` una
  vez por ABI antes de llamar a Gradle.
- **`local.properties`**: si ni `ANDROID_HOME` ni `ANDROID_SDK_ROOT` están
  definidas, Compass escribe `build/android/local.properties` con `sdk.dir`.
- **`applicationId` ≠ `namespace`**: el `applicationId` (`com.wails.app` por
  defecto) es el paquete de la APK; la actividad se llama
  `<namespace>.MainActivity` (`com.wails.app.MainActivity`). Se leen los dos
  de `build/android/app/build.gradle`.
- **Firma**: Wails v3 firma el release con **variables de entorno**, no con un
  `keystore.properties`:
  `ANDROID_KEYSTORE_FILE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` y
  `ANDROID_KEY_PASSWORD` (las lee `build/android/app/build.gradle`). Compass
  lo resuelve así:
  - La tarjeta **Signing** funciona igual que en los demás tipos: crea/importa
    el keystore, escribe las properties, saca las huellas, hace respaldos y
    verifica la APK. Lo único que cambia es **dónde** y **cómo**:
    - `AndroidLayout` tiene dos carpetas: `module_dir` (el módulo Android,
      `build/android/app`, de donde sale la APK que se verifica) y
      `signing_dir` (dónde viven el keystore y el `.properties`).
    - En Wails `signing_dir` es **`android-signing/`** en la raíz, **fuera de
      `build/`**: `wails3 update build-assets` regenera `build/android/` y se
      llevaría el keystore (que es irrecuperable si no está respaldado).
    - `patch_gradle = false`: el Gradle de Wails ya lee el entorno, así que
      **no se parchea** el `build.gradle` (además lo regenera Wails).
  - Al compilar la APK/AAB release, `gowails::signing_env()` lee
    `android-signing/keystore.properties` y exporta las cuatro variables al
    proceso de Gradle (`Command::env`). Sin keystore, el build avisa de que
    la APK saldrá firmada con la llave de **debug** (que Play Store rechaza).
  - El `.gitignore` de Go+Wails ignora `android-signing/` y
    `keystore-backups/`: contienen el keystore y las contraseñas.
- El **AAB** no se instala con `adb install`: es un bundle de Play Store.
- `APP_NAME` y `BIN_DIR` se leen del bloque `vars:` del `Taskfile.yml` de la
  raíz (sólo literales; los valores con plantilla `{{...}}` se ignoran).

**Verificado** en `school-tracker` (Windows): `assembleRelease` con el init
script genera `app-arm64-v8a-release.apk`, `app-armeabi-v7a-release.apk` y
`app-universal-release.apk`, todas con `versionName='1.0.1'` /
`versionCode='1000001'`; la universal contiene los dos `libwails.so` (20 MB
cada uno, sin *strip*) y la `x86_64` de la plantilla queda fuera. El `.so` de
armv7 se compiló con `GOARCH=arm GOARM=7` y `clang --target=armv7a-linux-androideabi21`.
En escritorio, `wails3 task build ARCH=386` produce un `PE32 i386` y
`wails3 task package ARCH=arm64` deja `bin/school-tracker-arm64-installer.exe`.
Con `ANDROID_KEYSTORE_*` exportadas, `apksigner verify --print-certs` muestra
el certificado del keystore configurado; sin ellas, el de debug
(`CN=Android Debug`).

### Tauri Android package consistency (`ensure_package_consistent`)
Detecta el error típico "Project directory ... does not exist" que ocurre cuando se cambia el `identifier` en `tauri.conf.json` sin regenerar el proyecto Android. Lee el identifier, calcula la ruta esperada del paquete Java (`app/src/main/java/<pkg_path>`) y si no existe:
1. Muestra el identifier esperado y la ruta faltante.
2. Pregunta si desea eliminar `src-tauri/gen/android` y ejecutar `tauri android init`.
3. Si responde `s`: elimina y recrea. Si responde `n`: continúa (el build fallará).

### Device Selection (`android::select_device()`)

**Sólo dispositivos físicos.** El emulador se eliminó del flujo: ya no hay
tarjeta "Emulator", ni task `launch-emulator`, ni `emulator_path` en el contexto,
ni AVDs. `utils/android.rs` se reduce a `Device { serial }` + `select_device()`.

| Escenario | Comportamiento |
|-----------|---------------|
| 1 físico conectado | Auto-selecciona, espera `sys.boot_completed` |
| > 1 físicos | Menú con los físicos, luego espera el boot del elegido |
| 0 físicos | Error con pasos concretos (debug USB, huella RSA, `adb devices -l`) |

Dos detalles que importan en la práctica:

- **`emulator-*` se filtra** por serie. Si un emulador queda corriendo de antes,
  no se confunde con un teléfono.
- **Espera de boot**: un teléfono recién conectado aparece como `device` en
  `adb devices` pero aún rechaza `adb install`. `wait_for_device_boot()` espera
  a que `sys.boot_completed` valga `1` (60 s) para fallar tarde, no con un error
  confuso en medio del build.
- **`unauthorized` / `offline`**: cuando hay líneas en `adb devices` pero ningún
  dispositivo utilizable, se diagnostica el caso concreto en vez de decir
  genéricamente "no se detectaron dispositivos".

### UI Helpers (`ui.rs`)
| Función | Descripción |
|---------|-------------|
| `ask_text(prompt, default)` | Prompt inline (cursor después de `: `), no usa inquire |
| `ask_password(prompt)` | Password inline con enmascaramiento `*`, raw mode vía crossterm |
| `success_banner(title)` | Borde ┌─┐└─┘ con ✅ centrado |
| `info_banner(title)` | Borde ┌─┐└─┘ con ℹ️ centrado |
| `pause()` | Espera Enter (read_line, sin raw mode) |
| `pause_for(secs)` | Cuenta regresiva y cierra automáticamente tras `secs` segundos. La usan **todas** las tareas de compilación (debug y release, todas las plataformas) al terminar con éxito, para que la ventana se cierre sola. |

### Signer prompts
`signer.rs` usa `ui::ask_text()` y `ui::ask_password()` en vez de `inquire::Text`/`inquire::Password` para que el cursor aparezca después de los dos puntos en la misma línea.

### CLI directo (sin TUI)
```bash
compass --task windows-dev
compass --path /ruta/proyecto --task sign
compass --task context-menu-install
compass --task windows-release   # submenú x86/x64/ARM64
compass --task android-release   # submenú arm64/armv7/universal
compass --task android-aab       # bundle de Play Store
compass  # abre menú interactivo
```

Tareas sólo de Go + Wails: `--task package` (alias de `windows-build`; ambos compilan el instalador NSIS/MSIX con `wails3 task package`).

## .gitignore Templates

Cada tipo de proyecto tiene su propio `.gitignore`:

- **Rust**: ignorar target/ (preserva src/, Cargo.toml, tests/)
- **Tauri**: ignorar Rust target, gen/android, node_modules, dist, Vite cache
- **Flutter**: ignorar build, .dart_tool, Android builds, ios symlinks
- **Kotlin**: ignorar build, .gradle, .kotlin, local.properties, APK/AAB artefactos
- **Go puro**: ignorar binarios, tests compilados, `bin/` y archivos `.env`
- **Go + Wails**: ignorar bin/, frontend/node_modules, frontend/dist, frontend/bindings, build/android (build, .gradle, jniLibs, overlay.json, local.properties), *.syso, APK/AAB

Todos preservan: fuente, config, wrapper/scripts, firmas, motor WhatsApp.

## install.bat

Script de instalación para Windows:

1. **Salta la compilación si no hace falta** (ver "Saltar lo que ya está al día")
2. **Siempre** copia `assets/icon/Icon.ico` a `%USERPROFILE%\.cargo\bin\`
3. **Siempre** copia `assets/icon/opencode.ico` al mismo directorio
4. Instala los menús contextuales de Compass (incluida la acción `.gitignore`) y Vibe Coding, cada uno sólo si la sonda dice que falta
5. Si `cargo install` falla, avisa y **no** configura los menús: apuntar el registro a un binario roto deja el sistema incoherente
6. `install.bat --force` rehace todo aunque no haga falta

### Saltar lo que ya está al día

Reinstalar costaba ~30 s en cada ejecución, casi todo en cosas innecesarias. Los tres pasos se deciden con una **sola** invocación de PowerShell, `assets/install-probe.ps1`, que devuelve `<compilar>;<menu CR>;<menu vibe>;<faltan>` con 1 = actuar:

| Antes | Ahora | Motivo |
|---|---|---|
| `cargo install --path . --force` siempre | sólo si algún archivo de `src/`, `Cargo.toml` o `Cargo.lock` es más nuevo que el `.exe` | el `--force` de cargo no es opcional: sin él se niega a reinstalar la misma versión |
| 2 sondas de registro en PowerShell | 1 sola, con las cuatro comprobaciones | cada arranque de PowerShell cuesta (5.1 ~2,5 s, pwsh ~0,5 s); 3 llamadas son ~7,5 s |
| Menú de Vibe Coding **siempre** reinstallado | sólo si falta **o** si lo escribió otro binario | la sonda miraba `zz_VibeCoding\shell\OpenCode\command`, una clave que **no existe**: el padre sólo lleva `ExtendedSubCommandsKey` y las subentradas viven en `Directory\ContextMenus\VibeCoding\shell\` con prefijo numérico (`01OpenCode`). Además, `install_vibe_coding()` guarda `CompassStamp` (mtime del exe en segundos) y la sonda lo compara con el `compass.exe` instalado: recompilar y añadir una herramienta obliga a reescribir el árbol. Sin la marca (instalación anterior) se reinstala una vez |
| Sin detección de herramientas ausentes | cuarto campo de la sonda | va en la **misma** invocación de PowerShell, así que no cuesta los ~2,5 s de arranque; avisa en minúscula si falta alguna |
| `timeout /t 3` al final | eliminado | 3 s de espera muerta |

Medido: **~30 s → ~7 s** sin nada que cambiar, ~19 s cuando hay que recompilar, ~32 s con `--force`. El grosso del coste era `opencode-install`, que tardaba **18 s** sólo en detectar las herramientas (un `Get-Command` por herramienta, o sea un arranque de PowerShell cada una). La sonda de `install.bat` hace todas las comprobaciones en **una** invocación, así que añadir herramientas a `VIBE_TOOLS` no encarece el instalador.

⚠️ La sonda vive en un `.ps1` y no en un one-liner dentro de `for /f` porque las tuberías (`|`) son caracteres especiales de `cmd` y rompen el bucle. Y ⚠️ `1` significa **actuar**: leerlo al revés hace reinstallar siempre (que es lo que pasó en la primera versión de este cambio).

`install.bat` ejecuta la sonda con **pwsh si está instalado** y cae a
`powershell` si no. La detección es **ejecutándolo** (`pwsh -NoLogo -NoProfile
-Command "exit 0"` + `if not errorlevel 1`), no con `where`: el alias de la
Microsoft Store es un stub de 0 bytes que `where` encuentra y que no arranca.

La sonda también considera incompleto el menú de Compass si falta la acción `.gitignore` en el clic derecho sobre una carpeta o sobre el fondo. Así `install.bat` migra instalaciones anteriores que ya tenían Compass, pero no esa acción.

⚠️ **Migración de cmd a PowerShell**: la sonda fuerza la reinstalación si el
comando de `zz_CompassGitignore` o el de `01OpenCode` todavía contiene `cmd.exe`,
y también si `zz_Compass` no pasa por `pwsh`/`powershell` (antes lanzaba el
`.exe` directo, con el icono genérico de consola). La marca `CompassStamp` sola
no basta: instalar pwsh *después* de Compass no cambia el mtime de
`compass.exe`, así que la marca coincidiría y el menú viejo se quedaría para
siempre. Cubierto por `probe_forces_migration_of_cmd_based_entries`.

⚠️ `install.bat` usa `setlocal EnableDelayedExpansion` y `!errorlevel!` dentro de los bloques: con `%errorlevel%` se evalúa al parsear el `if`, antes de que `cargo` corra, y siempre da 0.

## Testing

`cargo test` cubre el TUI: renderizado y estados de las cards (ver "Estados
visuales de una card"), filtrado por tipo de proyecto, orden y rangos de las
pestañas, la rejilla (`cards_fill_the_window_in_a_four_column_grid` comprueba
que 60 columnas dan 2 y 200 dan 4, `first_row_shows_separated_cards` que hay una
celda entre cards), `ensure_visible`/scroll por filas en terminales cortas,
hit-testing de cards y pestañas (`card_at`/`tab_at`, incluido que los huecos no
responden y que el clic acierta tras hacer scroll), navegación (`arrows_follow_the_grid`:
`↑↓` por filas y `←→` card a card con salto de pestaña en el borde), adaptación al
ancho (`layout`), el encabezado (`header_shows_the_real_compass_version`
comprueba que muestra `CARGO_PKG_VERSION` y no repite el tipo de proyecto), el
halo de hover (arranque, bucle de fase, brillo real del borde e interior, y
recorrido del perímetro) y la paleta/mezcla de colores de
`theme.rs`. De `context_menu.rs` se verifican las partes puras: orden y
prefijos del submenú, comandos de instalación de cada herramienta, nombre del
binario, shell de instalación, fallbacks fuera del PATH (incluido `HOME/` de
Qoder), la marca `CompassStamp`, el comando que aloja la TUI en PowerShell y el
detector de `cmd.exe`/`pwsh` que comparte con la sonda del instalador. De
`cmd.rs` se cubren `ps_quote`, el script y los
argumentos de las ventanas (`&`, `Read-Host`, `-NoProfile`, sin sintaxis de
cmd), la resolución del shell y `newest_artifact`. De `terminal.rs`, la carpeta
de perfil de cada shell además de la versión y el BOM. De `tauri.rs`, el
fallback a `npx.cmd`. De `container.rs`, la codificación de teclas a xterm
(texto, control, flechas, Alt y F1–F12), el recorte de títulos largos de la
barra y el que la zona clicable del `×` cubra la celda donde el `Paragraph`
pinta el glifo (sin ConPTY: dibuja la barra en un `TestBackend` y busca el `×`
en el buffer). De `console_guard.rs`, el texto del aviso (singular/plural y la
indicación de usar `Esc`) y que sin `install()` el contador no toca Win32. De
`environment.rs`, los marcadores de los scripts embebidos (que sean los reales),
la idempotencia de `materialize()`, la elección del respaldo más reciente, las
entradas del submenú y el parseo de respuestas afirmativas. De `gowails.rs` se
cubren `version_code` (semver → entero monótono de Play), los nombres
versionados de APK/AAB y escritorio, el contenido del init script (versión,
ABIs, splits, `doNotStrip` y que use `beforeProject`/`afterEvaluate` y no
`projectsEvaluated`), `release_meta` contra `build/config.yml` y la selección
de cada split por nombre (`newest_matching`). De `release.rs`, las entradas del
submenú por tipo, las claves de paleta y el mapeo triple/GOARCH; de `kotlin.rs`,
el init script de ABIs y `has_native_libs`; de `cmd.rs`, que `rename_artifact`
sobrescribe un destino existente; de `submenu.rs`, el flag `warn_android`; y de
`menu.rs`, el routing de las cards de release y que `AndroidBuildArmv7` queda
oculta en todos los tipos.

Dos pruebas abren una **ConPTY real** y están `#[ignore]` porque dependen del
sistema y tardan unos segundos:

```text
cargo test --bin compass session_renders_its_output -- --ignored
cargo test --test container_e2e -- --ignored --nocapture
```

La primera comprueba que una sesión pinta su salida (`hola contenedor`). La
segunda (`tests/container_e2e.rs`) arranca el `compass.exe` real en una ConPTY
de 120×35, entra en `CONFIGURACIÓN`, ejecuta la card Git y verifica la secuencia
completa: primera fila con las cajas redondeadas `Compass` y `1 Git ×`, submenú
de Git dentro de la pestaña (con el estilo del menú: `GIT 5` y `volver`), F12 al
menú sin cerrar la pestaña, `Esc` con la pestaña abierta (aviso de salida, luego
`Esc` para cancelarlo), F12 de vuelta, Ctrl+W para cerrarla y `Esc` para salir.
La espera del menú no puede usar `Proyecto:`: el submenú reutiliza la cabecera y
también lo pinta; se distingue por la fila `DESARROLLO · MNT`. El clic del `×` y
el hover no se simulan: crossterm lee el ratón de los registros de consola y una
ConPTY no los genera.

De `tasks/submenu.rs` se cubren la rejilla compartida (cada centro de card
resuelve a su índice, huecos y cabecera no son cards), la cabecera con la versión
real, el scroll en terminales cortas y que `Enter` devuelve la entrada
seleccionada; `signer.rs` verifica que sus 11 entradas aparecen una sola vez, con
`Back` al final y con claves de paleta distintas. `container.rs` cubre además el
reenvío del ratón (`mouse_is_forwarded_relative_to_the_body`,
`mouse_drag_and_wheel_use_the_extended_codes`, `mouse_on_the_bar_is_not_forwarded`).

El resto —detección de proyectos, escritura real en el registro— se
verifica con compilación (`cargo build`) y pruebas manuales en la máquina de
destino.
