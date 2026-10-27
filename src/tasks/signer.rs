//! Firmas Android: la **fuente de verdad** de todo lo relacionado con el
//! keystore, en un solo módulo.
//!
//! Antes esto estaba repartido en tres módulos (`signer`, `keystore`, `sha1`)
//! más dos copias del mismo parseo en `kotlin.rs`, y con el layout del módulo
//! Android calculado en cuatro sitios distintos. Peor: `keystore.rs` para Kotlin
//! asumía siempre `composeApp/`, así que en un KMP con módulo `app/` el respaldo
//! miraba en la carpeta equivocada. Aquí se centraliza:
//!
//! | Pieza | Antes en |
//! |-------|----------|
//! | Layout (dir del módulo + nombre del `.properties`) | `keystore`, `signer`, `sha1`, `kotlin` |
//! | Parseo/escritura de `keystore.properties`/`key.properties` | `sha1`, `kotlin` |
//! | Crear / importar / manual + parcheo de Gradle | `signer` |
//! | Huellas SHA-1/SHA-256 | `sha1` |
//! | Respaldar / restaurar / listar | `keystore` |
//!
//! Sobre esa base se añaden: **estado/autodetección** de la firma, un **reporte
//! de secretos** para GitHub Actions (escaneando `.github/workflows`) y la
//! **verificación de un APK** con `apksigner` contra el keystore configurado.
//!
//! Todo se ofrece desde un único submenú, [`manage`].

use std::io::{self};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use chrono::Local;
use crossterm::{
    event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use inquire::Select;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame, Terminal,
};
use regex::Regex;

use crate::theme::{self, Theme};
use crate::utils::{
    cmd,
    context::{kotlin_android_dir, ProjectContext, ProjectType},
    platform, ui,
};

// ───────────────────────────────────────────────────────────────────────────
// Layout y properties
// ───────────────────────────────────────────────────────────────────────────

/// Dónde vive la firma de un proyecto Android.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidLayout {
    /// Directorio del módulo Android (`android/app`, `.../gen/android/app`,
    /// `composeApp`, `app` o `build/android/app` en Wails).
    pub module_dir: PathBuf,
    /// Nombre del archivo de propiedades: `key.properties` (Flutter) o
    /// `keystore.properties` (Tauri/KMP/Wails).
    pub props_name: &'static str,
    /// Carpeta donde viven el keystore y el `.properties`.
    ///
    /// Normalmente es `module_dir`. En **Go + Wails v3** es una carpeta aparte
    /// (`android-signing/`): el módulo `build/android/` lo regenera
    /// `wails3 update build-assets`, y perder el keystore es irrecuperable.
    pub signing_dir: PathBuf,
    /// `true` si hay que parchear el `build.gradle` para que use el keystore.
    ///
    /// En Wails v3 **no**: su Gradle ya lee `ANDROID_KEYSTORE_FILE` y
    /// compañía del entorno, así que Compass se limita a exportarlas al
    /// compilar (ver `tasks::gowails::signing_env`).
    pub patch_gradle: bool,
}

impl AndroidLayout {
    pub fn props_path(&self) -> PathBuf {
        self.signing_dir.join(self.props_name)
    }
}

/// Layout del proyecto, o `None` para Rust (no aplica).
///
/// Es el único sitio que conoce las rutas por tipo de proyecto. Todo lo demás
/// (estado, respaldos, huellas, parcheo) parte de aquí.
pub fn android_layout(ctx: &ProjectContext) -> Option<AndroidLayout> {
    let layout = match ctx.project_type {
        ProjectType::Tauri => AndroidLayout {
            module_dir: ctx.script_dir.join("src-tauri").join("gen").join("android").join("app"),
            props_name: "keystore.properties",
            signing_dir: ctx.script_dir.join("src-tauri").join("gen").join("android").join("app"),
            patch_gradle: true,
        },
        ProjectType::Flutter => AndroidLayout {
            module_dir: ctx.script_dir.join("android").join("app"),
            props_name: "key.properties",
            signing_dir: ctx.script_dir.join("android").join("app"),
            patch_gradle: true,
        },
        ProjectType::Kotlin => AndroidLayout {
            // `kotlin_android_dir` ya distingue `app/` de `composeApp/`; antes
            // aquí se asumía `composeApp` a secas (bug para KMP con `app/`).
            module_dir: kotlin_android_dir(ctx),
            props_name: "keystore.properties",
            signing_dir: kotlin_android_dir(ctx),
            patch_gradle: true,
        },
        ProjectType::Rust => return None,
        // Wails v3: el módulo Android es `build/android/app` y su Gradle ya
        // lee la firma del entorno (`ANDROID_KEYSTORE_FILE`, ...), así que no
        // se parchea. El keystore y el `.properties` viven en
        // `android-signing/`, fuera de `build/`, porque `wails3 update
        // build-assets` regenera esa carpeta.
        ProjectType::GoWails => AndroidLayout {
            module_dir: ctx.script_dir.join("build").join("android").join("app"),
            props_name: "keystore.properties",
            signing_dir: ctx.script_dir.join("android-signing"),
            patch_gradle: false,
        },
    };
    Some(layout)
}

/// Campos de `keystore.properties` / `key.properties`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Props {
    pub store_file: Option<String>,
    pub store_password: Option<String>,
    pub key_alias: Option<String>,
    pub key_password: Option<String>,
}

impl Props {
    /// Parseo tolerante: acepta los alias de clave usados por Flutter
    /// (`storePassword`/`keyPassword`/`keyAlias`/`storeFile`) y los de Tauri/KMP
    /// (`storepass`/`keypass`/`alias`).
    pub fn parse(text: &str) -> Self {
        let mut p = Props::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "storeFile" => p.store_file = Some(v),
                "storePassword" | "storepass" => p.store_password = Some(v),
                "keyAlias" | "alias" => p.key_alias = Some(v),
                "keyPassword" | "keypass" => p.key_password = Some(v),
                _ => {}
            }
        }
        p
    }

    pub fn read(path: &Path) -> Option<Self> {
        fs::read_to_string(path).ok().map(|t| Props::parse(&t))
    }

    /// Contenido del `.properties`, en el orden que ya usaba el resto del crate.
    pub fn render(&self) -> String {
        format!(
            "storePassword={}\nkeyPassword={}\nkeyAlias={}\nstoreFile={}\n",
            self.store_password.clone().unwrap_or_default(),
            self.key_password.clone().unwrap_or_default(),
            self.key_alias.clone().unwrap_or_default(),
            self.store_file.clone().unwrap_or_default(),
        )
    }
}

/// Resuelve el `storeFile` de las propiedades a una ruta real.
///
/// El valor puede ser absoluto o relativo a la carpeta de firma (que es como
/// lo escribe este módulo y como lo espera Gradle).
pub fn resolve_keystore(signing_dir: &Path, store_file: &str) -> PathBuf {
    let p = PathBuf::from(store_file);
    if p.is_absolute() {
        p
    } else {
        signing_dir.join(store_file)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Autodetección
// ───────────────────────────────────────────────────────────────────────────

/// Estado de la firma release, ya resuelto y listo para mostrar.
#[derive(Debug, Clone)]
pub struct SigningInfo {
    pub layout: AndroidLayout,
    /// `None` si el `.properties` no existe.
    pub props: Option<Props>,
    /// Ruta absoluta al keystore, sólo si el archivo existe.
    pub keystore: Option<PathBuf>,
    /// `storeFile` declarado en las properties pero inexistente en disco.
    pub keystore_missing: Option<String>,
}

impl SigningInfo {
    pub fn alias(&self) -> Option<&str> {
        self.props.as_ref().and_then(|p| p.key_alias.as_deref())
    }
    pub fn store_password(&self) -> Option<&str> {
        self.props.as_ref().and_then(|p| p.store_password.as_deref())
    }
    /// Contraseña de la llave; si falta, la de almacén (caso habitual).
    pub fn key_password(&self) -> Option<String> {
        self.props.as_ref().and_then(|p| {
            p.key_password
                .clone()
                .or_else(|| p.store_password.clone())
        })
    }
}

/// Reúne el estado de la firma a partir de lo que hay en disco.
pub fn detect(ctx: &ProjectContext) -> Option<SigningInfo> {
    let layout = android_layout(ctx)?;
    let props_path = layout.props_path();
    let props = Props::read(&props_path);

    let (keystore, keystore_missing) = match props.as_ref().and_then(|p| p.store_file.clone()) {
        Some(sf) if !sf.is_empty() => {
            let path = resolve_keystore(&layout.signing_dir, &sf);
            if path.exists() {
                (Some(path), None)
            } else {
                (None, Some(sf))
            }
        }
        _ => (None, None),
    };

    Some(SigningInfo { layout, props, keystore, keystore_missing })
}

/// Grado de parcheo del `build.gradle(.kts)` del módulo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradleSigning {
    /// No hay `build.gradle` ni `build.gradle.kts`.
    NoFile,
    /// Existe pero no tiene nuestro bloque de firma.
    Unpatched,
    /// Ya tiene el loader + `signingConfigs`.
    Patched,
}

/// Comprueba si el Gradle del módulo ya tiene el bloque de firma inyectado.
pub fn gradle_signing_status(module_dir: &Path) -> GradleSigning {
    let kts = module_dir.join("build.gradle.kts");
    let groovy = module_dir.join("build.gradle");
    let path = if kts.exists() {
        kts
    } else if groovy.exists() {
        groovy
    } else {
        return GradleSigning::NoFile;
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return GradleSigning::NoFile;
    };
    let has_loader = content.contains("keystorePropertiesFile");
    let has_configs = content.contains("signingConfigs {") || content.contains("signingConfigs{");
    if has_loader && has_configs {
        GradleSigning::Patched
    } else {
        GradleSigning::Unpatched
    }
}

// ───────────────────────────────────────────────────────────────────────────
// keytool y huellas
// ───────────────────────────────────────────────────────────────────────────

pub(crate) fn find_keytool() -> Result<String> {
    let mut paths: Vec<String> = vec!["keytool".to_string()];
    let kt = platform::exe_name("keytool");

    // JAVA_HOME es portable y es la vía más fiable en ambas plataformas.
    if let Ok(jh) = env::var("JAVA_HOME") {
        paths.push(
            Path::new(&jh)
                .join("bin")
                .join(&kt)
                .to_string_lossy()
                .into_owned(),
        );
    }

    if platform::IS_WINDOWS {
        paths.push(format!(r"C:\Program Files\Android\Android Studio\jbr\bin\{kt}"));
        paths.push(format!(r"C:\Program Files\Android\Android Studio\jre\bin\{kt}"));

        for base in &[r"C:\Program Files\Java", r"C:\Program Files (x86)\Java"] {
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    let k = entry.path().join("bin").join(&kt);
                    if k.exists() {
                        paths.push(k.to_string_lossy().into_owned());
                    }
                }
            }
        }

        if let Ok(local) = env::var("LOCALAPPDATA") {
            for jbr in ["jbr", "jre"] {
                let c = format!("{}\\Programs\\Android Studio\\{}\\bin\\{}", local, jbr, kt);
                if Path::new(&c).exists() {
                    paths.push(c);
                }
            }
        }
    } else {
        // Linux: el JDK suele estar en /usr/lib/jvm/<distro>-<version>/
        if let Ok(entries) = fs::read_dir("/usr/lib/jvm") {
            for entry in entries.flatten() {
                let k = entry.path().join("bin").join(&kt);
                if k.exists() {
                    paths.push(k.to_string_lossy().into_owned());
                }
            }
        }
    }

    for p in &paths {
        if p.is_empty() {
            continue;
        }
        if Command::new(p).arg("-help").output().is_ok() {
            return Ok(p.clone());
        }
    }

    anyhow::bail!(
        "No se pudo encontrar '{}'.\n\
         Asegúrate de tener un JDK (o Android Studio) instalado,\n\
         o agrega Java a tus variables de entorno (PATH/JAVA_HOME).",
        kt
    )
}

