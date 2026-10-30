//! Embebe el icono y los metadatos de versión en `compass.exe`.
//!
//! Sin `RT_GROUP_ICON` el binario muestra el icono genérico de consola (el
//! mismo que cmd) en Explorer y en la pestaña de Windows Terminal, y sin
//! `FileDescription` el perfil dinámico de la terminal no tiene nombre.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/Icon.ico");

    #[cfg(windows)]
    {
        let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        // RC interpreta `\a`, `\i`… como escapes dentro de las comillas, así que
        // la ruta se pasa con `/`.
        let icon = root
            .join("assets/icon/Icon.ico")
            .display()
            .to_string()
            .replace('\\', "/");
        let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
        let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        let (major, minor, patch) = (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        );

        let rc = format!(
            r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Compass\0"
      VALUE "ProductName", "Compass\0"
      VALUE "OriginalFilename", "compass.exe\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
            icon = icon,
        );

        let rc_path =
            std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("compass.rc");
        std::fs::write(&rc_path, rc).unwrap();
        embed_resource::compile(&rc_path, embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
