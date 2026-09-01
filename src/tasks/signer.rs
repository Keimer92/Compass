use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result};
use inquire::Select;

use crate::utils::{cmd, context::{kotlin_android_dir, ProjectContext, ProjectType}, ui};

pub fn create_or_import(ctx: &ProjectContext) -> Result<()> {
    let (android_dir, props_name) = match ctx.project_type {
        ProjectType::Tauri => (ctx.script_dir.join("src-tauri\\gen\\android\\app"), "keystore.properties"),
        ProjectType::Flutter => (ctx.script_dir.join("android\\app"), "key.properties"),
        ProjectType::Kotlin => (kotlin_android_dir(ctx), "keystore.properties"),
        ProjectType::Rust => {
            eprintln!("[ERROR] No aplicable a proyectos Rust.");
            ui::pause();
            return Ok(());
        }
    };

    if !android_dir.exists() {
        eprintln!("[ERROR] No se encontró el directorio Android del proyecto.");
        match ctx.project_type {
            ProjectType::Tauri => eprintln!("Ejecuta primero: npx tauri android init"),
            ProjectType::Flutter => eprintln!("Asegúrate de que exista android/app/ en tu proyecto Flutter."),
            ProjectType::Kotlin => eprintln!("Asegúrate de que exista composeApp/ en tu proyecto KMP."),
            ProjectType::Rust => {} // already handled above
        }
        ui::pause();
        return Ok(());
    }

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       CONFIGURAR FIRMAS ANDROID (RELEASE)        ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let opt_text = format!("3. Configurar firma manualmente ({})", props_name);
    let options = vec![
        "1. Crear un keystore nuevo automáticamente",
        "2. Usar un keystore existente",
        opt_text.as_str(),
        "4. Volver al menú anterior",
    ];

    let choice = Select::new("¿Qué deseas hacer?", options)
        .with_vim_mode(true)
        .prompt()?;

    match choice {
        "1. Crear un keystore nuevo automáticamente" => {
            create_keystore(ctx, &android_dir, props_name)?;
        }
        "2. Usar un keystore existente" => {
            import_keystore(ctx, &android_dir, props_name)?;
        }
        s if s.starts_with("3.") => {
            manual_configure(ctx, &android_dir, props_name)?;
        }
        _ => {
            return Ok(());
        }
    }

    ui::pause();
    Ok(())
}

fn create_keystore(_ctx: &ProjectContext, android_app: &Path, props_name: &str) -> Result<()> {
    println!();
    ui::info_banner("Crear nueva llave (Keystore)");

    let raw_name = ui::ask_text("Nombre del keystore (sin extensión)", Some("release"))?;
    let name = if raw_name.ends_with(".keystore") { raw_name } else { format!("{}.keystore", raw_name) };

    let alias = ui::ask_text("Alias de la llave", Some("upload"))?;

    let pass = ui::ask_password("Contraseña para el Keystore (mín 6 caracteres)")?;

    let dest_keystore = android_app.join(&name);

    let keytool = find_keytool()?;
    println!("  keytool encontrado en: {}", keytool);
    println!();

    // Clean up existing keystores
    println!("  Limpiando keystores existentes...");
    if let Ok(entries) = fs::read_dir(android_app) {
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

    // Generate keystore
    println!("\n  Generando keystore (esto puede tardar unos segundos)...");
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
        // Retry with temp alias
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
            return Ok(());
        }

        // Rename alias
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

    // Write properties and patch gradle
    write_properties(android_app, &name, &pass, &pass, &alias, props_name)?;
    patch_build_gradle(android_app, props_name)?;

    ui::success_banner("Firmas configuradas exitosamente");
    Ok(())
}