/// Huellas de un keystore ya resuelto.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fingerprints {
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub md5: Option<String>,
}

impl Fingerprints {
    pub fn is_empty(&self) -> bool {
        self.sha1.is_none() && self.sha256.is_none() && self.md5.is_none()
    }
}

/// Normaliza una huella para comparar: sin `:` y en minúsculas.
fn normalize_fingerprint(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Ejecuta `keytool -list -v` y extrae las huellas del certificado.
pub fn keystore_fingerprints(keytool: &str, keystore: &Path, store_pass: &str) -> Result<Fingerprints> {
    let out = Command::new(keytool)
        .args(["-list", "-v"])
        .arg("-keystore")
        .arg(keystore)
        .args(["-storepass", store_pass])
        .output()
        .context("Error al ejecutar keytool")?;

    if !out.status.success() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let msg = combined
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("error desconocido");
        anyhow::bail!("No se pudo leer el keystore: {}", msg);
    }

    Ok(parse_fingerprints(&String::from_utf8_lossy(&out.stdout)))
}

/// Extrae SHA-1/SHA-256/MD5 del texto de `keytool -list -v`.
pub fn parse_fingerprints(text: &str) -> Fingerprints {
    let mut fp = Fingerprints::default();
    for line in text.lines() {
        let t = line.trim();
        for prefix in ["SHA1:", "SHA256:", "MD5:", "SHA-1:", "SHA-256:"] {
            if let Some(rest) = t.strip_prefix(prefix) {
                let name = prefix.trim_end_matches(':').replace('-', "");
                let value = rest.trim().to_string();
                match name.as_str() {
                    "SHA1" => fp.sha1 = Some(value),
                    "SHA256" => fp.sha256 = Some(value),
                    _ => fp.md5 = Some(value),
                }
            }
        }
    }
    fp
}

/// Keystore de debug por defecto (`~/.android/debug.keystore`).
fn debug_keystore() -> Option<(PathBuf, &'static str, &'static str)> {
    let path = platform::home_dir()?.join(".android").join("debug.keystore");
    if path.exists() {
        Some((path, "androiddebugkey", "android"))
    } else {
        None
    }
}

// ───────────────────────────────────────────────────────────────────────────
// base64 (sin dependencias)
// ───────────────────────────────────────────────────────────────────────────

/// Codifica en base64 estándar (con relleno `=`), sin dependencias externas.
///
/// Se usa para el secreto `ANDROID_KEYSTORE_BASE64` que esperan los workflows
/// típicos: el keystore es binario y GitHub Secrets guarda texto.
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ───────────────────────────────────────────────────────────────────────────
// Secretos para GitHub Actions
// ───────────────────────────────────────────────────────────────────────────

/// Un secreto con su valor propuesto y de dónde sale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretGuess {
    pub name: String,
    pub value: String,
    pub source: &'static str,
}

/// Nombres de secreto referenciados en los workflows (`secrets.NOMBRE`).
///
/// Escanea `.github/workflows` recursivamente (`.yml`/`.yaml`) y devuelve los
/// nombres únicos, ordenados.
pub fn workflow_secret_names(root: &Path) -> Vec<String> {
    let dir = root.join(".github").join("workflows");
    let files = collect_files(&dir);
    let re = Regex::new(r"secrets\.([A-Za-z_][A-Za-z0-9_]*)").expect("regex válida");
    let mut names: Vec<String> = Vec::new();
    for f in files {
        let Ok(text) = fs::read_to_string(&f) else {
            continue;
        };
        for cap in re.captures_iter(&text) {
            let n = cap[1].to_string();
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    names.sort();
    names
}

fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(collect_files(&p));
        } else if matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("yml") | Some("yaml")
        ) {
            out.push(p);
        }
    }
    out
}

/// Empareja un nombre de secreto con el valor local que le corresponde.
///
/// El orden de las comprobaciones importa: `ANDROID_KEYSTORE_PASSWORD` contiene
/// `KEYSTORE`, así que si se mirara el keystore primero se mandaría el base64 en
/// lugar de la contraseña. Por eso las contraseñas se comprueban antes.
fn guess_secret(name: &str, info: &SigningInfo, keystore_b64: Option<&str>) -> SecretGuess {
    let u = name.to_ascii_uppercase();
    let store = info.store_password().unwrap_or_default().to_string();
    let keyp = info.key_password().unwrap_or_default();
    let alias = info.alias().unwrap_or_default().to_string();

    let (value, source) = if u.contains("BASE64") || u.ends_with("B64") {
        match keystore_b64 {
            Some(b) => (b.to_string(), "keystore (base64)"),
            None => (String::new(), "keystore no encontrado"),
        }
    } else if u.contains("ALIAS") {
        (alias, "keyAlias")
    } else if u.contains("KEYSTORE_PASSWORD") || u.contains("STORE_PASS") || u.contains("STOREPASS") {
        (store, "storePassword")
    } else if u.contains("KEY_PASSWORD") || u.contains("KEYPASS") {
        (keyp, "keyPassword")
    } else if u.contains("PASSWORD") {
        (store, "storePassword")
    } else if u.contains("KEYSTORE") || u.contains("STORE_FILE") {
        match keystore_b64 {
            Some(b) => (b.to_string(), "keystore (base64)"),
            None => (String::new(), "keystore no encontrado"),
        }
    } else {
        (String::new(), "sin datos")
    };

    SecretGuess { name: name.to_string(), value, source }
}

// ───────────────────────────────────────────────────────────────────────────
// TUI de cards para el menú de firmas (mismo estilo que el menú principal)
// ───────────────────────────────────────────────────────────────────────────

static TRACE: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
fn trace_event(msg: &str) {
    if !TRACE.load(Ordering::Relaxed) {
        return;
    }
    let log = std::env::temp_dir().join("compass-signing-tui.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        use std::io::Write;
        let _ = writeln!(f, "{msg}");
    }
}

const SIGNING_COLS: usize = 5;
const SIGNING_CARD_W: u16 = 26;
const SIGNING_CARD_H: u16 = 5;
const SIGNING_FOOTER_H: u16 = 1;

/// Opciones del submenú de firmas, en el orden en que aparecen en pantalla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningItem {
    Status,
    CreateKeystore,
    ImportKeystore,
    ManualConfig,
    Fingerprints,
    GithubSecrets,
    Backup,
    Restore,
    ListBackups,
    VerifyApk,
    Back,
}

