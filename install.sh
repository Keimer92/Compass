#!/usr/bin/env bash
#
# Instalador de code-runner para Linux (Pop!_OS / Ubuntu).
#
# No instala los menús contextuales: eso es exclusivo de Windows (registro).
# Las tarjetas "Menu Ctx" y "Terminal" quedan ocultas en Linux.
#
set -u

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
    echo "[ERROR] No se encuentra Cargo.toml."
    echo "Asegúrate de que install.sh esté en la raíz del proyecto."
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

# ── 4. Detectar terminal (para las ventanas de dev/build) ──────────────
echo "[INFO] Comprobando emulador de terminal..."
FOUND_TERM=""
for t in gnome-terminal konsole xfce4-terminal alacritty kitty wezterm xterm; do
    if command -v "$t" >/dev/null 2>&1; then
        FOUND_TERM="$t"
        break
    fi
done
if [ -n "$FOUND_TERM" ]; then
    echo "[OK] Terminal detectada: $FOUND_TERM"
else
    echo "[WARN] No se encontró ningún emulador de terminal."
    echo "       Instala uno para que las tareas abran ventana:"
    echo "         sudo apt install gnome-terminal"
fi
echo

# ── 5. Android SDK ────────────────────────────────────────────────────
if [ -n "${ANDROID_HOME:-}" ] || [ -n "${ANDROID_SDK_ROOT:-}" ]; then
    echo "[OK] ANDROID_HOME configurado: ${ANDROID_HOME:-$ANDROID_SDK_ROOT}"
else
    echo "[WARN] ANDROID_HOME no está configurado."
    echo "       Las tareas Android no funcionarán hasta que lo definas:"
    echo "         export ANDROID_HOME=\$HOME/Android/Sdk"
fi
echo

# ── 6. Resumen ────────────────────────────────────────────────────────
echo "============================================="
if [ "$EXIT_CODE" -eq 0 ]; then
    echo "  [OK] Instalación completada"
else
    echo "  [WARN] Instalación con problemas"
fi
echo "============================================="
echo
echo "Ya puedes usar 'code-runner' desde cualquier terminal."
echo
echo "Notas para Linux:"
echo "  - Las tareas de Tauri, Rust, Kotlin y Android funcionan."
echo "  - Flutter y KMP desktop compilan para Windows: usa la VM de Windows"
echo "    o un runner remoto para esos builds."
echo "  - Las tarjetas 'Menu Ctx' y 'Terminal' están ocultas (solo Windows)."
echo