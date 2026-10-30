//! Temas del TUI, inspirados en los temas de **OpenCode**.
//!
//! Cada `Theme` es un conjunto de roles semánticos (no colores sueltos) para que
//! el dibujado del menú no tenga ningún literal de color: cambiar de tema sólo
//! cambia esta tabla.
//!
//! ## Roles
//!
//! | Rol | Dónde se usa |
//! |-----|--------------|
//! | `text` / `muted` | texto normal / secundario (versión, separadores) |
//! | `border` | borde de una card en reposo |
//! | `brand` | título de Compass en la cabecera |
//! | `dev` `mnt` `cfg` | acento de cada sección (títulos y marcas del pie) |
//! | `card_hexes` | **acento propio de cada card** (18 entradas, en el orden de [`CARD_KEYS`]) |
//! | `hover_bg` | fondo de la card **hovereada** (neutro, distinto de la selección) |
//! | `background` | fondo de **toda la pantalla** (como el bg de los temas de OpenCode) |
//! | `success` `info` `accent` `warning` | badge de tipo de proyecto y avisos |
//!
//! El fondo de la card seleccionada se deriva del acento de esa card (`dark()`
//! al 27 %), así que por tema sólo se eligen 18 colores, no 36. El `hover_bg`
//! va explícito porque el "vidrio esmerilado" del hover es una decisión
//! estética, no una función.
//!
//! Hay tests (`tests::card_hexes_are_distinct_within_each_tab`,
//! `tests::card_bgs_never_collide_with_hover_or_background`) que garantizan que
//! hover y selección nunca colisionan en ningún tema y que dos cards de una
//! misma pestaña no comparten acento.
//!
//! ## Persistencia
//!
//! El tema elegido con `Ctrl+T` se guarda como una línea de texto en
//! `<data_dir>/compass/theme` (`%LOCALAPPDATA%\compass\theme` en Windows).
//! Si el fichero falta o el nombre no
//! existe se vuelve al tema por defecto, [`DEFAULT`].

use std::path::PathBuf;

use ratatui::style::Color;

use crate::utils::platform;

/// Color RGB a partir de un literal `0xRRGGBB`.
///
/// `const` para poder usarlo en las definiciones de [`THEMES`].
const fn hex(v: u32) -> Color {
    Color::Rgb(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    )
}

/// Oscurece `v` hacia el negro al `pct` por ciento.
///
/// Es el tinte que se usa como fondo de la card seleccionada: conserva el tono
/// del acento de la sección pero lo apaga para que el texto y el borde sigan
/// leyéndose.
const fn dark(v: u32, pct: u32) -> Color {
    let r = (((v >> 16) & 0xff) * pct / 100) as u8;
    let g = (((v >> 8) & 0xff) * pct / 100) as u8;
    let b = ((v & 0xff) * pct / 100) as u8;
    Color::Rgb(r, g, b)
}

/// Interpola dos colores RGB (`t = 0` → `a`, `t = 1` → `b`).
///
/// Si alguno no es `Rgb`, devuelve `b`: la animación de hover es decorativa y
/// nunca debe romper un tema ni pintar un color inesperado.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return b;
    };
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::Rgb(lerp(ar, br), lerp(ag, bg), lerp(ab, bb))
}

/// Claves de los acentos por card, en el orden del arreglo `Theme::card_hexes`.
///
/// Una clave por `MenuItem`. El orden es **load-bearing**: coincide posición a
/// posición con los 18 hex de cada tema, así que reordenar aquí reordena (y
/// destiñe) las 14 paletas. `menu::tests::card_keys_cover_every_item` verifica
/// que no falte ninguna tarjeta ni sobre ninguna clave.
pub const CARD_KEYS: [&str; 18] = [
    // DEV
    "run", "android", "reload", "build", "apk", "apk32", "aab", "install", "profile", "test",
    // MNT
    "clean", "debug", "sign", "version",
    // CFG
    "git", "whatsapp", "terminal", "environment",
];

/// Las claves de la pestaña 1 (`DESARROLLO · MNT`) y de la 2 (`CONFIGURACIÓN`).
///
/// Viven aquí y no en `menu.rs` para que `card_hexes_are_distinct_within_each_tab`
/// pueda comprobar la paleta completa sin depender del filtro por tipo de proyecto.
#[cfg(test)]
pub const CARD_KEYS_DEV_MNT: [&str; 14] = [
    "run", "android", "reload", "build", "apk", "apk32", "aab", "install", "profile", "test",
    "clean", "debug", "sign", "version",
];
#[cfg(test)]
pub const CARD_KEYS_CFG: [&str; 4] = ["git", "whatsapp", "terminal", "environment"];

