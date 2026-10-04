#!/usr/bin/env bash
# Prueba de la lógica de salto de install.sh, sin instalar nada.
set -u
cd "$(dirname "$0")/.."

prueba() {
    local etiqueta="$1" exe="$2"
    local newest
    newest="$(find src Cargo.toml Cargo.lock -type f -newer "$exe" 2>/dev/null | head -n 1)"
    if [ -z "$newest" ]; then
        echo "$etiqueta -> NEEDS_BUILD=0 (salta)"
    else
        echo "$etiqueta -> NEEDS_BUILD=1 (recompila, via: $newest)"
    fi
}

# Binario "instalado" hace 2 horas: las fuentes son más nuevas.
touch -d "2 hours ago" /tmp/cr-antiguo
prueba "exe viejo        " /tmp/cr-antiguo

# Binario "instalado" ahora: ninguna fuente es más nueva.
touch /tmp/cr-nuevo
prueba "exe recien hecho " /tmp/cr-nuevo

rm -f /tmp/cr-antiguo /tmp/cr-nuevo