impl SigningItem {
    fn icon(&self) -> &str {
        match self {
            Self::Status => "\u{1f4ca}",
            Self::CreateKeystore => "\u{1f511}",
            Self::ImportKeystore => "\u{1f4e4}",
            Self::ManualConfig => "\u{270f}",
            Self::Fingerprints => "\u{1f532}",
            Self::GithubSecrets => "\u{1f510}",
            Self::Backup => "\u{1f4be}",
            Self::Restore => "\u{1f5d1}",
            Self::ListBackups => "\u{1f4cb}",
            Self::VerifyApk => "\u{2705}",
            Self::Back => "\u{21a9}",
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Status => "Estado de la firma",
            Self::CreateKeystore => "Crear keystore",
            Self::ImportKeystore => "Importar keystore",
            Self::ManualConfig => "Configuracion manual",
            Self::Fingerprints => "Huellas (SHA-1/SHA-256)",
            Self::GithubSecrets => "Secretos de GitHub Actions",
            Self::Backup => "Respaldar keystore",
            Self::Restore => "Restaurar respaldo",
            Self::ListBackups => "Listar respaldos",
            Self::VerifyApk => "Verificar APK",
            Self::Back => "Volver al menu principal",
        }
    }

    fn description(&self) -> &str {
        match self {
            Self::Status => "Autodeteccion de keystore y Gradle",
            Self::CreateKeystore => "Generar un keystore nuevo",
            Self::ImportKeystore => "Importar keystore existente",
            Self::ManualConfig => "Escribir keystore.properties a mano",
            Self::Fingerprints => "Huellas SHA-1, SHA-256 y MD5",
            Self::GithubSecrets => "Valores para secretos de CI",
            Self::Backup => "Copia de keystore + properties",
            Self::Restore => "Restaurar desde un respaldo",
            Self::ListBackups => "Ver respaldos disponibles",
            Self::VerifyApk => "apksigner vs keystore configurado",
            Self::Back => "Regresa al menu principal",
        }
    }

    fn section(&self) -> &str {
        "MNT"
    }
}

/// Resultado de un evento del TUI de firmas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningAction {
    Run(SigningItem),
    Quit,
}

/// Estado del navegador de cards de firmas.
#[derive(Debug, Clone)]
pub struct SigningState {
    pub items: Vec<SigningItem>,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub theme: Theme,
    pub trace: bool,
}

impl SigningState {
    pub fn new(theme: Theme) -> Self {
        let items = vec![
            SigningItem::Status,
            SigningItem::CreateKeystore,
            SigningItem::ImportKeystore,
            SigningItem::ManualConfig,
            SigningItem::Fingerprints,
            SigningItem::GithubSecrets,
            SigningItem::Backup,
            SigningItem::Restore,
            SigningItem::ListBackups,
            SigningItem::VerifyApk,
            SigningItem::Back,
        ];
        Self {
            items,
            selected: 0,
            hovered: None,
            theme,
            trace: false,
        }
    }

    fn prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn next(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
        }
    }
}

/// Dimensiones de un grupo de cards (misma logica que el menu principal).
#[derive(Debug, Clone, Copy)]
struct GroupSize {
    cards: usize,
    rows: usize,
    height: u16,
}

fn calc_signing_group_sizes(cards: &[SigningItem]) -> Vec<GroupSize> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < cards.len() {
        let mut count = 0;
        while start + count < cards.len() && count < SIGNING_COLS {
            count += 1;
        }
        let rows = (count as u16 + SIGNING_COLS as u16 - 1) / SIGNING_COLS as u16;
        let height = rows * SIGNING_CARD_H + 1;
        groups.push(GroupSize { cards: count, rows: rows as usize, height });
        start += count;
    }
    groups
}

fn calc_signing_total_height(cards: &[SigningItem]) -> u16 {
    let groups = calc_signing_group_sizes(cards);
    groups.iter().map(|g| g.height).sum::<u16>() + (groups.len().saturating_sub(1) as u16)
}

/// Crea una card de ratatui para un elemento de firmas.
fn create_signing_card(
    item: SigningItem,
    is_selected: bool,
    is_hovered: bool,
    theme: &Theme,
) -> Paragraph<'static> {
    let section_color = theme.section(item.section());
    let icon = item.icon();
    let label = item.label();
    let desc = item.description();

    let mut lines: Vec<Line<'static>> = Vec::new();

    let mut line1_spans: Vec<Span<'static>> = vec![Span::raw(format!(" {icon}  "))];
    if is_selected {
        line1_spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(section_color).add_modifier(Modifier::BOLD),
        ));
    } else if is_hovered {
        line1_spans.push(Span::styled(label.to_string(), Style::default().fg(theme.text)));
    } else {
        line1_spans.push(Span::styled(label.to_string(), Style::default().fg(theme.muted)));
    }
    lines.push(Line::from(line1_spans));

    lines.push(Line::from(Span::styled(
        format!("   {desc}"),
        Style::default().fg(theme.muted),
    )));

    lines.push(Line::from(Span::styled(
        " ".repeat(SIGNING_CARD_W as usize - 2),
        Style::default().fg(theme.border),
    )));

    let indicator = if is_selected {
        ">>> SELECCIONADO <<<"
    } else if is_hovered {
        "--- hover ---"
    } else {
        ""
    };
    if !indicator.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {indicator}"),
            Style::default().fg(section_color).add_modifier(Modifier::BOLD),
        )));
    }

    let border_style = if is_selected {
        Style::default().fg(section_color).add_modifier(Modifier::BOLD)
    } else if is_hovered {
        Style::default().fg(theme.text)
    } else {
        Style::default().fg(theme.border)
    };

    let bg = if is_selected {
        Some(theme.selected_bg(item.section()))
    } else if is_hovered {
        Some(theme.hover_bg)
    } else {
        None
    };

    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .style(Style::default().bg(bg.unwrap_or(Color::Reset))),
        )
        .alignment(Alignment::Left)
}

/// Calcula en que card cae una posicion del raton (x, y).
fn signing_card_at(cards: &[SigningItem], x: u16, y: u16, area: Rect) -> Option<usize> {
    let groups = calc_signing_group_sizes(cards);
    let total_h = calc_signing_total_height(cards);

    let content_width = area.width as usize;
    let content_height = area.height as usize;

    let card_w = SIGNING_CARD_W as usize;
    let card_h = SIGNING_CARD_H as usize;
    let cols = SIGNING_COLS;

    let total_card_w = cols * card_w;
    let total_card_h = total_h as usize;

    let offset_x = content_width.saturating_sub(total_card_w) / 2;
    let offset_y = content_height.saturating_sub(total_card_h) / 2;

    if x < offset_x as u16 || x >= (offset_x + total_card_w) as u16 {
        return None;
    }
    if y < offset_y as u16 || y >= (offset_y + total_card_h) as u16 {
        return None;
    }

    let rel_x = (x as usize) - offset_x;
    let rel_y = (y as usize) - offset_y;

    let col = rel_x / card_w;
    let _row_in_group = rel_y / card_h;

    let mut card_index = 0;
    let mut y_acc = 0;
    for group in &groups {
        if rel_y < y_acc + group.height as usize {
            let row_in_group_adj = rel_y - y_acc;
            let card_row = row_in_group_adj / card_h as usize;
            if card_row < group.rows as usize {
                let idx = card_row * cols + col;
                if idx < group.cards {
                    return Some(card_index + idx);
                }
            }
            return None;
        }
        y_acc += group.height as usize;
        card_index += group.cards;
    }

    None
}

/// Dibuja todas las cards de firmas en el frame.
fn render_signing_cards(
    frame: &mut Frame,
    area: Rect,
    cards: &[SigningItem],
    state: &SigningState,
) {
    let groups = calc_signing_group_sizes(cards);
    let total_h = calc_signing_total_height(cards);

    let content_width = area.width as usize;
    let content_height = area.height as usize;

    let card_w = SIGNING_CARD_W as usize;
    let card_h = SIGNING_CARD_H as usize;
    let cols = SIGNING_COLS;

    let total_card_w = cols * card_w;
    let total_card_h = total_h as usize;

    let offset_x = content_width.saturating_sub(total_card_w) / 2;
    let offset_y = content_height.saturating_sub(total_card_h) / 2;

    let mut card_index = 0;
    let mut y_acc = 0;
    for group in &groups {
        let x_start = offset_x as u16;
        let y_start = (offset_y as u16).saturating_add(y_acc as u16);

        for i in 0..group.cards {
            let col = i % cols;
            let row = i / cols;
            let card_x = x_start.saturating_add((col * card_w) as u16);
            let card_y = y_start.saturating_add((row * card_h) as u16);
            let card_area = Rect::new(card_x, card_y, card_w as u16, card_h as u16);

            let item = cards[card_index];
            let is_selected = card_index == state.selected;
            let is_hovered = state.hovered.map_or(false, |h| h == card_index);

            let card = create_signing_card(item, is_selected, is_hovered, &state.theme);
            frame.render_widget(card, card_area);
            card_index += 1;
        }
        y_acc += group.height as usize;
    }
}