/// Conjunto de colores del TUI. Ver la tabla de roles del módulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub brand: Color,
    pub dev: Color,
    pub mnt: Color,
    pub cfg: Color,
    pub hover_bg: Color,
    pub background: Color,
    pub success: Color,
    pub info: Color,
    pub accent: Color,
    pub warning: Color,
    /// Acento propio de cada card, en el orden de [`CARD_KEYS`].
    pub card_hexes: [u32; 18],
}

impl Theme {
    /// Color de acento de una sección (`"DEV"` | `"MNT"` | `"CFG"`).
    ///
    /// Cualquier otro valor cae en `cfg`, que es lo que hacía el antiguo
    /// `MenuItem::section_color` con su brazo `_`.
    pub fn section(&self, section: &str) -> Color {
        match section {
            "DEV" => self.dev,
            "MNT" => self.mnt,
            _ => self.cfg,
        }
    }

    /// Hex del acento de una card. Una clave desconocida cae en `terminal`,
    /// el acento con el que ya contaban todas las cards de CFG.
    fn card_hex(&self, key: &str) -> u32 {
        let idx = CARD_KEYS
            .iter()
            .position(|k| *k == key)
            .unwrap_or_else(|| {
                CARD_KEYS
                    .iter()
                    .position(|k| *k == "terminal")
                    .unwrap_or(CARD_KEYS.len() - 1)
            });
        self.card_hexes[idx]
    }

    /// Color distintivo de una card (`key` sale de `CARD_KEYS`).
    pub fn card_accent(&self, key: &str) -> Color {
        hex(self.card_hex(key))
    }

    /// Fondo de esa misma card cuando está **seleccionada**: su acento apagado,
    /// con la receta de siempre (`dark()` al 27 %).
    pub fn card_bg(&self, key: &str) -> Color {
        dark(self.card_hex(key), 27)
    }
}

/// Tema por defecto: conserva el aspecto que tenía Compass antes de existir los
/// temas (Cyan/Yellow/Magenta + tinte de vidrio original).
pub const DEFAULT: &Theme = &THEMES[0];

