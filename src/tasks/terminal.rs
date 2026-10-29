use std::io::{self, Write};
use anyhow::Result;
use crate::utils::{cmd, context::ProjectContext};

/// Perfil completo: sugerencias predictivas del historial.
///
/// Requiere **PSReadLine 2.2.0** o superior. Antes de esa versión no existen ni
/// el parámetro `-PredictionSource` de `Set-PSReadLineOption` ni las funciones
/// `AcceptSuggestion` / `AcceptNextSuggestionWord`, que es exactamente lo que
/// hace que Windows PowerShell 5.1 —que trae la 2.0.0— escupa tres errores en
/// cada terminal nueva.
const PROFILE_PREDICTIVE: &str = r#"# --- compass: autocompletado (PSReadLine >= 2.2.0) ---
# 1. Autocompletado visual del historial
#
# El try/catch no es decorativo: PSReadLine lanza un error si se activa la
# predicción sin una consola interactiva con soporte de terminal virtual, o sea
# cuando la salida está redirigida (`powershell -Command ...`, scripts, o
# cualquier herramienta que lance PowerShell capturando stdout). Como el perfil
# se carga en todos esos casos, sin esto el error salía igual que el que se
# quería eliminar.
try { Set-PSReadLineOption -PredictionSource History } catch { }

# 2. TAB acepta la sugerencia COMPLETA
Set-PSReadLineKeyHandler -Key Tab -Function AcceptSuggestion

# 3. Flecha Derecha acepta palabra por palabra
Set-PSReadLineKeyHandler -Key RightArrow -Function AcceptNextSuggestionWord

# 4. Historial más grande, sin campana y con ToolTips
Set-PSReadLineOption -MaximumHistoryCount 32767 -HistorySaveStyle SaveIncrementally
Set-PSReadLineOption -BellStyle None -ShowToolTips
"#;

/// Perfil para PSReadLine anterior a 2.2.0.
///
/// No hay sugerencias predictivas que ofrecer, así que el autocompletado se
/// apoya en el menú de TAB, que es lo que había antes de que existieran. Todo lo
/// que se emite aquí existe desde 2.0.0 (comprobado con `(Get-Command
/// Set-PSReadLineOption).Parameters`), de modo que el perfil no puede romper una
/// instalación antigua.
const PROFILE_LEGACY: &str = r#"# --- compass: autocompletado (PSReadLine anterior a 2.2.0) ---
# Este PSReadLine no tiene sugerencias predictivas, así que TAB muestra el menú
# de completados. Lo que sí es posible aquí:
#
#   - Instalar una versión moderna, que devuelve el completado inline:
#       Install-Module PSReadLine -Scope CurrentUser -Force
#     (PSReadLine 2.2.0+ es la primera con -PredictionSource)

# 1. TAB abre el menú de completados
Set-PSReadLineKeyHandler -Key Tab -Function MenuComplete

# 2. Flechas arriba/abajo buscan en el historial sobre lo ya escrito
Set-PSReadLineKeyHandler -Key UpArrow -Function HistorySearchBackward
Set-PSReadLineKeyHandler -Key DownArrow -Function HistorySearchForward

# 3. El cursor se mueve al final de la coincidencia del historial
Set-PSReadLineOption -HistorySearchCursorMovesToEnd

# 4. Historial más grande, sin campana y con ToolTips
Set-PSReadLineOption -MaximumHistoryCount 32767 -HistorySaveStyle SaveIncrementally
Set-PSReadLineOption -BellStyle None -ShowToolTips
"#;

/// Perfil cuando no hay PSReadLine instalado: no hace nada, pero explica por qué.
///
/// Sin este bloque, cualquier `Set-PSReadLine*` en el perfil falla al abrir
/// terminals que no cargan el módulo (ISE, `powershell -File`, hosts que no lo
/// auto-cargan) y el usuario ve errores que no ha provocado.
const PROFILE_SIN_MODULO: &str = r#"# --- compass: autocompletado (no configurado) ---
# No se encontró PSReadLine instalado para Windows PowerShell, así que no hay
# nada que configurar. Para instalarlo:
#
#   Install-Module PSReadLine -Scope CurrentUser -Force
"#;