/// Configura la terminal para el TUI de firmas.
fn setup_signing_terminal() -> anyhow::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restaura la terminal despues del TUI de firmas.
fn restore_signing_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Dibuja el menu completo de firmas con cabeca y pie.
fn draw_signing_menu(frame: &mut Frame, state: &mut SigningState, ctx: &ProjectContext) {
    let area = frame.area();
    let theme = state.theme;

    // Fondo de toda la pantalla (como el menu principal).
    let background = Block::default().style(Style::default().bg(theme.background));
    frame.render_widget(background, area);

    // Cabecera: titulo
    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            "  FIRMAS / KEYSTORE  ",
            Style::default().fg(theme.brand).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("  Proyecto: {}", ctx.script_dir.display()),
            Style::default().fg(theme.muted),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );
    let title_area = Rect::new(area.left() + 2, area.top() + 1, area.width.saturating_sub(4), 5);
    frame.render_widget(title, title_area);

    // Cards
    let cards_area = Rect::new(
        area.left(),
        area.top() + 6,
        area.width,
        area.height.saturating_sub(6 + SIGNING_FOOTER_H),
    );
    render_signing_cards(frame, cards_area, &state.items, state);

    // Pie: instrucciones
    let footer_text = "  Flechas / WASD: navegar  Enter: ejecutar  Esc: volver";
    let footer = Paragraph::new(Span::styled(
        footer_text,
        Style::default().fg(theme.muted),
    ));
    let footer_area = Rect::new(
        area.left() + 2,
        area.top() + area.height.saturating_sub(SIGNING_FOOTER_H + 1),
        area.width.saturating_sub(4),
        SIGNING_FOOTER_H,
    );
    frame.render_widget(footer, footer_area);
}

/// Maneja eventos de teclado y raton del TUI de firmas.
fn handle_signing_event(
    state: &mut SigningState,
    area: Rect,
) -> anyhow::Result<SigningAction> {
    if crossterm::event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = crossterm::event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(SigningAction::Quit),
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => { state.prev(); return Ok(SigningAction::Run(state.items[state.selected])); }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => { state.next(); return Ok(SigningAction::Run(state.items[state.selected])); }
                    KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => { state.prev(); return Ok(SigningAction::Run(state.items[state.selected])); }
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => { state.next(); return Ok(SigningAction::Run(state.items[state.selected])); }
                    KeyCode::Enter => {
                        let item = state.items[state.selected];
                        return Ok(SigningAction::Run(item));
                    }
                    _ => { return Ok(SigningAction::Run(state.items[state.selected])); }
                }
            }
        } else if let Event::Mouse(mouse) = crossterm::event::read()? {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(idx) = signing_card_at(&state.items, mouse.column, mouse.row, area) {
                        state.selected = idx;
                        state.hovered = Some(idx);
                        return Ok(SigningAction::Run(state.items[idx]));
                    } else {
                        // No card at mouse position, do nothing
                        return Ok(SigningAction::Quit);
                    }
                }
                MouseEventKind::Moved => {
                    if let Some(idx) = signing_card_at(&state.items, mouse.column, mouse.row, area) {
                        state.hovered = Some(idx);
                        return Ok(SigningAction::Run(state.items[state.selected]));
                    } else {
                        return Ok(SigningAction::Run(state.items[state.selected]));
                    }
                }
                _ => { return Ok(SigningAction::Run(state.items[state.selected])); }
            }
        }
        // Si no hubo evento reconocido, continuamos el loop sin ejecutar nada
        // (no retornamos SigningAction::Run para evitar ejecuciones automáticas)
    }
    // Solo retornamos None para indicar "ningún evento procesado", el caller decidirá
    // Si el caller quiere cerrar, debe manejarlo aparte
    //
                       
    // Regresamos un resultado que indica "no hay evento" para que el caller sepa continuar
    // Pero como la firma requiere un Result<SigningAction>, usamos un enfoque diferente:
    // Retornamos Ok con una acción especial o usamos un Option. 
    // Para mantener compatibilidad, retornamos la acción actual solo si hubo teclas específicas.
    // Actually, the simplest fix: remove the unconditional return at the end.
}

/// Ejecuta la accion de firmas seleccionada.
fn run_signing_action(item: SigningItem, ctx: &ProjectContext) -> Result<()> {
    match item {
        SigningItem::Status => show_status(ctx),
        SigningItem::CreateKeystore => create_keystore(ctx),
        SigningItem::ImportKeystore => import_keystore(ctx),
        SigningItem::ManualConfig => manual_configure(ctx),
        SigningItem::Fingerprints => show_fingerprints(ctx),
        SigningItem::GithubSecrets => show_secrets(ctx),
        SigningItem::Backup => backup_keystore(ctx),
        SigningItem::Restore => restore_keystore(ctx),
        SigningItem::ListBackups => list_backups(ctx),
        SigningItem::VerifyApk => verify_apk(ctx),
        SigningItem::Back => Ok(()),
    }
}

/// Submenú único de firmas (antes: tarjetas Configure + Keystore + SHA1),
/// ahora con la misma interfaz de cards clickables del menú principal.
pub fn manage(ctx: &ProjectContext) -> Result<()> {
    if ctx.project_type == ProjectType::Rust {
        cmd::clear_screen();
        println!("[INFO] No aplicable a proyectos Rust.");
        ui::pause();
        return Ok(());
    }

    let theme = theme::load();
    let mut state = SigningState::new(*theme);
    state.trace = TRACE.load(Ordering::Relaxed);

    let mut terminal = setup_signing_terminal()?;
    cmd::set_console_title(&platform::dir_display_name(&ctx.script_dir));

    let result = (|| -> anyhow::Result<SigningAction> {
        loop {
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            terminal.draw(|f| draw_signing_menu(f, &mut state, ctx))?;

            match handle_signing_event(&mut state, area)? {
                SigningAction::Quit => return Ok(SigningAction::Quit),
                SigningAction::Run(item) => {
                    if item == SigningItem::Back {
                        return Ok(SigningAction::Run(item));
                    }
                    // Restaurar la terminal para ejecutar la accion (que usa println / inquire)
                    restore_signing_terminal(&mut terminal)?;
                    let action_result = run_signing_action(item, ctx);
                    // Volver a configurar la terminal para el TUI
                    let new_terminal = setup_signing_terminal()?;
                    terminal = new_terminal;
                    cmd::set_console_title(&platform::dir_display_name(&ctx.script_dir));
                    action_result?;
                }
            }
        }
    })();

    let _ = restore_signing_terminal(&mut terminal);
    match result? {
        SigningAction::Quit | SigningAction::Run(SigningItem::Back) => Ok(()),
        _ => unreachable!("manage solo retorna en Quit o Back"),
    }
}

fn require_layout(ctx: &ProjectContext) -> Result<AndroidLayout> {
    match android_layout(ctx) {
        Some(l) => Ok(l),
        None => {
            eprintln!("[ERROR] No aplicable a proyectos Rust.");
            ui::pause();
            anyhow::bail!("proyecto sin Android")
        }
    }
}

fn ensure_module_exists(layout: &AndroidLayout) -> bool {
    if !layout.module_dir.exists() {
        eprintln!("[ERROR] No se encontró el directorio Android del proyecto.");
        eprintln!("  Ruta esperada: {}", layout.module_dir.display());
        if !layout.patch_gradle {
            // Wails v3: los assets Android se generan aparte.
            eprintln!("  Genéralos con: wails3 update build-assets");
        }
        return false;
    }
    // La carpeta de firma se crea si falta: es donde viven el keystore y el
    // `.properties`, y no tiene por qué existir hasta que se configura.
    if !layout.signing_dir.exists() {
        if let Err(e) = fs::create_dir_all(&layout.signing_dir) {
            eprintln!("[ERROR] No se pudo crear: {}", layout.signing_dir.display());
            eprintln!("  {}", e);
            return false;
        }
    }
    true
}

// ── Estado / autodetección ─────────────────────────────────────────────────