/// Temario. El orden es el de `Ctrl+T`; `Compass` va primero a propósito para
/// que el ciclo empiece por el aspecto de siempre.
pub static THEMES: &[Theme] = &[
    // 0 — Compass (aspecto original)
    Theme {
        name: "Compass",
        text: Color::White,
        muted: Color::DarkGray,
        border: Color::DarkGray,
        brand: Color::Green,
        dev: Color::Cyan,
        mnt: Color::Yellow,
        cfg: Color::Magenta,
        hover_bg: hex(0x1e1e24),
        background: hex(0x16161e),
        success: Color::Green,
        info: Color::Cyan,
        accent: Color::Magenta,
        warning: Color::Yellow,
        // DEV·MNT dentro de la familia cyan/amarilla de siempre; CFG dentro del
        // magenta. Cada card se distingue por el tono, la pestaña por la familia.
        card_hexes: [
            0x00abcd, 0x00ffff, 0x5fd7ff, 0xafff00, 0xffd700, 0xffa500, 0xff87d7, 0x00d7af,
            0xffe79f, 0xffff00, 0xffcc2f, 0xff6f6f, 0xffb72f, 0xff4f4f, 0xff00ff, 0xd787ff,
            0xaf87ff, 0x00d7ff,
        ],
    },
    // 1 — OpenCode (tema oficial)
    Theme {
        name: "OpenCode",
        text: hex(0xeeeeee),
        muted: hex(0x808080),
        border: hex(0x484848),
        brand: hex(0xfab283),
        dev: hex(0x56b6c2),
        mnt: hex(0xf5a742),
        cfg: hex(0x9d7cd8),
        hover_bg: hex(0x1e1e1e),
        background: hex(0x131313),
        success: hex(0x7fd88f),
        info: hex(0x56b6c2),
        accent: hex(0x9d7cd8),
        warning: hex(0xf5a742),
        card_hexes: [
            0x56b6c2, 0x61afef, 0x98c379, 0xe5c07b, 0xe06c75, 0xff5c93, 0xd19a66, 0x7fd88f,
            0xef6c75, 0xc678dd, 0xf5a742, 0x9d7cd8, 0xfab283, 0xffb86c, 0xbd93f9, 0x2bbac5,
            0xa8a1ff, 0x89ddff,
        ],
    },
    // 2 — Tokyo Night
    Theme {
        name: "Tokyo Night",
        text: hex(0xc8d3f5),
        muted: hex(0x828bb8),
        border: hex(0x545c7e),
        brand: hex(0x82aaff),
        dev: hex(0x82aaff),
        mnt: hex(0xffc777),
        cfg: hex(0xc099ff),
        hover_bg: hex(0x222436),
        background: hex(0x1a1b26),
        success: hex(0xc3e88d),
        info: hex(0x86e1fc),
        accent: hex(0xc099ff),
        warning: hex(0xffc777),
        card_hexes: [
            0x7aa2f7, 0x86e1fc, 0xc3e88d, 0xffc777, 0xf7768e, 0xdb5c3c, 0x2ac3de, 0x73daca,
            0xbb9af7, 0xe0af68, 0xff9e64, 0x7dcfff, 0xffa29b, 0x7cf6b5, 0xb8b2fa, 0xad7ea8,
            0x9d7cd8, 0x2ac3de,
        ],
    },
    // 3 — Catppuccin (Mocha)
    Theme {
        name: "Catppuccin",
        text: hex(0xcdd6f4),
        muted: hex(0x7f849c),
        border: hex(0x313244),
        brand: hex(0x89b4fa),
        dev: hex(0x89b4fa),
        mnt: hex(0xf9e2af),
        cfg: hex(0xcba6f7),
        hover_bg: hex(0x313244),
        background: hex(0x1e1e2e),
        success: hex(0xa6e3a1),
        info: hex(0x94e2d5),
        accent: hex(0xcba6f7),
        warning: hex(0xf9e2af),
        card_hexes: [
            0x89b4fa, 0x94e2d5, 0xa6e3a1, 0xf9e2af, 0xf38ba8, 0xe78284, 0xf2cdcd, 0x94b8ff,
            0xf5c2e7, 0xfab387, 0xeba069, 0xb4befe, 0xcba6f7, 0xffa657, 0xa0c4ff, 0x7dcfff,
            0x89d88b, 0xffa657,
        ],
    },
    // 4 — Dracula
    Theme {
        name: "Dracula",
        text: hex(0xf8f8f2),
        muted: hex(0x6272a4),
        border: hex(0x44475a),
        brand: hex(0xbd93f9),
        dev: hex(0x8be9fd),
        mnt: hex(0xf1fa8c),
        cfg: hex(0xff79c6),
        hover_bg: hex(0x3a3c4e),
        background: hex(0x282a36),
        success: hex(0x50fa7b),
        info: hex(0xffb86c),
        accent: hex(0xff79c6),
        warning: hex(0xf1fa8c),
        card_hexes: [
            0x8be9fd, 0xbd93f9, 0x50fa7b, 0xf1fa8c, 0xff5555, 0xffb86c, 0x62d4a8, 0x3ee6c4,
            0xe3f9fb, 0x8aff80, 0xfabf9f, 0xff79c6, 0x69d0e7, 0x5aebad, 0xefe48d, 0xff92d0,
            0xf264c9, 0x8be9fd,
        ],
    },
    // 5 — Gruvbox (dark)
    Theme {
        name: "Gruvbox",
        text: hex(0xebdbb2),
        muted: hex(0x928374),
        border: hex(0x504945),
        brand: hex(0xb8bb26),
        dev: hex(0x83a598),
        mnt: hex(0xfabd2f),
        cfg: hex(0xd3869b),
        hover_bg: hex(0x3c3836),
        background: hex(0x282828),
        success: hex(0xb8bb26),
        info: hex(0x83a598),
        accent: hex(0xd3869b),
        warning: hex(0xfe8019),
        card_hexes: [
            0x83a598, 0x458588, 0xb8bb26, 0xfabd2f, 0xfb4934, 0xfe8019, 0xba5446, 0x8ec07c,
            0xdbe2d5, 0xa9b665, 0xf9f24a, 0xeceaea, 0xd3869b, 0xb16286, 0xd79921, 0xfbf1c7,
            0x689d6a, 0x458588,
        ],
    },
    // 6 — Nord
    Theme {
        name: "Nord",
        text: hex(0xeceff4),
        muted: hex(0x8b95a7),
        border: hex(0x434c5e),
        brand: hex(0x88c0d0),
        dev: hex(0x88c0d0),
        mnt: hex(0xebcb8b),
        cfg: hex(0xb48ead),
        hover_bg: hex(0x3b4252),
        background: hex(0x2e3440),
        success: hex(0xa3be8c),
        info: hex(0x81a1c1),
        accent: hex(0xb48ead),
        warning: hex(0xd08770),
        card_hexes: [
            0x88c0d0, 0x81a1c1, 0xa3be8c, 0xebcb8b, 0xbf616a, 0xd08770, 0xe5e9f0, 0x8fbcbb,
            0x8fa8d8, 0xb48ead, 0x5e81a7, 0x87afaf, 0xc07866, 0x57909c, 0xd899c4, 0xaeb7df,
            0xc6d0dd, 0x88c0d0,
        ],
    },
    // 7 — One Dark
    Theme {
        name: "One Dark",
        text: hex(0xabb2bf),
        muted: hex(0x5c6370),
        border: hex(0x393f4a),
        brand: hex(0x61afef),
        dev: hex(0x61afef),
        mnt: hex(0xe5c07b),
        cfg: hex(0xc678dd),
        hover_bg: hex(0x353b45),
        background: hex(0x282c34),
        success: hex(0x98c379),
        info: hex(0x56b6c2),
        accent: hex(0xc678dd),
        warning: hex(0xe5c07b),
        card_hexes: [
            0x61afef, 0x56b6c2, 0x3ece9d, 0xe5c07b, 0xe06c75, 0xd19a66, 0xbe5046, 0x98c379,
            0xc678dd, 0xb8d17a, 0x7ae0c0, 0xef7d57, 0x9d7cd8, 0xef6ac0, 0x8fa3bf, 0x25d366,
            0x7f848e, 0x61afef,
        ],
    },
    // 8 — Monokai
    Theme {
        name: "Monokai",
        text: hex(0xf8f8f2),
        muted: hex(0x75715e),
        border: hex(0x3e3d32),
        brand: hex(0xa6e22e),
        dev: hex(0x66d9ef),
        mnt: hex(0xe6db74),
        cfg: hex(0xae81ff),
        hover_bg: hex(0x3e3d32),
        background: hex(0x272822),
        success: hex(0xa6e22e),
        info: hex(0xfd971f),
        accent: hex(0xae81ff),
        warning: hex(0xe6db74),
        card_hexes: [
            0x66d9ef, 0xae81ff, 0xa6e22e, 0xe6db74, 0xf92672, 0xfd971f, 0xff4040, 0x2ee6a8,
            0x8fd1e4, 0xf5f5a0, 0x70e0d0, 0xff9ec9, 0xc06fe0, 0x5ef38c, 0xffcb3d, 0x8be04e,
            0xb8b8b8, 0x66d9ef,
        ],
    },
    // 9 — Rosé Pine
    Theme {
        name: "Rosé Pine",
        text: hex(0xe0def4),
        muted: hex(0x6e6a86),
        border: hex(0x403d52),
        brand: hex(0xebbcba),
        dev: hex(0x9ccfd8),
        mnt: hex(0xf6c177),
        cfg: hex(0xc4a7e7),
        hover_bg: hex(0x26233a),
        background: hex(0x191724),
        success: hex(0x9ccfd8),
        info: hex(0x9ccfd8),
        accent: hex(0xc4a7e7),
        warning: hex(0xf6c177),
        card_hexes: [
            0x9ccfd8, 0xc4a7e7, 0x31748f, 0xf6c177, 0xeb6c88, 0xe6976b, 0xf8bdc0, 0x8ec9a4,
            0xd8b4f0, 0xc2c2a8, 0x6bb8c4, 0xea84a2, 0xcba0f5, 0x5ec4a8, 0x9083a4, 0x6fcf9f,
            0xebbcba, 0x9ccfd8,
        ],
    },
    // 10 — Kanagawa
    Theme {
        name: "Kanagawa",
        text: hex(0xdcd7ba),
        muted: hex(0x727169),
        border: hex(0x54546d),
        brand: hex(0x7e9cd8),
        dev: hex(0x7e9cd8),
        mnt: hex(0xd7a657),
        cfg: hex(0x957fb8),
        hover_bg: hex(0x2a2a37),
        background: hex(0x1f1f28),
        success: hex(0x98bb6c),
        info: hex(0x76946a),
        accent: hex(0xd27e99),
        warning: hex(0xd7a657),
        card_hexes: [
            0x7e9cd8, 0x7fb4ca, 0x98bb6c, 0xd7a657, 0xe8246a, 0xcc8844, 0xd2787c, 0x80a66f,
            0xa3c48f, 0xc9b87a, 0x6da8c0, 0x76946a, 0x957fb8, 0xd4b08c, 0x5c8496, 0x8fd0a8,
            0xc0b8a0, 0x7e9cd8,
        ],
    },
    // 11 — Matrix
    Theme {
        name: "Matrix",
        text: hex(0x62ff94),
        muted: hex(0x8ca391),
        border: hex(0x1e2a1b),
        brand: hex(0x2eff6a),
        dev: hex(0x62ff94),
        mnt: hex(0xe6ff57),
        cfg: hex(0xc770ff),
        hover_bg: hex(0x141c12),
        background: hex(0x0a120a),
        success: hex(0x2eff6a),
        info: hex(0x30b3ff),
        accent: hex(0xc770ff),
        warning: hex(0xe6ff57),
        card_hexes: [
            0x62ff94, 0x30b3ff, 0x2eff6a, 0xe6ff57, 0xff4f7e, 0xff8a3d, 0x00e5b0, 0x7cffc4,
            0xb8ff6a, 0xffd800, 0x00b3a6, 0x57d8ff, 0xc770ff, 0x9dff7a, 0x00ffc8, 0xff6a6a,
            0xc0c0c0, 0x30b3ff,
        ],
    },
    // 12 — Synthwave '84
    Theme {
        name: "Synthwave 84",
        text: hex(0xffffff),
        muted: hex(0x848bbd),
        border: hex(0x495495),
        brand: hex(0xff7edb),
        dev: hex(0x36f9f6),
        mnt: hex(0xfede5d),
        cfg: hex(0xb084eb),
        hover_bg: hex(0x2a2139),
        background: hex(0x241b2f),
        success: hex(0x72f1b8),
        info: hex(0xff8b39),
        accent: hex(0xb084eb),
        warning: hex(0xfede5d),
        card_hexes: [
            0x36f9f6, 0xff7edb, 0x72f1b8, 0xfede5d, 0xfe44f6, 0xff8b39, 0xfc297b, 0x00d9ff,
            0xb084eb, 0xd8f97f, 0x5580ff, 0xff9de7, 0x9bf6ff, 0xffc0cb, 0x05d9e0, 0x39ff14,
            0xcd6fff, 0x36f9f6,
        ],
    },
    // 13 — Solarized (dark)
    Theme {
        name: "Solarized",
        text: hex(0x839496),
        muted: hex(0x586e75),
        border: hex(0x073642),
        brand: hex(0x268bd2),
        dev: hex(0x268bd2),
        mnt: hex(0xb58900),
        cfg: hex(0xd33682),
        hover_bg: hex(0x0d3a47),
        background: hex(0x002b36),
        success: hex(0x859900),
        info: hex(0x2aa198),
        accent: hex(0xd33682),
        warning: hex(0xb58900),
        card_hexes: [
            0x268bd2, 0x6c71c4, 0x2aa198, 0xb58900, 0xdc322f, 0xcb4b16, 0xd33682, 0x859900,
            0x5bbb9a, 0x6fd4c0, 0x93a1a1, 0x8fa8ff, 0xff79b0, 0xffd95e, 0x2b9dd0, 0x25d366,
            0x657b83, 0x268bd2,
        ],
    },
];