fn import_keystore(_ctx: &ProjectContext, android_app: &Path, props_name: &str) -> Result<()> {
    println!();
    ui::info_banner("Usando llave existente");

    let path_str = ui::ask_text("Ruta a tu archivo .keystore o .jks", None)?;

    let mut keystore_file = PathBuf::from(&path_str);

    if !keystore_file.exists() {
        eprintln!("[ERROR] No se encontró el archivo: {}", path_str);
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
            return Ok(());
        }
        println!("  Llave encontrada: {}", keystore_file.file_name().unwrap().to_string_lossy());
    }

    let alias = ui::ask_text("Alias de la llave (Key Alias)", None)?;

    let store_pass = ui::ask_password("Contraseña del Keystore (Store Password)")?;

    let key_pass_input = ui::ask_password("Contraseña de la Llave (Key Password)")?;
    let key_pass = if key_pass_input.is_empty() { store_pass.clone() } else { key_pass_input };

    let k_name = keystore_file.file_name().unwrap().to_str().unwrap().to_string();
    let dest = android_app.join(&k_name);
    fs::copy(&keystore_file, &dest).context("Error al copiar el archivo")?;
    println!("  [OK] Archivo copiado correctamente.");

    write_properties(android_app, &k_name, &store_pass, &key_pass, &alias, props_name)?;
    patch_build_gradle(android_app, props_name)?;

    ui::success_banner("Firmas configuradas exitosamente");
    Ok(())
}

fn manual_configure(_ctx: &ProjectContext, android_app: &Path, props_name: &str) -> Result<()> {
    println!();
    ui::info_banner("Configuración manual");

    let name = ui::ask_text("Nombre del archivo .keystore (ej: release.keystore)", Some("release.keystore"))?;

    let alias = ui::ask_text("Alias de la llave", Some("upload"))?;

    let store_pass = ui::ask_password("Contraseña del Keystore (Store Password)")?;

    let key_pass_input = ui::ask_password("Contraseña de la Llave (Key Password)")?;
    let key_pass = if key_pass_input.is_empty() { store_pass.clone() } else { key_pass_input };

    write_properties(android_app, &name, &store_pass, &key_pass, &alias, props_name)?;
    patch_build_gradle(android_app, props_name)?;

    ui::success_banner("Configuración manual completada");
    println!("  Asegúrate de que '{}' exista en la carpeta app/.", name);
    Ok(())
}

pub(crate) fn find_keytool() -> Result<String> {
    let mut paths: Vec<String> = Vec::new();

    paths.push("keytool".to_string());

    if let Ok(jh) = env::var("JAVA_HOME") {
        paths.push(format!("{}\\bin\\keytool.exe", jh));
    }

    paths.push(r"C:\Program Files\Android\Android Studio\jbr\bin\keytool.exe".to_string());
    paths.push(r"C:\Program Files\Android\Android Studio\jre\bin\keytool.exe".to_string());

    for base in &["C:\\Program Files\\Java", "C:\\Program Files (x86)\\Java"] {
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                let kt = entry.path().join("bin").join("keytool.exe");
                if kt.exists() {
                    paths.push(kt.to_string_lossy().into_owned());
                }
            }
        }
    }

    if let Ok(local) = env::var("LOCALAPPDATA") {
        let candidates = vec![
            format!("{}\\Programs\\Android Studio\\jbr\\bin\\keytool.exe", local),
            format!("{}\\Programs\\Android Studio\\jre\\bin\\keytool.exe", local),
        ];
        for c in &candidates {
            if Path::new(c).exists() {
                paths.push(c.clone());
            }
        }
    }

    for p in &paths {
        if p.is_empty() { continue; }
        if Command::new(p).arg("-help").output().is_ok() {
            return Ok(p.clone());
        }
    }

    anyhow::bail!(
        "No se pudo encontrar 'keytool.exe'.\n\
         Asegúrate de tener Android Studio o Java JDK instalado,\n\
         o agrega Java a tus variables de entorno (PATH/JAVA_HOME)."
    );
}

fn write_properties(android_app: &Path, keystore_name: &str, store_pass: &str, key_pass: &str, key_alias: &str, props_name: &str) -> Result<()> {
    let content = format!(
        "storePassword={}\nkeyPassword={}\nkeyAlias={}\nstoreFile={}\n",
        store_pass, key_pass, key_alias, keystore_name
    );
    let props_path = android_app.join(props_name);
    fs::write(&props_path, content).context(format!("Error al escribir {}", props_name))?;
    println!("  [OK] {} generado.", props_name);
    Ok(())
}

