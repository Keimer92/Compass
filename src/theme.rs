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
//! | `dev_bg` `mnt_bg` `cfg_bg` | fondo de la card **seleccionada** por sección |
//! | `hover_bg` | fondo de la card **hovereada** (neutro, distinto de la selección) |
//! | `success` `info` `accent` `warning` | badge de tipo de proyecto y avisos |
//!
//! Los `*_bg` y el `hover_bg` no se derivan del resto: van explícitos por tema
//! porque el "vidrio esmerilado" de la selección es una decisión estética, no
//! una función. Aun así hay tests (`tests::hover_is_distinct_from_every_selection`)
//! que garantizan que hover y selección nunca colisionan en ningún tema.
//!
//! ## Persistencia
//!
//! El tema elegido con `Ctrl+T` se guarda como una línea de texto en
//! `<data_dir>/compass/theme` (`%LOCALAPPDATA%\compass\theme` en Windows,
//! `~/.local/share/compass/theme` en Linux). Si el fichero falta o el nombre no
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
    pub dev_bg: Color,
    pub mnt_bg: Color,
    pub cfg_bg: Color,
    pub hover_bg: Color,
    pub success: Color,
    pub info: Color,
    pub accent: Color,
    pub warning: Color,
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

    /// Fondo de la card **seleccionada** de una sección.
    pub fn selected_bg(&self, section: &str) -> Color {
        match section {
            "DEV" => self.dev_bg,
            "MNT" => self.mnt_bg,
            _ => self.cfg_bg,
        }
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
        dev_bg: hex(0x163444),
        mnt_bg: hex(0x443a16),
        cfg_bg: hex(0x3c1c48),
        hover_bg: hex(0x1e1e24),
        success: Color::Green,
        info: Color::Cyan,
        accent: Color::Magenta,
        warning: Color::Yellow,
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
        dev_bg: dark(0x56b6c2, 27),
        mnt_bg: dark(0xf5a742, 27),
        cfg_bg: dark(0x9d7cd8, 27),
        hover_bg: hex(0x1e1e1e),
        success: hex(0x7fd88f),
        info: hex(0x56b6c2),
        accent: hex(0x9d7cd8),
        warning: hex(0xf5a742),
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
        dev_bg: dark(0x82aaff, 27),
        mnt_bg: dark(0xffc777, 27),
        cfg_bg: dark(0xc099ff, 27),
        hover_bg: hex(0x222436),
        success: hex(0xc3e88d),
        info: hex(0x86e1fc),
        accent: hex(0xc099ff),
        warning: hex(0xffc777),
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
        dev_bg: dark(0x89b4fa, 27),
        mnt_bg: dark(0xf9e2af, 27),
        cfg_bg: dark(0xcba6f7, 27),
        hover_bg: hex(0x313244),
        success: hex(0xa6e3a1),
        info: hex(0x94e2d5),
        accent: hex(0xcba6f7),
        warning: hex(0xf9e2af),
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
        dev_bg: dark(0x8be9fd, 27),
        mnt_bg: dark(0xf1fa8c, 27),
        cfg_bg: dark(0xff79c6, 27),
        hover_bg: hex(0x3a3c4e),
        success: hex(0x50fa7b),
        info: hex(0xffb86c),
        accent: hex(0xff79c6),
        warning: hex(0xf1fa8c),
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
        dev_bg: dark(0x83a598, 27),
        mnt_bg: dark(0xfabd2f, 27),
        cfg_bg: dark(0xd3869b, 27),
        hover_bg: hex(0x3c3836),
        success: hex(0xb8bb26),
        info: hex(0x83a598),
        accent: hex(0xd3869b),
        warning: hex(0xfe8019),
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
        dev_bg: dark(0x88c0d0, 27),
        mnt_bg: dark(0xebcb8b, 27),
        cfg_bg: dark(0xb48ead, 27),
        hover_bg: hex(0x3b4252),
        success: hex(0xa3be8c),
        info: hex(0x81a1c1),
        accent: hex(0xb48ead),
        warning: hex(0xd08770),
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
        dev_bg: dark(0x61afef, 27),
        mnt_bg: dark(0xe5c07b, 27),
        cfg_bg: dark(0xc678dd, 27),
        hover_bg: hex(0x353b45),
        success: hex(0x98c379),
        info: hex(0x56b6c2),
        accent: hex(0xc678dd),
        warning: hex(0xe5c07b),
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
        dev_bg: dark(0x66d9ef, 27),
        mnt_bg: dark(0xe6db74, 27),
        cfg_bg: dark(0xae81ff, 27),
        hover_bg: hex(0x3e3d32),
        success: hex(0xa6e22e),
        info: hex(0xfd971f),
        accent: hex(0xae81ff),
        warning: hex(0xe6db74),
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
        dev_bg: dark(0x9ccfd8, 27),
        mnt_bg: dark(0xf6c177, 27),
        cfg_bg: dark(0xc4a7e7, 27),
        hover_bg: hex(0x26233a),
        success: hex(0x9ccfd8),
        info: hex(0x9ccfd8),
        accent: hex(0xc4a7e7),
        warning: hex(0xf6c177),
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
        dev_bg: dark(0x7e9cd8, 27),
        mnt_bg: dark(0xd7a657, 27),
        cfg_bg: dark(0x957fb8, 27),
        hover_bg: hex(0x2a2a37),
        success: hex(0x98bb6c),
        info: hex(0x76946a),
        accent: hex(0xd27e99),
        warning: hex(0xd7a657),
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
        dev_bg: dark(0x62ff94, 27),
        mnt_bg: dark(0xe6ff57, 27),
        cfg_bg: dark(0xc770ff, 27),
        hover_bg: hex(0x141c12),
        success: hex(0x2eff6a),
        info: hex(0x30b3ff),
        accent: hex(0xc770ff),
        warning: hex(0xe6ff57),
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
        dev_bg: dark(0x36f9f6, 27),
        mnt_bg: dark(0xfede5d, 27),
        cfg_bg: dark(0xb084eb, 27),
        hover_bg: hex(0x2a2139),
        success: hex(0x72f1b8),
        info: hex(0xff8b39),
        accent: hex(0xb084eb),
        warning: hex(0xfede5d),
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
        dev_bg: dark(0x268bd2, 27),
        mnt_bg: dark(0xb58900, 27),
        cfg_bg: dark(0xd33682, 27),
        hover_bg: hex(0x0d3a47),
        success: hex(0x859900),
        info: hex(0x2aa198),
        accent: hex(0xd33682),
        warning: hex(0xb58900),
    },
];