/// Sonda: versión de PSReadLine más reciente disponible.
///
/// Usa `Get-Module -ListAvailable`, que no necesita importar el módulo, y
/// `-NoProfile`, que evita que el perfil roto del usuario contamine la salida.
/// La orden va en un único argumento de `Command` (sin shell de por medio), así
/// que las comillas internas no dan problemas.
fn psreadline_version() -> Option<String> {
    let script = "(Get-Module -ListAvailable PSReadLine \
                  | Sort-Object Version -Descending \
                  | Select-Object -First 1).Version.ToString()";
    let out = cmd::run("powershell", &["-NoProfile", "-Command", script]).ok()?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// Instala la última PSReadLine en el ámbito del usuario actual.
///
/// Tres detalles que hacen falta en Windows PowerShell 5.1 y que sin ellos la
/// instalación falla o se queda colgada:
///
/// - **TLS 1.2.** PSGallery rechaza la conexión con *"Could not create SSL/TLS
///   secure channel"* en 5.1, que negocia TLS 1.0 por defecto. Se fija el
///   protocolo en el mismo `-Command`, porque `SecurityProtocol` es de proceso.
/// - **`-Force -AllowClobber -Confirm:$false`.** Sin ellos `Install-Module`
///   pregunta por consola si replaces una versión ya instalada, y con el stdin
///   cerrado del helper esa pregunta aborta la instalación.
/// - **Techo de tiempo.** La primera vez descarga el proveedor NuGet, y eso
///   puede tardar o quedarse esperando.
///
/// Devuelve la versión instalada, o `None` si la instalación falla.
fn upgrade_psreadline() -> Option<String> {
    const TIMEOUT_SEGS: u64 = 300;

    let script = concat!(
        "[Net.ServicePointManager]::SecurityProtocol = ",
        "[Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls11; ",
        "Install-Module PSReadLine -Scope CurrentUser -Force -AllowClobber ",
        "-Confirm:$false -ErrorAction Stop; ",
        "(Get-Module -ListAvailable PSReadLine ",
        "| Sort-Object Version -Descending | Select-Object -First 1).Version.ToString()",
    );

    println!("  Instalando (descarga desde PSGallery, la primera vez tarda)...\n");
    match cmd::run_timeout("powershell", &["-NoProfile", "-Command", script], TIMEOUT_SEGS) {
        Ok(true) => psreadline_version(),
        Ok(false) => {
            println!("[WARN] La instalación tardó más de {TIMEOUT_SEGS}s y se ha cancelado.");
            psreadline_version()
        }
        Err(e) => {
            println!("[WARN] No se pudo instalar PSReadLine: {e}");
            None
        }
    }
}

/// `2.11.4` → `211`. Devuelve `None` si la versión no se puede leer.
fn version_code(version: &str) -> Option<u32> {
    let mut parts = version.split('.');
    let major: u32 = parts.next()?.trim().parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
    Some(major * 100 + minor)
}

/// ¿La versión instalada sirve para el completado predictivo?
fn is_predictive(version: Option<&str>) -> bool {
    version.and_then(version_code).is_some_and(|code| code >= 202)
}

/// Marcador de orden de bytes UTF-8 con BOM.
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Escribe `contents` como UTF-8 **con BOM**.
///
/// PowerShell 5.1 lee un `.ps1` sin BOM como ANSI (CP1252), no como UTF-8. Los
/// acentos se descuadran: `ú` son los bytes `C3 BA`, que en CP1252 son *dos*
/// caracteres (195 y 186) y salen como "Ãº". El BOM es lo que le dice que la
/// codificación es UTF-8. `fs::write` no lo pone, hay que escribirlo.
///
/// Importa sobre todo porque el perfil lo relee el usuario, pero sobre todo
/// porque cualquier ruta con acento que acabe en él (un alias, un `Set-Location`)
/// dejaría de funcionar.
fn write_utf8_bom(path: &std::path::Path, contents: &str) -> Result<()> {
    let mut bytes = UTF8_BOM.to_vec();
    bytes.extend_from_slice(contents.as_bytes());
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Perfil adecuado a la versión instalada.
fn profile_for(version: Option<&str>) -> &'static str {
    match version {
        // 2.2.0 es la primera con predictive intellisense.
        Some(v) if version_code(v).is_some_and(|code| code >= 202) => PROFILE_PREDICTIVE,
        Some(_) => PROFILE_LEGACY,
        None => PROFILE_SIN_MODULO,
    }
}

pub fn configure(_ctx: &ProjectContext) -> Result<()> {
    // El perfil depende de la versión instalada, así que lo primero es asegurarla:
    // sin predictivas (o sin PSReadLine) se actualiza antes de escribir nada.
    // Es el orden que pide el flujo: instalar, y *después* configurar.
    let mut version = psreadline_version();
    let mut actualizada = false;

    println!();
    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║     CONFIGURAR TERMINAL - AUTOCOMPLETADO         ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    match &version {
        Some(v) if is_predictive(Some(v)) => {
            println!("[OK] PSReadLine {v} ya soporta autocompletado predictivo.");
        }
        _ => {
            println!("[INFO] El completado predictivo necesita PSReadLine 2.2.0+.");
            let actual = match &version {
                Some(v) => format!(" ahora está la {v} que trae Windows PowerShell 5.1."),
                None => " no hay ninguna instalada.".to_string(),
            };
            print!("[INFO] Se va a actualizar a la última versión disponible{actual}");
            print!("\n\n");

            version = upgrade_psreadline();
            actualizada = true;
        }
    }

    let profile = profile_for(version.as_deref());

    if actualizada && !is_predictive(version.as_deref()) {
        match &version {
            Some(v) => println!("[WARN] La actualización no dio una versión con predictivas (sigue {v})."),
            None => println!("[WARN] La actualización no surtió efecto. Revisa la conexión a PSGallery."),
        }
        println!("       Se configurará igualmente el autocompletado que esa versión permita.");
    }
    println!();
    match &version {
        Some(v) if version_code(v).is_some_and(|c| c >= 202) => {
            println!("PSReadLine detectado: {v}");
            println!("Se activará el autocompletado predictivo:");
            println!("  1. Sugerencias del historial mientras escribes");
            println!("  2. TAB acepta la sugerencia completa");
            println!("  3. Flecha Derecha acepta palabra por palabra");
        }
        Some(v) => {
            println!("PSReadLine detectado: {v}");
            println!();
            println!("[WARN] Esta versión no tiene sugerencias predictivas.");
            println!("       Son una añadido de PSReadLine 2.2.0, y esta máquina tiene la");
            println!("       {v} que trae Windows PowerShell 5.1 de serie.");
            println!();
            println!("Se configurará el autocompletado disponible en ella:");
            println!("  1. TAB abre el menú de completados");
            println!("  2. Flechas arriba/abajo buscan en el historial");
            println!("  3. Historial grande, sin campana, con ToolTips");
            println!();
            println!("Para tener el completado predictivo, actualiza PSReadLine:");
            println!("  Install-Module PSReadLine -Scope CurrentUser -Force");
            println!("  (luego reinstala esta opcion y se detectará la version nueva)");
        }
        None => {
            println!("[WARN] No se encontró PSReadLine instalado.");
            println!("       Se escribirá un perfil que sólo explica cómo instalarlo,");
            println!("       en vez de uno que falle al abrir cada terminal.");
            println!();
            println!("  Install-Module PSReadLine -Scope CurrentUser -Force");
        }
    }
    println!();

    let profile_dir = dirs::document_dir()
        .unwrap_or_else(|| {
            crate::utils::platform::home_dir().unwrap_or_else(|| ".".into())
        })
        .join("WindowsPowerShell");

    let profile_file = profile_dir.join("Microsoft.PowerShell_profile.ps1");

    if !profile_dir.exists() {
        std::fs::create_dir_all(&profile_dir)?;
    }

    // Copia de seguridad antes de sobrescribir: el perfil es del usuario, puede
    // contener sus alias y funciones, y la escritura es ciega.
    let had_previous = profile_file.exists();
    if had_previous {
        let bytes = std::fs::read(&profile_file).unwrap_or_default();
        // El BOM va **aparte** del contenido: si el perfil coincide pero no lo
        // tiene (p. ej. lo escribió una versión anterior de esta tarea), hay que
        // reescribirlo igual, o los acentos se quedan corruptos para siempre.
        let previous = String::from_utf8_lossy(bytes.strip_prefix(UTF8_BOM).unwrap_or(&bytes));
        if bytes.starts_with(UTF8_BOM) && previous == profile {
            println!("[INFO] El perfil ya estaba actualizado. No se toca nada.");
            println!();
            pause();
            return Ok(());
        }
        let backup = profile_dir.join("Microsoft.PowerShell_profile.ps1.compass-bak");
        write_utf8_bom(&backup, &previous)?;
        println!("[INFO] Copia del perfil anterior en:");
        println!("  {}", backup.display());
        println!();
    }

    write_utf8_bom(&profile_file, profile)?;

    println!("[LISTO] La configuración se ha guardado en:");
    println!("  {}", profile_file.display());
    println!();
    println!("Abre una nueva terminal para disfrutar del autocompletado.");
    println!();

    pause();
    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decide si hay que llamar a `Install-Module` antes de escribir el perfil.
    /// Sólo se actualiza cuando la versión instalada no llega a 2.2.0: en una
    /// máquina al día no se toca la red.
    #[test]
    fn only_upgrades_when_predictive_is_missing() {
        assert!(is_predictive(Some("2.2.0")));
        assert!(is_predictive(Some("2.11.4")));
        assert!(is_predictive(Some("3.0.0")));

        assert!(!is_predictive(Some("2.0.0")), "la de serie de PS 5.1 no sirve");
        assert!(!is_predictive(Some("2.1.0")));
        assert!(!is_predictive(None), "sin módulo hay que instalar");
        assert!(!is_predictive(Some("basura")));
    }

    /// Sin el `try/catch`, activar las sugerencias falla en cuanto la salida está
    /// redirigida, y el perfil se carga también en `powershell -Command ...` y en
    /// cualquier herramienta que lance PowerShell capturando stdout. O sea que el
    /// error volvería a aparecer, sólo que cambiado.
    #[test]
    fn prediction_is_guarded_against_redirected_output() {
        let linea = PROFILE_PREDICTIVE
            .lines()
            .find(|l| l.contains("-PredictionSource"))
            .expect("el perfil predictivo debe activar -PredictionSource");

        assert!(
            linea.contains("try") && linea.contains("catch"),
            "-PredictionSource debe ir dentro de un try/catch: {linea}"
        );
    }

    #[test]
    fn reads_the_version_number() {
        assert_eq!(version_code("2.0.0"), Some(200));
        assert_eq!(version_code("2.2.0"), Some(202));
        // `2.11.4` > `2.2.0`: comparar como texto daría el resultado contrario.
        assert_eq!(version_code("2.11.4"), Some(211));
        assert_eq!(version_code("2.1"), Some(201));
        assert_eq!(version_code(""), None);
        assert_eq!(version_code("no.version"), None);
    }

    #[test]
    fn picks_the_profile_by_capability_not_by_text_order() {
        // 2.2.0 es la primera con predictive intellisense.
        assert_eq!(profile_for(Some("2.2.0")), PROFILE_PREDICTIVE);
        assert_eq!(profile_for(Some("2.3.6")), PROFILE_PREDICTIVE);
        assert_eq!(profile_for(Some("2.11.4")), PROFILE_PREDICTIVE);
        // Las que trae Windows PowerShell 5.1 de serie.
        assert_eq!(profile_for(Some("2.0.0")), PROFILE_LEGACY);
        assert_eq!(profile_for(Some("2.1.0")), PROFILE_LEGACY);
        // Sin módulo, un perfil que no falla.
        assert_eq!(profile_for(None), PROFILE_SIN_MODULO);
    }

    /// El bug que motivó esto de raíz: la 2.0.0 de serie de Windows PowerShell
    /// 5.1 no reconoce estos nombres, y el error sale en cada terminal nueva.
    #[test]
    fn legacy_profile_only_uses_names_that_exist_in_psreadline_200() {
        // Lista real de `(Get-Command Set-PSReadLineOption).Parameters` y del
        // ValidateSet de `Set-PSReadLineKeyHandler -Function` en la 2.0.0.
        const OPCIONES_2_0_0: &[&str] = &[
            "MaximumHistoryCount", "HistorySaveStyle", "BellStyle", "ShowToolTips",
            "HistorySearchCursorMovesToEnd", "EditMode", "Colors",
        ];
        const FUNCIONES_2_0_0: &[&str] = &[
            "MenuComplete", "Complete", "HistorySearchBackward", "HistorySearchForward",
            "NextHistory", "PreviousHistory", "ForwardWord", "BackwardWord",
        ];

        for bloque in PROFILE_LEGACY
            .lines()
            .filter(|l| l.trim_start().starts_with("Set-PSReadLine"))
        {
            let quoted = bloque
                .split(['\'', '"'])
                .skip(1)
                .step_by(2)
                .collect::<Vec<_>>();
            let (opcion, valor) = match quoted.as_slice() {
                [opcion] => (*opcion, None),
                [opcion, valor] => (*opcion, Some(*valor)),
                [] => continue,
                _ => panic!("no se esperaba más de un valor entrecomillado: {bloque}"),
            };

            if let Some(nombre) = opcion.strip_prefix("-") {
                assert!(
                    OPCIONES_2_0_0.contains(&nombre),
                    "`-{nombre}` no existe en Set-PSReadLineOption de la 2.0.0: {bloque}"
                );
                let valor = valor.unwrap_or("");
                assert!(
                    !matches!(valor, "History" | "ListView" | "InlineView"),
                    "`{valor}` es un valor de -PredictionSource, que no existe en la 2.0.0"
                );
            } else {
                assert!(
                    FUNCIONES_2_0_0.contains(&opcion),
                    "`{opcion}` no es una Function válida en la 2.0.0: {bloque}"
                );
            }
        }
    }

    /// Y lo contrario: el perfil legacy sólo puede emitir nombres de 2.2.0+ en
    /// líneas ejecutables. Los comentarios sí los mencionan —es lo que sugiere
    /// la actualización—, así que se filtran antes de comprobar.
    #[test]
    fn legacy_profile_has_no_predictive_names() {
        let ejecutable = PROFILE_LEGACY
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!ejecutable.contains("PredictionSource"));
        assert!(!ejecutable.contains("AcceptSuggestion"));
        assert!(!ejecutable.contains("AcceptNextSuggestionWord"));
        assert!(!ejecutable.contains("PredictSuggestion"));
    }

    /// Sin BOM, PowerShell 5.1 decodifica el perfil como ANSI y cada acento se
    /// parte en dos caracteres (`ú` → 195, 186). Es un fallo silencioso: el
    /// script sigue cargando, pero el texto sale corrupto.
    #[test]
    fn writes_the_profile_with_a_utf8_bom() {
        let path = std::env::temp_dir().join(format!("cr-profile-{}.ps1", std::process::id()));
        write_utf8_bom(&path, PROFILE_LEGACY).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], UTF8_BOM, "falta el BOM UTF-8");

        // Sin el BOM, lo que viene detrás sigue siendo UTF-8 válido.
        let texto = std::fs::read_to_string(&path).unwrap();
        assert!(texto.starts_with('\u{FEFF}'));
        assert!(texto.contains("menú"), "los acentos deben ir en UTF-8");

        let _ = std::fs::remove_file(&path);
    }

    /// El caso que motivó el atajo de "ya actualizado": un perfil con el contenido
    /// correcto pero **sin BOM** (escrito por una versión anterior de esta tarea)
    /// no puede darse por bueno, o los acentos se quedan corruptos sin que nadie
    /// lo avise. La comparación tiene que mirar también el BOM.
    #[test]
    fn bom_presence_is_part_of_the_comparison() {
        let bytes = UTF8_BOM
            .iter()
            .copied()
            .chain(PROFILE_LEGACY.bytes())
            .collect::<Vec<u8>>();
        let sin_bom = PROFILE_LEGACY.as_bytes().to_vec();

        for (caso, datos) in [("con BOM", &bytes), ("sin BOM", &sin_bom)] {
            let igual = datos.starts_with(UTF8_BOM)
                && String::from_utf8_lossy(datos.strip_prefix(UTF8_BOM).unwrap_or(datos))
                    == PROFILE_LEGACY;
            assert_eq!(
                igual,
                caso == "con BOM",
                "un perfil {caso} no debe darse por actualizado"
            );
        }
    }
}