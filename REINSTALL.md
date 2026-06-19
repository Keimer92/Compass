# Code Runner — Reinstalacion Rapida

Si formateaste tu PC o estas en un equipo nuevo, sigue estos pasos para tener todo listo en segundos.

## Requisito previo

- [Rust](https://rustup.rs/) instalado en el sistema
- Conexion a Internet (solo la primera vez)

## Reinstalar (o actualizar)

Abre una terminal en **esta carpeta** y ejecuta:

```bash
cargo install --path . --force
```

Eso es todo. El comando `code-runner` quedara disponible globalmente.

## Verificar

```bash
code-runner --version
```

Si ves `code-runner 0.1.0`, todo funciona correctamente.

## Solucion de problemas

| Problema | Solucion |
|----------|----------|
| `'cargo' no se reconoce` | Instala Rust desde [rustup.rs](https://rustup.rs/) |
| `'code-runner' no se reconoce` | Reinicia la terminal o agrega `%USERPROFILE%\.cargo\bin` al PATH |
| `Access is denied` | Cierra cualquier instancia de `code-runner` que este ejecutandose |
