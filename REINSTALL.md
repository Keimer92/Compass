# Developing Runner — Reinstalación Rápida

Si formateaste tu PC o estás en un equipo nuevo, sigue estos pasos para tener todo listo en segundos.

## Requisito previo

- [Rust](https://rustup.rs/) instalado en el sistema
- Conexión a Internet (solo la primera vez)

## Reinstalar (o actualizar)

Abre una terminal en **esta carpeta** y ejecuta:

```bash
cargo install --path . --force
```

Eso es todo. El comando `developing-runner` quedará disponible globalmente.

## Verificar

```bash
developing-runner --version
```

Si ves `developing-runner 0.1.0`, todo funciona correctamente.

## Solución de problemas

| Problema | Solución |
|----------|----------|
| `'cargo' no se reconoce` | Instala Rust desde [rustup.rs](https://rustup.rs/) |
| `'developing-runner' no se reconoce` | Reinicia la terminal o agrega `%USERPROFILE%\.cargo\bin` al PATH |
| `Access is denied` | Cierra cualquier instancia de `developing-runner` que esté ejecutándose |