fn show_status(ctx: &ProjectContext) -> Result<()> {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║            ESTADO DE LA FIRMA                    ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let Some(info) = detect(ctx) else {
        match ctx.project_type {
            ProjectType::GoWails => {
                println!("[INFO] En Go + Wails v3 la firma la hace el propio Gradle de Wails");
                println!("       con variables de entorno, no con un keystore.properties:");
                println!("         ANDROID_KEYSTORE_FILE, ANDROID_KEYSTORE_PASSWORD,");
                println!("         ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD");
            }
            _ => println!("[INFO] No aplicable a proyectos Rust."),
        }
        ui::pause();
        return Ok(());
    };

    let type_label = match ctx.project_type {
        ProjectType::Tauri => "Tauri",
        ProjectType::Flutter => "Flutter",
        ProjectType::Kotlin => "Kotlin/KMP",
        ProjectType::Rust => "Rust",
        ProjectType::GoWails => "Go + Wails",
    };
    println!("  Proyecto : {}", type_label);
    println!("  Módulo   : {}", info.layout.module_dir.display());
    if info.layout.signing_dir != info.layout.module_dir {
        println!("  Firma en : {}", info.layout.signing_dir.display());
    }

    match &info.props {
        Some(_) => println!("  Properties: {}  [OK]", info.layout.props_name),
        None => println!("  Properties: {}  [FALTA]", info.layout.props_name),
    }

    if let Some(ks) = &info.keystore {
        let size = fs::metadata(ks).map(|m| m.len()).unwrap_or(0);
        println!("  Keystore : {}  ({} KB)  [OK]", ks.display(), size / 1024);
    } else if let Some(missing) = &info.keystore_missing {
        println!("  Keystore : {}  [NO EXISTE]", missing);
    } else {
        println!("  Keystore : (sin configurar)");
    }

    if let Some(alias) = info.alias() {
        println!("  Alias    : {}", alias);
    }

    let gradle = if !info.layout.patch_gradle {
        // Wails v3: la firma llega al Gradle por variables de entorno, y
        // Compass las exporta al compilar la APK/AAB release.
        "firma por variables de entorno (ANDROID_KEYSTORE_*) [OK]".to_string()
    } else {
        match gradle_signing_status(&info.layout.module_dir) {
            GradleSigning::Patched => "parcheado (signingConfigs inyectado) [OK]",
            GradleSigning::Unpatched => "sin parchear — usa la opción 2/3/4 [WARN]",
            GradleSigning::NoFile => "no se encontró build.gradle(.kts) [WARN]",
        }
        .to_string()
    };
    println!("  Gradle   : {}", gradle);

    // Huellas release, si hay keystore y contraseña. Sin `keytool` el estado
    // sigue siendo útil, así que no aborta: sólo avisa.
    let keytool = find_keytool().ok();
    println!();
    println!("── Huellas release ────────────────────────────────");
    match (&info.keystore, info.store_password()) {
        (Some(ks), Some(pass)) => match &keytool {
            Some(kt) => match keystore_fingerprints(kt, ks, pass) {
                Ok(fp) => print_fingerprints(&fp),
                Err(e) => println!("  [WARN] {}", e),
            },
            None => println!("  [WARN] Sin 'keytool' (JDK) para leer las huellas."),
        },
        (Some(_), None) => println!("  [INFO] Falta storePassword en el .properties."),
        _ => println!("  [INFO] Sin keystore release configurado."),
    }

    // Huellas debug (suelen usarse para Firebase en desarrollo).
    println!();
    println!("── Huellas debug ──────────────────────────────────");
    match debug_keystore() {
        Some((path, alias, pass)) => {
            println!("  Keystore: {}", path.display());
            println!("  Alias   : {}", alias);
            match &keytool {
                Some(kt) => match keystore_fingerprints(kt, &path, pass) {
                    Ok(fp) => print_fingerprints(&fp),
                    Err(e) => println!("  [WARN] {}", e),
                },
                None => println!("  [WARN] Sin 'keytool' (JDK) para leer las huellas."),
            }
        }
        None => println!("  [INFO] No hay debug.keystore en ~/.android (compila una vez en debug)."),
    }

    if info.keystore.is_some() || info.keystore_missing.is_some() {
        println!();
        println!("[TIP] Opción 6: genera los secretos para GitHub Actions.");
    }
    ui::pause();
    Ok(())
}

fn print_fingerprints(fp: &Fingerprints) {
    if let Some(v) = &fp.sha1 {
        println!("  SHA-1   : {}", v);
    }
    if let Some(v) = &fp.sha256 {
        println!("  SHA-256 : {}", v);
    }
    if let Some(v) = &fp.md5 {
        println!("  MD5     : {}", v);
    }
    if fp.is_empty() {
        println!("  [WARN] keytool no devolvió ninguna huella reconocible.");
    }
}

/// Muestra las huellas debug y release (equivalente a la antigua tarjeta SHA1).
fn show_fingerprints(ctx: &ProjectContext) -> Result<()> {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║         HUELLAS DIGITALES ANDROID                ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let keytool = find_keytool()?;

    println!("── DEBUG ──────────────────────────────────────────");
    match debug_keystore() {
        Some((path, alias, pass)) => {
            println!("  Keystore: {}", path.display());
            println!("  Alias   : {}", alias);
            match keystore_fingerprints(&keytool, &path, pass) {
                Ok(fp) => print_fingerprints(&fp),
                Err(e) => println!("  [WARN] {}", e),
            }
        }
        None => {
            let dir = platform::home_dir()
                .map(|h| h.join(".android"))
                .unwrap_or_default();
            println!("  [WARN] No se encontró debug.keystore en {}", dir.display());
            println!("  Compila una vez en debug o créalo con keytool.");
        }
    }
    println!();

    println!("── RELEASE ────────────────────────────────────────");
    match detect(ctx) {
        Some(info) => match (&info.keystore, info.store_password()) {
            (Some(ks), Some(pass)) => {
                println!("  Keystore: {}", ks.display());
                if let Some(a) = info.alias() {
                    println!("  Alias   : {}", a);
                }
                match keystore_fingerprints(&keytool, ks, pass) {
                    Ok(fp) => print_fingerprints(&fp),
                    Err(e) => println!("  [WARN] {}", e),
                }
            }
            _ => {
                println!(
                    "  [INFO] Sin firma release configurada (falta {}).",
                    info.layout.props_name
                );
                println!("  Usa la opción 2 o 3 para crearla.");
            }
        },
        None => println!("  [INFO] No aplicable a proyectos Rust."),
    }

    println!();
    println!("[INFO] Pega estas huellas en Firebase / Google Cloud Console según lo necesites.");
    ui::pause();
    Ok(())
}

// ── Crear / importar / manual ──────────────────────────────────────────────

fn create_keystore(ctx: &ProjectContext) -> Result<()> {
    let layout = require_layout(ctx)?;
    if !ensure_module_exists(&layout) {
        ui::pause();
        return Ok(());
    }
    println!();
    ui::info_banner("Crear nueva llave (Keystore)");

    let raw_name = ui::ask_text("Nombre del keystore (sin extensión)", Some("release"))?;
    let name = if raw_name.ends_with(".keystore") {
        raw_name
    } else {
        format!("{}.keystore", raw_name)
    };

    let alias = ui::ask_text("Alias de la llave", Some("upload"))?;
    let pass = ui::ask_password("Contraseña para el Keystore (mín 6 caracteres)")?;

    let dest_keystore = layout.signing_dir.join(&name);
    let keytool = find_keytool()?;
    println!("  keytool encontrado en: {}", keytool);
    println!();

    // Limpieza de keystores previos: el proyecto sólo debe tener el suyo.
    println!("  Limpiando keystores existentes...");
    if let Ok(entries) = fs::read_dir(&layout.signing_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if ext == "keystore" || ext == "jks" {
                        let _ = fs::remove_file(&p);
                        println!("    Eliminado: {}", p.file_name().unwrap().to_string_lossy());
                    }
                }
            }
        }
    }

    println!("\n  Generando keystore (esto puede tardar unos segundos)...");
    // `CN=Tauri App` se mantiene por compatibilidad histórica; no afecta a la
    // firma, sólo a los metadatos del certificado.
    let dname = "CN=Tauri App, OU=Development, O=Developer, L=City, ST=State, C=US";

    let status = Command::new(&keytool)
        .args([
            "-genkey", "-v",
            "-keystore", dest_keystore.to_str().unwrap(),
            "-alias", &alias,
            "-keyalg", "RSA",
            "-keysize", "2048",
            "-validity", "10000",
            "-storepass", &pass,
            "-keypass", &pass,
            "-dname", dname,
        ])
        .status()
        .context("Error al ejecutar keytool")?;

    if !status.success() {
        println!("  Falló con alias '{}'. Probando con alias temporal...", alias);
        let temp_alias = format!("temp_{}", alias);
        let _ = Command::new(&keytool)
            .args(["-delete", "-alias", &temp_alias, "-keystore", dest_keystore.to_str().unwrap(), "-storepass", &pass])
            .output();

        let status2 = Command::new(&keytool)
            .args([
                "-genkey", "-v",
                "-keystore", dest_keystore.to_str().unwrap(),
                "-alias", &temp_alias,
                "-keyalg", "RSA",
                "-keysize", "2048",
                "-validity", "10000",
                "-storepass", &pass,
                "-keypass", &pass,
                "-dname", dname,
            ])
            .status()?;

        if !status2.success() {
            eprintln!("[ERROR] Falló la creación del keystore.");
            eprintln!("Sugerencia: prueba con otro nombre de archivo o alias diferente.");
            ui::pause();
            return Ok(());
        }

        println!("  Renombrando alias '{}' a '{}'...", temp_alias, alias);
        let _ = Command::new(&keytool)
            .args([
                "-changealias",
                "-alias", &temp_alias,
                "-destalias", &alias,
                "-keystore", dest_keystore.to_str().unwrap(),
                "-storepass", &pass,
                "-keypass", &pass,
            ])
            .output();
    }

    println!("  [OK] Keystore generado: {}", name);
    println!();

    write_properties(&layout, &name, &pass, &pass, &alias)?;
    patch_build_gradle(&layout)?;

    ui::success_banner("Firmas configuradas exitosamente");
    ui::pause();
    Ok(())
}

