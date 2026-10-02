#!/usr/bin/env bash
#
# Instalador de code-runner para Linux (Pop!_OS / Ubuntu / COSMIC).
#
# Instala:
#   - el binario (cargo install)
#   - la entrada del launcher (.desktop) + iconos hicolor
#   - opcionalmente el menú contextual de COSMIC Files (--with-context-menu)
#
# La tarjeta "Terminal" del menú TUI queda oculta en Linux: escribe un perfil
# de PowerShell que no aplica. "Menu Ctx" sí funciona, vía context_actions.
#
set -u

# El instalador trabaja sobre su propia carpeta, no sobre el CWD de quien lo
# invoca. Sin esto, `./ruta/al/proyecto/install.sh` fallaría el check de
# Cargo.toml aunque el mensaje dijera lo contrario.
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$SCRIPT_DIR" || exit 1

WITH_CONTEXT_MENU=0
for arg in "$@"; do
    case "$arg" in
        --with-context-menu) WITH_CONTEXT_MENU=1 ;;
        -h|--help)
            cat <<'EOF'
Uso: install.sh [--with-context-menu]

  --with-context-menu   Instala también las entradas de Vibe Coding en el
                        menú contextual de COSMIC Files (context_actions).
                        Requiere cosmic-files >= 1.0.10.
EOF
            exit 0
            ;;
        *) ;;
    esac
done

echo
echo "============================================="
echo "  Code Runner - Instalador para Linux"
echo "============================================="
echo

# ── 1. Verificar Rust ─────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
    echo "[ERROR] Cargo (Rust) no está instalado."
    echo
    echo "Instálalo con:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo
    exit 1
fi

if [ ! -f "Cargo.toml" ]; then
    echo "[ERROR] No se encuentra Cargo.toml en: $SCRIPT_DIR"
    echo
    exit 1
fi

# ── 2. Cerrar instancias en ejecución ─────────────────────────────────
if pgrep -x code-runner >/dev/null 2>&1; then
    echo "[INFO] Cerrando code-runner en ejecución..."
    pkill -x code-runner && sleep 1
fi

# ── 3. Compilar e instalar ────────────────────────────────────────────
echo "[INFO] Compilando e instalando..."
cargo install --path . --force
EXIT_CODE=$?
echo
echo "[INFO] Compilación finalizada."

if [ "$EXIT_CODE" -ne 0 ]; then
    echo
    echo "============================================="
    echo "  [WARN] La compilación tuvo problemas."
    echo "============================================="
    echo "Se usará la versión anterior de code-runner si está instalada."
    echo
fi

# Ruta del binario recién instalado. `command -v` puede devolver un shim de
# rustup o un alias, así que se busca también en ~/.cargo/bin.
EXE_BIN="$(command -v code-runner 2>/dev/null || true)"
if [ -z "$EXE_BIN" ] || [ ! -x "$EXE_BIN" ]; then
    for cand in "$HOME/.cargo/bin/code-runner" /usr/local/bin/code-runner; do
        [ -x "$cand" ] && EXE_BIN="$cand" && break
    done
fi
EXE_BIN="${EXE_BIN:-code-runner}"

# ── 4. Detectar terminal ──────────────────────────────────────────────
# El orden replica `TERMINALS` en src/utils/cmd.rs: cosmic-term primero,
# porque es el terminal por defecto de Pop!_OS y puede ser el único instalado.
echo "[INFO] Comprobando emulador de terminal..."
FOUND_TERM=""
FOUND_TERM_ARGS=""
for entry in \
    "cosmic-term:--title \"Code Runner\"" \
    "gnome-terminal:--title \"Code Runner\"" \
    "konsole:--" \
    "xfce4-terminal:--title \"Code Runner\"" \
    "alacritty:--title \"Code Runner\"" \
    "kitty:--title \"Code Runner\"" \
    "wezterm:start --" \
    "xterm:-T \"Code Runner\""
do
    bin="${entry%%:*}"
    if command -v "$bin" >/dev/null 2>&1; then
        FOUND_TERM="$bin"
        FOUND_TERM_ARGS="${entry#*:}"
        break
    fi
done
echo
if [ -n "$FOUND_TERM" ]; then
    echo "[OK] Terminal detectada: $FOUND_TERM"
else
    echo "[WARN] No se encontró ningún emulador de terminal."
    echo "       Sin uno, las tareas no pueden abrir ventana y la entrada del"
    echo "       launcher no se instalará. Instala uno, por ejemplo:"
    echo "         sudo apt install gnome-terminal"
fi
echo