fn patch_build_gradle(android_app: &Path, props_name: &str) -> Result<()> {
    // Try .kts first, then .groovy
    let gradle_path = android_app.join("build.gradle.kts");
    let use_kts = gradle_path.exists();
    let gradle_path = if use_kts {
        gradle_path
    } else {
        let groovy = android_app.join("build.gradle");
        if !groovy.exists() {
            eprintln!("[WARNING] No se encontró build.gradle(.kts) en: {}", android_app.display());
            eprintln!("  Puedes agregar la configuración manualmente.");
            return Ok(());
        }
        groovy
    };

    let mut content = fs::read_to_string(&gradle_path).context(format!("Error al leer {}", gradle_path.display()))?;
    let mut modified = false;

    let module_name = android_app
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("app");

    if use_kts {
        // Kotlin Script (.kts)
        // Asegura el import de java.util.Properties (requerido por el loader inyectado).
        if !content.contains("import java.util.Properties") && !content.contains("import java.util.*") {
            content = format!("import java.util.Properties\n\n{}", content);
            modified = true;
        }

        let properties_ref = format!("{}/{}", module_name, props_name);

        // 1. Inject properties loader
        let load_marker = "val keystorePropertiesFile";
        if !content.contains(load_marker) {
            let loader = format!("\nval keystorePropertiesFile = rootProject.file(\"{}\")\nval keystoreProperties = Properties()\nif (keystorePropertiesFile.exists()) {{\n    keystorePropertiesFile.inputStream().use {{ keystoreProperties.load(it) }}\n}}\n", properties_ref);
            content = content.replace("android {", &format!("{}\nandroid {{", loader));
            modified = true;
        }

        // 2. Inject signingConfigs
        if !content.contains("signingConfigs {") {
            let signing = "\n    signingConfigs {\n        if (keystorePropertiesFile.exists()) {\n            create(\"release\") {\n                keyAlias = keystoreProperties.getProperty(\"keyAlias\") ?: \"\"\n                keyPassword = keystoreProperties.getProperty(\"keyPassword\") ?: \"\"\n                storeFile = file(keystoreProperties.getProperty(\"storeFile\") ?: \"\")\n                storePassword = keystoreProperties.getProperty(\"storePassword\") ?: \"\"\n            }\n        }\n    }\n";
            content = content.replace("    defaultConfig {", &format!("{}\n    defaultConfig {{", signing));
            modified = true;
        }

        // 3. Apply signing to release build type
        if !content.contains("signingConfig = signingConfigs.getByName(\"release\")") {
            let apply = "\n            if (keystorePropertiesFile.exists()) {\n                signingConfig = signingConfigs.getByName(\"release\")\n            }";
            content = content.replace("getByName(\"release\") {", &format!("getByName(\"release\") {{{}", apply));
            modified = true;
        }
    } else {
        // Groovy (.gradle)
        let properties_ref = format!("{}/{}", module_name, props_name);

        // 1. Inject properties loader
        if !content.contains("def keystorePropertiesFile") {
            let loader = format!("\ndef keystorePropertiesFile = rootProject.file('{}')\ndef keystoreProperties = new Properties()\nif (keystorePropertiesFile.exists()) {{\n    keystoreProperties.load(new FileInputStream(keystorePropertiesFile))\n}}\n", properties_ref);
            content = content.replace("android {", &format!("{}\nandroid {{", loader));
            modified = true;
        }

        // 2. Inject signingConfigs
        if !content.contains("signingConfigs {") {
            let signing = "\n    signingConfigs {\n        if (keystorePropertiesFile.exists()) {\n            release {\n                keyAlias keystoreProperties['keyAlias']\n                keyPassword keystoreProperties['keyPassword']\n                storeFile file(keystoreProperties['storeFile'])\n                storePassword keystoreProperties['storePassword']\n            }\n        }\n    }\n";
            content = content.replace("    defaultConfig {", &format!("{}\n    defaultConfig {{", signing));
            modified = true;
        }

        // 3. Apply signing to release build type
        if !content.contains("signingConfig signingConfigs.release") {
            let apply = "\n            if (keystorePropertiesFile.exists()) {\n                signingConfig = signingConfigs.release\n            }";
            content = content.replace("release {", &format!("release {{{}", apply));
            modified = true;
        }
    }

    if modified {
        fs::write(&gradle_path, content).context(format!("Error al escribir {}", gradle_path.display()))?;
        println!("  [OK] {} actualizado con configuración de firma.", gradle_path.file_name().unwrap().to_string_lossy());
    } else {
        println!("  {} ya contenía la configuración.", gradle_path.file_name().unwrap().to_string_lossy());
    }

    Ok(())
}