/// Busca un tema por nombre exacto (insensible a mayúsculas y espacios sobrantes).
pub fn by_name(name: &str) -> Option<&'static Theme> {
    let needle = name.trim();
    THEMES
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(needle))
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
}/// Ruta del fichero donde se guarda el nombre del tema activo.
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

    /// Regresión equivalente a la del menú, pero para **todos** los temas: el
    /// hover nunca debe pintarse igual que una card seleccionada, o dejaría de
    /// distinguirse (el bug que ya se arregló una vez con el tinte compartido).
    #[test]
    fn hover_is_distinct_from_every_selection() {
        for t in THEMES {
            assert_ne!(t.hover_bg, t.dev_bg, "{}: hover == selección DEV", t.name);
            assert_ne!(t.hover_bg, t.mnt_bg, "{}: hover == selección MNT", t.name);
            assert_ne!(t.hover_bg, t.cfg_bg, "{}: hover == selección CFG", t.name);
        }
    }

    #[test]
    fn text_is_readable_against_muted() {
        for t in THEMES {
            assert_ne!(t.text, t.muted, "{}: texto y texto atenuado iguales", t.name);
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
        assert_eq!(t.selected_bg("DEV"), t.dev_bg);
        assert_eq!(t.selected_bg("desconocida"), t.cfg_bg);
    }
}