# ── 5. Iconos PNG (el launcher de COSMIC no lee .ico) ──────────────────
# Icon.ico es un BMP-DIB de 32bpp que ImageMagick/icoutils no están en Pop!_OS,
# así que se convierte con el conversor de la stdlib incluido en tools/.
ICONS_OK=0
if [ -d "assets/icon" ]; then
    echo "[INFO] Preparando iconos..."
    if ! ls assets/icon/code-runner-*.png >/dev/null 2>&1; then
        if command -v python3 >/dev/null 2>&1; then
            python3 tools/ico2png.py >/dev/null 2>&1 \
                && echo "[OK] Iconos convertidos desde assets/icon/Icon.ico" \
                || echo "[WARN] No se pudieron convertir los .ico a PNG."
        else
            echo "[WARN] Falta python3 para convertir los iconos."
        fi
    fi

    ICON_BASE="$HOME/.local/share/icons/hicolor"
    for png in assets/icon/code-runner-*.png; do
        [ -e "$png" ] || continue
        size="$(basename "$png" .png)"
        size="${size##*-}"            # code-runner-64 -> 64
        dest="$ICON_BASE/${size}x${size}/apps"
        if mkdir -p "$dest" 2>/dev/null \
            && cp -f "$png" "$dest/code-runner.png" 2>/dev/null; then
            ICONS_OK=1
        else
            echo "[WARN] No se pudo escribir el icono en: $dest"
        fi
    done
    [ "$ICONS_OK" -eq 1 ] && echo "[OK] Iconos instalados en ~/.local/share/icons/hicolor"
fi
echo

# ── 6. Entrada del launcher ────────────────────────────────────────────
# code-runner es una TUI: el launcher no puede lanzarla directamente (no hay
# tty), así que la entrada envuelve el binario en un emulador de terminal.
# `--pick` hace que abra el selector de proyecto, porque desde el launcher el
# directorio actual es $HOME y no hay nada que autodetectar.
LAUNCHER_OK=0
if [ -n "$FOUND_TERM" ] && [ -f "assets/code-runner.desktop.in" ]; then
    APPS_DIR="$HOME/.local/share/applications"
    if mkdir -p "$APPS_DIR" 2>/dev/null; then
        # El campo Exec de .desktop separa argumentos por espacios; el título va
        # entrecomillado para que "Code Runner" sea un solo argumento.
        EXEC_CMD="$FOUND_TERM $FOUND_TERM_ARGS -e \"$EXE_BIN\" --pick"
        if [ "$FOUND_TERM_ARGS" = "--" ] || [ "$FOUND_TERM_ARGS" = "start --" ]; then
            EXEC_CMD="$FOUND_TERM $FOUND_TERM_ARGS \"$EXE_BIN\" --pick"
        fi
        if sed "s|@EXEC_CMD@|$EXEC_CMD|" assets/code-runner.desktop.in \
            > "$APPS_DIR/code-runner.desktop" 2>/dev/null; then
            chmod +x "$APPS_DIR/code-runner.desktop"
            # Validar antes de darlo por bueno: un .desktop mal formado desaparece
            # del launcher en silencio.
            if command -v desktop-file-validate >/dev/null 2>&1; then
                if desktop-file-validate "$APPS_DIR/code-runner.desktop" 2>&1; then
                    LAUNCHER_OK=1
                else
                    echo "[WARN] El .desktop no pasa desktop-file-validate:"
                    desktop-file-validate "$APPS_DIR/code-runner.desktop" 2>&1 | sed 's/^/       /'
                fi
            else
                LAUNCHER_OK=1
            fi
            if [ "$LAUNCHER_OK" -eq 1 ]; then
                echo "[OK] Entrada del launcher: $APPS_DIR/code-runner.desktop"
                echo "     Exec: $EXEC_CMD"
            fi
        else
            echo "[WARN] No se pudo escribir el .desktop"
        fi
    else
        echo "[WARN] No se pudo crear $APPS_DIR"
    fi

    # Refrescar la caché de la aplicación para que COSMIC la vea sin reiniciar
    # la sesión. `update-desktop-database` viene en desktop-file-utils; si no está,
    # el launcher lo relee por su cuenta en unos segundos.
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$HOME/.local/share/applications" 2>/dev/null \
            && echo "[OK] Caché de aplicaciones actualizada."
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 \
            && echo "[OK] Caché de iconos actualizada."
    fi
else
    echo "[--] Entrada del launcher omitida (falta terminal o plantilla)."
fi
echo

# ── 7. Menú contextual de COSMIC Files (opcional) ──────────────────────
if [ "$WITH_CONTEXT_MENU" -eq 1 ]; then
    echo "[INFO] Instalando menú contextual en COSMIC Files..."
    if command -v code-runner >/dev/null 2>&1; then
        if code-runner --task opencode-install; then
            echo "[OK] Menú contextual instalado."
        else
            echo "[WARN] El menú contextual no se pudo instalar."
        fi
    fi
    echo "[INFO] Cierra COSMIC Files por completo y vuelve a abrirlo."
    echo