/// Busca un tema por nombre exacto (insensible a mayúsculas y espacios sobrantes).
pub fn by_name(name: &str) -> Option<&'static Theme> {
    let needle = name.trim();
    THEMES.iter().find(|t| t.name.eq_ignore_ascii_case(needle))
}

/// Tema siguiente en el ciclo (vuelve al primero tras el último).
pub fn next(name: &str) -> &'static Theme {
    let i = index_of(name);
    &THEMES[(i + 1) % THEMES.len()]
}

/// Tema anterior en el ciclo (`Ctrl+Shift+T`).
pub fn prev(name: &str) -> &'static Theme {
    let i = index_of(name);
    &THEMES[(i + THEMES.len() - 1) % THEMES.len()]
}

/// Índice de un tema en [`THEMES`] por nombre (0 si no se encuentra).
pub fn index_of(name: &str) -> usize {
    THEMES
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(0)
}
/// Ruta del fichero donde se guarda el nombre del tema activo.
fn config_path() -> Option<PathBuf> {
    Some(platform::data_dir()?.join("compass").join("theme"))
}

/// Tema guardado por el usuario, o [`DEFAULT`] si no hay ninguno válido.
///
/// Un fichero ilegible, ausente o con un nombre desconocido no es un error:
/// simplemente se arranca con el tema por defecto.
pub fn load() -> &'static Theme {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| by_name(&s))
        .unwrap_or(DEFAULT)
}

