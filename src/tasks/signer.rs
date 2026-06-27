use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result};
use inquire::{Password, Select, Text};

use crate::utils::{cmd, context::ProjectContext};

pub fn create_or_import(ctx: &ProjectContext) -> Result<()> {
    let android_app = ctx.script_dir.join("src-tauri\\gen\\android\\app");
    if !android_app.exists() {
        eprintln!("[ERROR] No se encontró 'src-tauri/gen/android/app'.");
        eprintln!("Ejecuta primero: npx tauri android init");
        pause();
        return Ok(());
    }

    cmd::clear_screen();
    println!("╔══════════════════════════════════════════════════╗");
    println!("║       CONFIGURAR FIRMAS ANDROID (RELEASE)        ║");
    println!("╚══════════════════════════════════════════════════╝");
    println!();

    let options = vec![
        "1. Crear un keystore nuevo automáticamente",
        "2. Usar un keystore existente",
        "3. Configurar firma manualmente (keystore.properties)",
        "4. Volver al menú anterior",
    ];

    let choice = Select::new("¿Qué deseas hacer?", options)
        .with_vim_mode(true)
        .prompt()?;

    match choice {
        "1. Crear un keystore nuevo automáticamente" => {
            create_keystore(ctx, &android_app)?;
        }
        "2. Usar un keystore existente" => {
            import_keystore(ctx, &android_app)?;
        }
        "3. Configurar firma manualmente (keystore.properties)" => {
            manual_configure(ctx, &android_app)?;
        }
        _ => {
            return Ok(());
        }
    }

    pause();
    Ok(())
}

fn create_keystore(_ctx: &ProjectContext, android_app: &Path) -> Result<()> {
    println!("\n--- Creando nueva llave (Keystore) ---\n");

    let raw_name: String = Text::new("Nombre del keystore (sin extensión):")
        .with_default("release")
        .prompt()?;
    let name = if raw_name.ends_with(".keystore") { raw_name } else { format!("{}.keystore", raw_name) };

    let alias: String = Text::new("Alias de la llave:")
        .with_default("upload")
        .prompt()?;

    let pass = Password::new("Contraseña para el Keystore (mínimo 6 caracteres):")
        .without_confirmation()
        .prompt()?;

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
    write_properties(android_app, &name, &pass, &pass, &alias)?;
    patch_build_gradle(android_app)?;

    println!("\n[OK] ¡Firmas configuradas exitosamente!");
    Ok(())
}

fn import_keystore(_ctx: &ProjectContext, android_app: &Path) -> Result<()> {
    println!("\n--- Usando llave existente ---\n");

    let path_str: String = Text::new("Ruta a tu archivo .keystore o .jks:")
        .prompt()?;

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

    let alias: String = Text::new("Alias de la llave (Key Alias):")
        .prompt()?;

    let store_pass = Password::new("Contraseña del Keystore (Store Password):")
        .without_confirmation()
        .prompt()?;

    let key_pass_input = Password::new("Contraseña de la Llave (Key Password) [Enter si es la misma]:")
        .without_confirmation()
        .prompt()
        .unwrap_or_default();
    let key_pass = if key_pass_input.is_empty() { store_pass.clone() } else { key_pass_input };

    let k_name = keystore_file.file_name().unwrap().to_str().unwrap().to_string();
    let dest = android_app.join(&k_name);
    fs::copy(&keystore_file, &dest).context("Error al copiar el archivo")?;
    println!("  [OK] Archivo copiado correctamente.");

    write_properties(android_app, &k_name, &store_pass, &key_pass, &alias)?;
    patch_build_gradle(android_app)?;

    println!("\n[OK] ¡Firmas configuradas exitosamente!");
    Ok(())
}

fn manual_configure(_ctx: &ProjectContext, android_app: &Path) -> Result<()> {
    println!("\n--- Configuración manual ---\n");

    let name: String = Text::new("Nombre del archivo .keystore (ej: release.keystore):")
        .with_default("release.keystore")
        .prompt()?;

    let alias: String = Text::new("Alias de la llave:")
        .with_default("upload")
        .prompt()?;

    let store_pass = Password::new("Contraseña del Keystore (Store Password):")
        .without_confirmation()
        .prompt()?;

    let key_pass_input = Password::new("Contraseña de la Llave (Key Password) [Enter si es la misma]:")
        .without_confirmation()
        .prompt()
        .unwrap_or_default();
    let key_pass = if key_pass_input.is_empty() { store_pass.clone() } else { key_pass_input };

    write_properties(android_app, &name, &store_pass, &key_pass, &alias)?;
    patch_build_gradle(android_app)?;

    println!("\n[OK] Configuración manual completada.");
    println!("  Asegúrate de que '{}' exista en la carpeta app/.", name);
    Ok(())
}

fn find_keytool() -> Result<String> {
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

fn write_properties(android_app: &Path, keystore_name: &str, store_pass: &str, key_pass: &str, key_alias: &str) -> Result<()> {
    let content = format!(
        "storePassword={}\nkeyPassword={}\nkeyAlias={}\nstoreFile={}\n",
        store_pass, key_pass, key_alias, keystore_name
    );
    let props_path = android_app.join("keystore.properties");
    fs::write(&props_path, content).context("Error al escribir keystore.properties")?;
    println!("  [OK] keystore.properties generado.");
    Ok(())
}

fn patch_build_gradle(android_app: &Path) -> Result<()> {
    let gradle_path = android_app.join("build.gradle.kts");
    if !gradle_path.exists() {
        eprintln!("[WARNING] No se encontró build.gradle.kts en: {}", android_app.display());
        eprintln!("  Puedes agregar la configuración manualmente.");
        return Ok(());
    }

    let mut content = fs::read_to_string(&gradle_path).context("Error al leer build.gradle.kts")?;
    let mut modified = false;

    // 1. Inject properties loader
    if !content.contains("val keystorePropertiesFile") {
        let loader = "\nval keystorePropertiesFile = rootProject.file(\"app/keystore.properties\")\nval keystoreProperties = Properties()\nif (keystorePropertiesFile.exists()) {\n    keystorePropertiesFile.inputStream().use { keystoreProperties.load(it) }\n}\n";
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

    if modified {
        fs::write(&gradle_path, content).context("Error al escribir build.gradle.kts")?;
        println!("  [OK] build.gradle.kts actualizado con configuración de firma.");
    } else {
        println!("  build.gradle.kts ya contenía la configuración.");
    }

    Ok(())
}

fn pause() {
    print!("\nPresiona Enter para continuar...");
    io::stdout().flush().ok();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).ok();
}