fi

# ── 8. Dependencias de Android ─────────────────────────────────────────
# Se comprueba el SDK *y* el JDK: sin java no hay keytool (firmas/huellas),
# ni sdkmanager (NDK), ni Gradle. Falta cualquiera de los dos, todas las tareas
# Android y de firma fallan.
echo "[INFO] Comprobando dependencias..."
DEPS_OK=1

# ANDROID_HOME: la app lo autodetecta en varios sitios, así que basta con que
# exista alguno.
ANDROID_SDK=""
for k in ANDROID_HOME ANDROID_SDK_ROOT; do
    [ -n "${!k:-}" ] && [ -d "${!k}" ] && ANDROID_SDK="${!k}" && break
done
if [ -z "$ANDROID_SDK" ]; then
    for cand in "$HOME/Android/Sdk" "$HOME/android-sdk" /usr/lib/android-sdk; do
        [ -d "$cand" ] && ANDROID_SDK="$cand" && break
    done
fi

if [ -n "$ANDROID_SDK" ]; then
    echo "[OK] Android SDK: $ANDROID_SDK"
    # El emulador no se comprueba: el flujo usa sólo dispositivos físicos.
    for tool in "platform-tools/adb" "cmdline-tools/latest/bin/sdkmanager"; do
        if [ -x "$ANDROID_SDK/$tool" ]; then
            echo "     ✔ ${tool##*/}"
        else
            echo "     [WARN] falta ${tool}"
        fi
    done
    if [ ! -x "$ANDROID_SDK/platform-tools/adb" ]; then
        DEPS_OK=0
    fi
else
    DEPS_OK=0
    echo "[WARN] No se encontró el Android SDK."
fi

if command -v java >/dev/null 2>&1 || [ -n "${JAVA_HOME:-}" ]; then
    if command -v keytool >/dev/null 2>&1 || [ -x "${JAVA_HOME:-/nope}/bin/keytool" ]; then
        echo "[OK] JDK: $(java -version 2>&1 | head -1)"
    else
        DEPS_OK=0
        echo "[WARN] Hay java pero no keytool (instala un JDK, no un JRE)."
    fi
else
    DEPS_OK=0
    echo "[WARN] No hay JDK instalado."
fi

for tool in node npm; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "[WARN] Falta '$tool' (necesario para Tauri y el motor WhatsApp)."
        DEPS_OK=0
    }
done

if [ "$DEPS_OK" -ne 1 ]; then
    echo
    echo "  Para completar en Pop!_OS:"
    echo "    sudo apt install openjdk-21-jdk-headless wl-clipboard"
    echo "    sudo apt install google-android-cmdline-tools-13.0-installer"
    echo "    export ANDROID_HOME=\$HOME/Android/Sdk"
    echo "    sdkmanager --licenses"
    echo "    sdkmanager \"platform-tools\" \"platforms;android-35\" \"build-tools;35.0.0\""
    echo "    rustup target add aarch64-linux-android armv7-linux-androideabi"
    echo
fi
echo

# ── 9. Resumen ────────────────────────────────────────────────────────
echo "============================================="
if [ "$EXIT_CODE" -eq 0 ]; then
    echo "  [OK] Instalación completada"
else
    echo "  [WARN] Instalación con problemas"
fi
echo "============================================="
echo
echo "  Binario      : $EXE_BIN"
[ "$LAUNCHER_OK" -eq 1 ] && echo "  Launcher     : instalado (búscalo como 'Code Runner')"
[ "$ICONS_OK" -eq 1 ] && echo "  Iconos       : instalados"
echo
echo "Uso:"
echo "  code-runner           # abre el menú TUI en el proyecto actual"
echo "  code-runner --pick    # elige la carpeta del proyecto primero"
echo "  code-runner --task clean --path /ruta/proyecto"
echo
echo "Notas para Linux:"
echo "  - Funcionan: Tauri (dev + Android), Rust, Kotlin/KMP, Android."
echo "  - Las tarjetas Desktop/Linux reemplazan a las de Windows."
echo "  - 'Terminal' está oculta (escribe un perfil de PowerShell)."
echo "  - 'Menu Ctx' usa context_actions de COSMIC Files."
echo "  - Las ventanas de dev/build usan $FOUND_TERM y heredan tu PATH;"
echo "    si las lanzas desde el launcher y algún comando no se encuentra,"
echo "    revisa que tu gestor de versiones (fnm/nvm/asdf) inicialice en"
echo "    ~/.profile, no sólo en ~/.bashrc."
echo