fn import_keystore(ctx: &ProjectContext) -> Result<()> {
    let layout = require_layout(ctx)?;
    if !ensure_module_exists(&layout) {
        ui::pause();
        return Ok(());
    }
    println!();
    ui::info_banner("Usando llave existente");

    let path_str = ui::ask_text("Ruta a tu archivo .keystore o .jks", None)?;
    let mut keystore_file = PathBuf::from(&path_str);

    if !keystore_file.exists() {
        eprintln!("[ERROR] No se encontró el archivo: {}", path_str);
        ui::pause();
        return Ok(());
    }

    if keystore_file.is_dir() {
        println!("  Buscando archivo .keystore o .jks en la carpeta...");
        let mut found = false;
        if let Ok(entries) = fs::read_dir(&keystore_file) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "keystore" || ext == "jks" {
                        keystore_file = p;
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found {
            eprintln!("[ERROR] No se encontró ningún .keystore ni .jks en la carpeta.");
            ui::pause();
            return Ok(());
        }
        println!("  Llave encontrada: {}", keystore_file.file_name().unwrap().to_string_lossy());
    }

    let alias = ui::ask_text("Alias de la llave (Key Alias)", None)?;
    let store_pass = ui::ask_password("Contraseña del Keystore (Store Password)")?;
    let key_pass_input = ui::ask_password("Contraseña de la Llave (Key Password)")?;
    let key_pass = if key_pass_input.is_empty() { store_pass.clone() } else { key_pass_input };

    let k_name = keystore_file.file_name().unwrap().to_str().unwrap().to_string();
    let dest = layout.signing_dir.join(&k_name);
    fs::copy(&keystore_file, &dest).context("Error al copiar el archivo")?;
    println!("  [OK] Archivo copiado correctamente.");

    write_properties(&layout, &k_name, &store_pass, &key_pass, &alias)?;
    patch_build_gradle(&layout)?;

    ui::success_banner("Firmas configuradas exitosamente");
    ui::pause();
    Ok(())
}

fn manual_configure(ctx: &ProjectContext) -> Result<()> {
    let layout = require_layout(ctx)?;
    if !ensure_module_exists(&layout) {
        ui::pause();
        return Ok(());
    }
    println!();
    ui::info_banner("Configuración manual");

    let name = ui::ask_text("Nombre del archivo .keystore (ej: release.keystore)", Some("release.keystore"))?;
    let alias = ui::ask_text("Alias de la llave", Some("upload"))?;
    let store_pass = ui::ask_password("Contraseña del Keystore (Store Password)")?;
    let key_pass_input = ui::ask_password("Contraseña de la Llave (Key Password)")?;
    let key_pass = if key_pass_input.is_empty() { store_pass.clone() } else { key_pass_input };

    write_properties(&layout, &name, &store_pass, &key_pass, &alias)?;
    patch_build_gradle(&layout)?;

    ui::success_banner("Configuración manual completada");
    println!(
        "  Asegúrate de que '{}' exista en {}.",
        name,
        layout.signing_dir.display()
    );
    ui::pause();
    Ok(())
}

fn write_properties(layout: &AndroidLayout, keystore_name: &str, store_pass: &str, key_pass: &str, key_alias: &str) -> Result<()> {
    let props = Props {
        store_file: Some(keystore_name.to_string()),
        store_password: Some(store_pass.to_string()),
        key_alias: Some(key_alias.to_string()),
        key_password: Some(key_pass.to_string()),
    };
    let path = layout.props_path();
    fs::write(&path, props.render())
        .with_context(|| format!("Error al escribir {}", path.display()))?;
    println!("  [OK] {} generado.", layout.props_name);
    Ok(())
}

/// Inyecta el loader, `signingConfigs` y el `signingConfig` de release en el
/// `build.gradle(.kts)` del módulo. Soporta Kotlin Script y Groovy.
fn patch_build_gradle(layout: &AndroidLayout) -> Result<()> {
    // En Go + Wails v3 el Gradle ya lee la firma del entorno
    // (`ANDROID_KEYSTORE_FILE` y compañía): parchearlo duplicaría la
    // configuración y, además, el archivo lo regenera
    // `wails3 update build-assets`.
    if !layout.patch_gradle {
        println!("  [OK] El Gradle de Wails ya lee la firma del entorno; no hay que parchearlo.");
        println!("       Compass exporta ANDROID_KEYSTORE_* al compilar la APK/AAB release.");
        return Ok(());
    }

    let android_app = &layout.module_dir;
    let props_name = layout.props_name;

    let gradle_kts = android_app.join("build.gradle.kts");
    let use_kts = gradle_kts.exists();
    let gradle_path = if use_kts {
        gradle_kts
    } else {
        let groovy = android_app.join("build.gradle");
        if !groovy.exists() {
            eprintln!("[WARNING] No se encontró build.gradle(.kts) en: {}", android_app.display());
            eprintln!("  Puedes agregar la configuración manualmente.");
            return Ok(());
        }
        groovy
    };

    let mut content = fs::read_to_string(&gradle_path)
        .with_context(|| format!("Error al leer {}", gradle_path.display()))?;
    let mut modified = false;

    let module_name = android_app
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("app");

    if use_kts {
        if !content.contains("import java.util.Properties") && !content.contains("import java.util.*") {
            content = format!("import java.util.Properties\n\n{}", content);
            modified = true;
        }

        let properties_ref = format!("{}/{}", module_name, props_name);

        if !content.contains("val keystorePropertiesFile") {
            let loader = format!("\nval keystorePropertiesFile = rootProject.file(\"{}\")\nval keystoreProperties = Properties()\nif (keystorePropertiesFile.exists()) {{\n    keystorePropertiesFile.inputStream().use {{ keystoreProperties.load(it) }}\n}}\n", properties_ref);
            content = content.replace("android {", &format!("{}\nandroid {{", loader));
            modified = true;
        }

        if !content.contains("signingConfigs {") {
            let signing = "\n    signingConfigs {\n        if (keystorePropertiesFile.exists()) {\n            create(\"release\") {\n                keyAlias = keystoreProperties.getProperty(\"keyAlias\") ?: \"\"\n                keyPassword = keystoreProperties.getProperty(\"keyPassword\") ?: \"\"\n                storeFile = file(keystoreProperties.getProperty(\"storeFile\") ?: \"\")\n                storePassword = keystoreProperties.getProperty(\"storePassword\") ?: \"\"\n            }\n        }\n    }\n";
            content = content.replace("    defaultConfig {", &format!("{}\n    defaultConfig {{", signing));
            modified = true;
        }

        if !content.contains("signingConfig = signingConfigs.getByName(\"release\")") {
            let apply = "\n            if (keystorePropertiesFile.exists()) {\n                signingConfig = signingConfigs.getByName(\"release\")\n            }";
            content = content.replace("getByName(\"release\") {", &format!("getByName(\"release\") {{{}", apply));
            modified = true;
        }
    } else {
        let properties_ref = format!("{}/{}", module_name, props_name);

        if !content.contains("def keystorePropertiesFile") {
            let loader = format!("\ndef keystorePropertiesFile = rootProject.file('{}')\ndef keystoreProperties = new Properties()\nif (keystorePropertiesFile.exists()) {{\n    keystoreProperties.load(new FileInputStream(keystorePropertiesFile))\n}}\n", properties_ref);
            content = content.replace("android {", &format!("{}\nandroid {{", loader));
            modified = true;
        }

        if !content.contains("signingConfigs {") {
            let signing = "\n    signingConfigs {\n        if (keystorePropertiesFile.exists()) {\n            release {\n                keyAlias keystoreProperties['keyAlias']\n                keyPassword keystoreProperties['keyPassword']\n                storeFile file(keystoreProperties['storeFile'])\n                storePassword keystoreProperties['storePassword']\n            }\n        }\n    }\n";
            content = content.replace("    defaultConfig {", &format!("{}\n    defaultConfig {{", signing));
            modified = true;
        }

        if !content.contains("signingConfig signingConfigs.release") {
            let apply = "\n            if (keystorePropertiesFile.exists()) {\n                signingConfig = signingConfigs.release\n            }";
            content = content.replace("release {", &format!("release {{{}", apply));
            modified = true;
        }
    }

    if modified {
        fs::write(&gradle_path, content)
            .with_context(|| format!("Error al escribir {}", gradle_path.display()))?;
        println!("  [OK] {} actualizado con configuración de firma.", gradle_path.file_name().unwrap().to_string_lossy());
    } else {
        println!("  {} ya contenía la configuración.", gradle_path.file_name().unwrap().to_string_lossy());
    }

    Ok(())
}

// ── Respaldos ──────────────────────────────────────────────────────────────

fn backup_dir(ctx: &ProjectContext) -> PathBuf {
    ctx.script_dir.join("keystore-backups")
}

fn backup_keystore(ctx: &ProjectContext) -> Result<()> {
    println!("--- RESPALDAR KEYSTORE ---");
    let layout = require_layout(ctx)?;
    if !ensure_module_exists(&layout) {
        ui::pause();
        return Ok(());
    }

    // Keystores de la carpeta de firma + el declarado en las properties (puede
    // vivir fuera).
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&layout.signing_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if p.is_file() && (name.ends_with(".keystore") || name.ends_with(".jks")) {
                files.push(p);
            }
        }
    }
    if let Some(info) = detect(ctx) {
        if let Some(ks) = info.keystore {
            if !files.contains(&ks) {
                files.push(ks);
            }
        }
    }

    if files.is_empty() {
        eprintln!();
        eprintln!("[ERROR] No se encontraron archivos .keystore o .jks.");
        eprintln!("La ruta buscada es: {}", layout.signing_dir.display());
        eprintln!("Primero crea uno con la opción 2.");
        ui::pause();
        return Ok(());
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let dest_dir = backup_dir(ctx).join(&timestamp);
    fs::create_dir_all(&dest_dir)?;

    for src in &files {
        let name = src.file_name().unwrap();
        fs::copy(src, dest_dir.join(name))?;
        println!("  [OK] {} respaldado.", name.to_string_lossy());
    }

    // Las properties van con el keystore: sin ellas el respaldo no sirve.
    for props_name in &["keystore.properties", "key.properties"] {
        let props = layout.signing_dir.join(props_name);
        if props.exists() {
            fs::copy(&props, dest_dir.join(props_name))?;
            println!("  [OK] {} respaldado.", props_name);
        }
    }

    println!();
    println!("[OK] Respaldo completado en:");
    println!("     {}", dest_dir.display());
    println!();
    println!("IMPORTANTE: Guarda esta carpeta en un lugar seguro (USB, nube).");
    ui::pause();
    Ok(())
}