/// Guarda el tema activo. Un fallo de escritura no es fatal: el tema sigue
/// aplicado en la sesión actual, sólo se pierde al cerrar.
pub fn save(theme: &Theme) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, theme.name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_first_and_exists() {
        assert_eq!(DEFAULT.name, "Compass");
        assert_eq!(THEMES[0].name, DEFAULT.name);
    }

    #[test]
    fn names_are_unique() {
        for (i, a) in THEMES.iter().enumerate() {
            for b in &THEMES[i + 1..] {
                assert_ne!(a.name, b.name, "nombre de tema duplicado: {}", a.name);
            }
        }
    }

    #[test]
    fn next_cycles_through_every_theme_and_wraps() {
        let mut seen = Vec::new();
        let mut current = DEFAULT.name;
        for _ in 0..THEMES.len() {
            seen.push(current);
            current = next(current).name;
        }
        assert_eq!(current, DEFAULT.name, "el ciclo debe volver al inicio");
        // Visto exactamente una vez cada uno.
        seen.sort_unstable();
        let mut all: Vec<&str> = THEMES.iter().map(|t| t.name).collect();
        all.sort_unstable();
        assert_eq!(seen, all);
    }

    #[test]
    fn prev_is_the_inverse_of_next() {
        for t in THEMES {
            assert_eq!(prev(next(t.name).name).name, t.name, "{}", t.name);
        }
    }

    #[test]
    fn by_name_ignores_case_and_padding() {
        assert_eq!(by_name("  tokyo night ").unwrap().name, "Tokyo Night");
        assert!(by_name("no-existe").is_none());
    }

    #[test]
    fn sections_are_visually_distinct() {
        for t in THEMES {
            assert_ne!(t.dev, t.mnt, "{}: DEV y MNT comparten color", t.name);
            assert_ne!(t.dev, t.cfg, "{}: DEV y CFG comparten color", t.name);
            assert_ne!(t.mnt, t.cfg, "{}: MNT y CFG comparten color", t.name);
        }
    }

    #[test]
    fn text_is_readable_against_muted() {
        for t in THEMES {
            assert_ne!(
                t.text, t.muted,
                "{}: texto y texto atenuado iguales",
                t.name
            );
        }
    }

    /// El fondo de pantalla no debe coincidir ni con el texto (texto invisible)
    /// ni con el hover de card (el hover dejaría de verse).
    #[test]
    fn background_differs_from_text_and_hover() {
        for t in THEMES {
            assert_ne!(t.background, t.text, "{}: background == texto", t.name);
            assert_ne!(t.background, t.hover_bg, "{}: background == hover", t.name);
        }
    }

    #[test]
    fn section_lookup_matches_fields() {
        let t = DEFAULT;
        assert_eq!(t.section("DEV"), t.dev);
        assert_eq!(t.section("MNT"), t.mnt);
        assert_eq!(t.section("CFG"), t.cfg);
        // Desconocida cae en CFG.
        assert_eq!(t.section("???"), t.cfg);
    }

    /// Toda clave de [`CARD_KEYS`] debe resolver en su posición del arreglo, y
    /// una clave desconocida debe caer en `terminal` (el último acento).
    #[test]
    fn card_keys_map_to_their_position_in_the_palette() {
        assert_eq!(CARD_KEYS.len(), 18);
        assert_eq!(
            CARD_KEYS_DEV_MNT.len() + CARD_KEYS_CFG.len(),
            CARD_KEYS.len(),
            "las claves por pestaña deben cubrir CARD_KEYS sin huecos ni repeticiones"
        );
        let joined: Vec<&str> = CARD_KEYS_DEV_MNT
            .iter()
            .chain(CARD_KEYS_CFG.iter())
            .copied()
            .collect();
        assert_eq!(joined.as_slice(), CARD_KEYS.as_slice());
        for (i, k) in CARD_KEYS.iter().enumerate() {
            assert_eq!(DEFAULT.card_hex(k), DEFAULT.card_hexes[i], "posición de {k}");
        }
        assert_eq!(DEFAULT.card_hex("no-existe"), DEFAULT.card_hex("terminal"));
    }

    /// El peor caso es la pestaña 1 con 14 cards (GoWails + Tauri): si dos
    /// acentos coinciden, dos tarjetas se ven iguales y el color deja de
    /// identificar la acción.
    #[test]
    fn card_hexes_are_distinct_within_each_tab() {
        for t in THEMES {
            for group in [CARD_KEYS_DEV_MNT.as_slice(), CARD_KEYS_CFG.as_slice()] {
                for (i, a) in group.iter().enumerate() {
                    for b in &group[i + 1..] {
                        assert_ne!(
                            t.card_accent(a),
                            t.card_accent(b),
                            "{}: las cards {a} y {b} comparten acento",
                            t.name
                        );
                    }
                }
            }
        }
    }

    /// El hover no puede pintarse como una selección (fondo derivado de cada
    /// acento), y el fondo de la pantalla tampoco.
    #[test]
    fn card_bgs_never_collide_with_hover_or_background() {
        for t in THEMES {
            for key in CARD_KEYS {
                let bg = t.card_bg(key);
                assert_ne!(bg, t.hover_bg, "{}: hover == selección de {key}", t.name);
                assert_ne!(bg, t.background, "{}: background == selección de {key}", t.name);
                assert_ne!(
                    t.card_accent(key), t.background,
                    "{}: el acento de {key} se confunde con el fondo",
                    t.name
                );
            }
        }
    }

    /// `mix` es la base de la animación de hover: interpola de forma lineal y
    /// clampa fuera de `0..=1`.
    #[test]
    fn mix_interpolates_and_clamps() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(200, 100, 50);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, -1.0), a, "t<0 se clampa al origen");
        assert_eq!(mix(a, b, 2.0), b, "t>1 se clampa al destino");
        assert_eq!(mix(a, b, 0.5), Color::Rgb(100, 50, 25));
        // Con un color no-RGB (consola legacy) la animación no inventa colores.
        assert_eq!(mix(Color::Red, b, 0.3), b);
    }

    /// Cubo de matiz de 30° (0..11); los neutros forman el cubo 255.
    fn hue_bucket(c: Color) -> u8 {
        let (r, g, b) = match c {
            Color::Rgb(r, g, b) => (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0),
            _ => return 255,
        };
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;
        if chroma < 0.08 {
            return 255;
        }
        let h = if max == r {
            60.0 * (((g - b) / chroma) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / chroma + 2.0)
        } else {
            60.0 * ((r - g) / chroma + 4.0)
        };
        let h = (h + 360.0) % 360.0;
        (h as u16 / 30) as u8
    }

    /// El punto del rediseño es que **cada card tenga su propio color**. Si
    /// alguien vuelve a derivar los acentos de `dev`/`mnt`/`cfg`, las 14 cards
    /// de la pestaña 1 acabarían en dos o tres cubos de matiz y este test falla.
    #[test]
    fn card_accents_span_several_hue_families() {
        for t in THEMES {
            for group in [CARD_KEYS_DEV_MNT.as_slice(), CARD_KEYS_CFG.as_slice()] {
                let buckets: std::collections::BTreeSet<u8> =
                    group.iter().map(|k| hue_bucket(t.card_accent(k))).collect();
                let min = if group.len() == CARD_KEYS_DEV_MNT.len() {
                    4
                } else {
                    2
                };
                assert!(
                    buckets.len() >= min,
                    "{}: la pestaña de {} cards sólo usa {} familias de color",
                    t.name,
                    group.len(),
                    buckets.len()
                );
            }
        }
    }
}