fn restore_keystore(ctx: &ProjectContext) -> Result<()> {
    println!("--- RESTAURAR KEYSTORE ---");
    let layout = require_layout(ctx)?;

    let dir = backup_dir(ctx);
    if !dir.exists() {
        eprintln!("[ERROR] No existe la carpeta de respaldos.");
        eprintln!("Primero crea un respaldo con la opción 7.");
        ui::pause();
        return Ok(());
    }

    let mut backups: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                backups.push(e.path());
            }
        }
    }
    if backups.is_empty() {
        eprintln!("[ERROR] No hay respaldos disponibles.");
        ui::pause();
        return Ok(());
    }
    backups.sort_by_key(|p| p.file_name().unwrap().to_os_string());

    let labels: Vec<String> = backups
        .iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let count = fs::read_dir(p).map(|d| d.count()).unwrap_or(0);
            format!("{} ({} archivo(s))", name, count)
        })
        .collect();

    // Índice en vez de texto: el nombre puede llevar paréntesis.
    let idx = match Select::new("Selecciona respaldo a restaurar:", labels)
        .with_vim_mode(true)
        .raw_prompt()
    {
        Ok(c) => c.index,
        Err(_) => return Ok(()),
    };
    let selected_path = &backups[idx];

    let mut count = 0;
    for entry in fs::read_dir(selected_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let name = entry.file_name();
            fs::copy(&path, layout.signing_dir.join(&name))?;
            println!("  [OK] {} restaurado.", name.to_string_lossy());
            count += 1;
        }
    }

    println!();
    println!("[OK] Restauración completada ({} archivo(s)).", count);
    println!("Ya puedes compilar tu Android en modo release con firma.");
    ui::pause();
    Ok(())
}

fn list_backups(ctx: &ProjectContext) -> Result<()> {
    println!("--- RESPALDOS DISPONIBLES ---");

    let dir = backup_dir(ctx);
    if !dir.exists() {
        println!("No existe la carpeta de respaldos.");
        println!("Usa la opción 7 para crear tu primer respaldo.");
        ui::pause();
        return Ok(());
    }

    let mut backups: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                backups.push(e.path());
            }
        }
    }

    if backups.is_empty() {
        println!("No hay respaldos aún.");
        println!("Usa la opción 7 para crear tu primer respaldo.");
    } else {
        backups.sort_by_key(|p| p.file_name().unwrap().to_os_string());
        println!();
        println!("Total de respaldos: {}", backups.len());
        println!();
        for entry in &backups {
            let name = entry
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let mut files = 0usize;
            let mut total_size = 0u64;
            if let Ok(rd) = fs::read_dir(entry) {
                for f in rd.flatten() {
                    files += 1;
                    total_size += f.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
            println!("  {}  |  {} archivo(s)  |  {:.2} KB", name, files, total_size as f64 / 1024.0);
        }
    }
    ui::pause();
    Ok(())
}

// ── Secretos para GitHub Actions ───────────────────────────────────────────

fn show_secrets(ctx: &ProjectContext) -> Result<()> {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║        SECRETOS PARA GITHUB ACTIONS              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let Some(info) = detect(ctx) else {
        println!("[INFO] No aplicable a proyectos Rust.");
        ui::pause();
        return Ok(());
    };

    let keystore_b64 = info
        .keystore
        .as_ref()
        .and_then(|p| fs::read(p).ok())
        .map(|b| base64_encode(&b));

    if keystore_b64.is_none() {
        println!("[WARN] No se encontró el keystore; el base64 irá vacío.");
        println!("       Configura la firma primero (opción 2 o 3).");
        println!();
    }

    // 1. Nombres detectados en los workflows del proyecto.
    let names = workflow_secret_names(&ctx.script_dir);
    let workflows = collect_files(&ctx.script_dir.join(".github").join("workflows"));

    if !names.is_empty() {
        println!("── Detectados en tus workflows ({} archivo(s)) ────", workflows.len());
        println!();
        let mut rows: Vec<SecretGuess> = Vec::new();
        for n in &names {
            let g = guess_secret(n, &info, keystore_b64.as_deref());
            print_secret(&g);
            rows.push(g);
        }
        println!();
    } else {
        println!("[INFO] No se encontraron secretos (`secrets.NOMBRE`) en .github/workflows.");
        println!("       Abajo van los valores canónicos que suelen usar las apps Android.");
        println!();
    }

    // 2. Valores canónicos, siempre (aunque no haya workflows).
    println!("── Valores canónicos ──────────────────────────────");
    println!();
    let canonical = [
        SecretGuess {
            name: "ANDROID_KEYSTORE_BASE64".to_string(),
            value: keystore_b64.clone().unwrap_or_default(),
            source: "keystore (base64)",
        },
        SecretGuess {
            name: "ANDROID_KEYSTORE_PASSWORD".to_string(),
            value: info.store_password().unwrap_or_default().to_string(),
            source: "storePassword",
        },
        SecretGuess {
            name: "ANDROID_KEY_ALIAS".to_string(),
            value: info.alias().unwrap_or_default().to_string(),
            source: "keyAlias",
        },
        SecretGuess {
            name: "ANDROID_KEY_PASSWORD".to_string(),
            value: info.key_password().unwrap_or_default(),
            source: "keyPassword",
        },
    ];
    let mut report = String::new();
    report.push_str("# Secretos para GitHub Actions (generado por compass)\n");
    report.push_str("# NO COMMITEAR: contiene la contraseña del keystore.\n\n");
    for g in &canonical {
        print_secret(g);
        report.push_str(&format!("{}\n{}\n\n", g.name, g.value));
    }

    if !names.is_empty() {
        report.push_str("# Detectados en tus workflows\n");
        for n in &names {
            let g = guess_secret(n, &info, keystore_b64.as_deref());
            report.push_str(&format!("{} # {}\n", g.name, g.source));
        }
    }

    println!();
    println!("[WARN] Estos valores son secretos: la contraseña aparece en claro para");
    println!("       que la copies a GitHub ▸ Settings ▸ Secrets and variables ▸ Actions.");
    println!();

    let save = ui::ask_text("¿Guardar el reporte en un archivo? (s/N)", Some("N"))?;
    if is_yes(&save) {
        let path = ctx.script_dir.join("github-secrets.txt");
        fs::write(&path, &report)
            .with_context(|| format!("No se pudo escribir {}", path.display()))?;
        println!("  [OK] Reporte guardado en: {}", path.display());

        // Girar el .gitignore del proyecto para no filtrar el archivo.
        let gi = ctx.script_dir.join(".gitignore");
        if gi.exists() {
            let content = fs::read_to_string(&gi).unwrap_or_default();
            if !content.contains("github-secrets.txt") {
                let mut new = content;
                if !new.ends_with('\n') {
                    new.push('\n');
                }
                new.push_str("\n# Secretos generados por compass\ngithub-secrets.txt\n");
                let _ = fs::write(&gi, new);
                println!("  [OK] Añadido a .gitignore (no se subirá por accidente).");
            }
        } else {
            println!("  [WARN] No hay .gitignore: asegúrate de no commitear este archivo.");
        }
    }

    ui::pause();
    Ok(())
}

fn print_secret(g: &SecretGuess) {
    let shown = if g.value.is_empty() {
        "(sin datos)".to_string()
    } else if g.value.len() > 72 {
        format!("{}… ({} bytes)", &g.value[..72], g.value.len())
    } else {
        g.value.clone()
    };
    println!("  {:<28} = {:<80} ({})", g.name, shown, g.source);
}

fn is_yes(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "s" | "si" | "sí" | "y" | "yes")
}

// ── Verificación de APK ────────────────────────────────────────────────────

/// Localiza `apksigner` en la build-tools más reciente del SDK.
fn find_apksigner(ctx: &ProjectContext) -> Option<PathBuf> {
    let home = ctx.android_home.as_ref()?;
    let bin = platform::exe_name("apksigner");
    let root = home.join("build-tools");
    let mut versions: Vec<(Vec<u64>, PathBuf)> = Vec::new();
    for entry in fs::read_dir(&root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let key: Vec<u64> = name.split('.').filter_map(|p| p.parse().ok()).collect();
        if key.is_empty() {
            continue;
        }
        let candidate = entry.path().join(&bin);
        if candidate.exists() {
            versions.push((key, candidate));
        }
    }
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().next().map(|(_, p)| p)
}

/// APK más reciente entre las carpetas de salida típicas.
fn find_newest_apk(ctx: &ProjectContext) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![
        // Flutter
        ctx.script_dir.join("build").join("app").join("outputs"),
    ];
    if let Some(l) = android_layout(ctx) {
        // Tauri y KMP
        roots.push(l.module_dir.join("build").join("outputs").join("apk"));
    }

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for root in roots {
        for apk in collect_apks(&root) {
            let modified = fs::metadata(&apk)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                newest = Some((modified, apk));
            }
        }
    }
    newest.map(|(_, p)| p)
}

fn collect_apks(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(collect_apks(&p));
        } else if p.extension().and_then(|x| x.to_str()) == Some("apk") {
            out.push(p);
        }
    }
    out
}

fn verify_apk(ctx: &ProjectContext) -> Result<()> {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║           VERIFICAR FIRMA DE UN APK              ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let apk = match find_newest_apk(ctx) {
        Some(p) => {
            println!("[INFO] APK detectado: {}", p.display());
            p
        }
        None => {
            let input = ui::ask_text("Ruta del APK a verificar", None)?;
            if input.is_empty() {
                println!("[INFO] Cancelado.");
                ui::pause();
                return Ok(());
            }
            PathBuf::from(input)
        }
    };

    if !apk.exists() {
        eprintln!("[ERROR] No existe: {}", apk.display());
        ui::pause();
        return Ok(());
    }

    let Some(apksigner) = find_apksigner(ctx) else {
        eprintln!("[ERROR] No se encontró 'apksigner' en $ANDROID_HOME/build-tools.");
        eprintln!("Instala build-tools con: sdkmanager \"build-tools;34.0.0\"");
        ui::pause();
        return Ok(());
    };

    let out = Command::new(&apksigner)
        .args(["verify", "--print-certs"])
        .arg(&apk)
        .output()
        .context("Error al ejecutar apksigner")?;

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    println!();
    let certs = parse_apksigner_certs(&text);
    if certs.is_empty() {
        println!("[ERROR] apksigner no devolvió certificados.");
        println!("        ¿Está el APK firmado? Salida:");
        for line in text.lines().take(10) {
            println!("        {}", line);
        }
        ui::pause();
        return Ok(());
    }

    println!("── Firmante del APK ───────────────────────────────");
    let mut apk_sha1 = None;
    let mut apk_sha256 = None;
    for (label, value) in &certs {
        println!("  {:<9}: {}", label, value);
        match label.as_str() {
            "SHA-1" => apk_sha1 = Some(normalize_fingerprint(value)),
            "SHA-256" => apk_sha256 = Some(normalize_fingerprint(value)),
            _ => {}
        }
    }

    // Comparar contra el keystore release configurado.
    println!();
    println!("── Comparación con el keystore configurado ────────");
    match detect(ctx) {
        Some(info) if info.keystore.is_some() && info.store_password().is_some() => {
            let keytool = match find_keytool() {
                Ok(k) => k,
                Err(e) => {
                    println!("  [WARN] {}", e);
                    ui::pause();
                    return Ok(());
                }
            };
            let ks = info.keystore.as_ref().unwrap();
            match keystore_fingerprints(&keytool, ks, info.store_password().unwrap()) {
                Ok(fp) => {
                    let want_sha256 = fp.sha256.as_deref().map(normalize_fingerprint);
                    let want_sha1 = fp.sha1.as_deref().map(normalize_fingerprint);
                    let sha256_ok = want_sha256.as_deref() == apk_sha256.as_deref();
                    let sha1_ok = want_sha1.as_deref() == apk_sha1.as_deref();
                    if sha256_ok || sha1_ok {
                        ui::success_banner("El APK coincide con el keystore configurado");
                    } else {
                        println!("  [WARN] El APK NO coincide con el keystore release configurado.");
                        println!("         El APK se firmó con otras llaves (¿debug? ¿keystore viejo?).");
                    }
                }
                Err(e) => println!("  [WARN] No se pudieron leer las huellas del keystore: {}", e),
            }
        }
        _ => {
            println!("  [INFO] No hay keystore release configurado para comparar.");
        }
    }

    ui::pause();
    Ok(())
}

/// Extrae `(label, valor)` de la salida de `apksigner verify --print-certs`.
pub fn parse_apksigner_certs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(v) = t.split_once("certificate SHA-256 digest:") {
            out.push(("SHA-256".to_string(), v.1.trim().to_string()));
        } else if let Some(v) = t.split_once("certificate SHA-1 digest:") {
            out.push(("SHA-1".to_string(), v.1.trim().to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_roundtrip() {
        let p = Props {
            store_file: Some("release.keystore".into()),
            store_password: Some("s3cret".into()),
            key_alias: Some("upload".into()),
            key_password: Some("keypass".into()),
        };
        assert_eq!(Props::parse(&p.render()), p);
    }

    #[test]
    fn props_accept_flutter_and_tauri_key_aliases() {
        let flutter = Props::parse("storePassword=a\nkeyPassword=b\nkeyAlias=c\nstoreFile=d.jks");
        assert_eq!(flutter.store_password.as_deref(), Some("a"));
        assert_eq!(flutter.key_password.as_deref(), Some("b"));
        assert_eq!(flutter.key_alias.as_deref(), Some("c"));
        assert_eq!(flutter.store_file.as_deref(), Some("d.jks"));

        let tauri = Props::parse("storepass=a\nkeypass=b\nalias=c\nstoreFile=d");
        assert_eq!(tauri.store_password.as_deref(), Some("a"));
        assert_eq!(tauri.key_password.as_deref(), Some("b"));
        assert_eq!(tauri.key_alias.as_deref(), Some("c"));
    }

    #[test]
    fn props_strip_quotes() {
        let p = Props::parse("storeFile=\"a b.keystore\"\nkeyAlias = \"x\"");
        assert_eq!(p.store_file.as_deref(), Some("a b.keystore"));
        assert_eq!(p.key_alias.as_deref(), Some("x"));
    }

    #[test]
    fn resolve_keystore_handles_absolute_and_relative() {
        let dir = Path::new("/tmp/mod");
        assert_eq!(
            resolve_keystore(dir, "release.keystore"),
            PathBuf::from("/tmp/mod/release.keystore")
        );
        assert_eq!(
            resolve_keystore(dir, "/abs/other.jks"),
            PathBuf::from("/abs/other.jks")
        );
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    fn sample_info() -> SigningInfo {
        SigningInfo {
            layout: AndroidLayout {
                module_dir: PathBuf::from("/tmp/app"),
                props_name: "keystore.properties",
                signing_dir: PathBuf::from("/tmp/app"),
                patch_gradle: true,
            },
            props: Some(Props {
                store_file: Some("release.keystore".into()),
                store_password: Some("storepw".into()),
                key_alias: Some("upload".into()),
                key_password: Some("keypw".into()),
            }),
            keystore: Some(PathBuf::from("/tmp/app/release.keystore")),
            keystore_missing: None,
        }
    }

    /// El caso que rompe la heurística ingenua: `KEYSTORE_PASSWORD` contiene
    /// `KEYSTORE` pero debe resolver a la contraseña, no al base64 del archivo.
    #[test]
    fn secret_guessing_prefers_password_over_keystore() {
        let info = sample_info();
        let b64 = Some("BASE64DATA");

        assert_eq!(
            guess_secret("ANDROID_KEYSTORE_BASE64", &info, b64).value,
            "BASE64DATA"
        );
        assert_eq!(
            guess_secret("ANDROID_KEYSTORE_PASSWORD", &info, b64).value,
            "storepw"
        );
        assert_eq!(guess_secret("ANDROID_KEY_ALIAS", &info, b64).value, "upload");
        assert_eq!(guess_secret("ANDROID_KEY_PASSWORD", &info, b64).value, "keypw");
        assert_eq!(guess_secret("STORE_FILE", &info, b64).source, "keystore (base64)");
        assert_eq!(guess_secret("OTRA_COSA", &info, b64).source, "sin datos");
    }

    #[test]
    fn workflow_secret_names_scans_yaml_recursively() {
        let root = std::env::temp_dir().join(format!("cr-secrets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let wf = root.join(".github").join("workflows");
        fs::create_dir_all(&wf).unwrap();
        fs::write(
            wf.join("release.yml"),
            "env:\n  K: ${{ secrets.ANDROID_KEYSTORE_BASE64 }}\n  P: ${{ secrets.MY_PASS }}\n",
        )
        .unwrap();
        fs::write(wf.join("notes.txt"), "secrets.IGNORED\n").unwrap();

        let names = workflow_secret_names(&root);
        assert_eq!(names, vec!["ANDROID_KEYSTORE_BASE64", "MY_PASS"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gradle_status_detects_the_patch() {
        let dir = std::env::temp_dir().join(format!("cr-gradle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        assert_eq!(gradle_signing_status(&dir), GradleSigning::NoFile);

        fs::write(dir.join("build.gradle.kts"), "android {\n}\n").unwrap();
        assert_eq!(gradle_signing_status(&dir), GradleSigning::Unpatched);

        fs::write(
            dir.join("build.gradle.kts"),
            "val keystorePropertiesFile = rootProject.file(\"app/keystore.properties\")\nandroid {\n signingConfigs {\n }\n}\n",
        )
        .unwrap();
        assert_eq!(gradle_signing_status(&dir), GradleSigning::Patched);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apksigner_cert_parsing() {
        let text = "Verifies\nSigner #1 certificate DN: CN=x\n\
                    Signer #1 certificate SHA-256 digest: AABB:CCDD\n\
                    Signer #1 certificate SHA-1 digest: 11:22\n";
        let certs = parse_apksigner_certs(text);
        assert_eq!(
            certs,
            vec![
                ("SHA-256".to_string(), "AABB:CCDD".to_string()),
                ("SHA-1".to_string(), "11:22".to_string()),
            ]
        );
    }

    #[test]
    fn fingerprint_normalization_ignores_colons_and_case() {
        assert_eq!(
            normalize_fingerprint("AB:CD:ef"),
            normalize_fingerprint("abcdef")
        );
    }
}
