import { Plugin } from "@opencode/plugin/tui"
import { readFileSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"

const PROVIDER = "inferx"
const ENV_KEY = "INFERX_API_KEY"
const PLACEHOLDER = "PEGA_TU_API_AQUI"

/**
 * Archivos donde puede vivir el proveedor InferX. Se revisan en orden:
 * primero el global de OpenCode y después el del proyecto abierto.
 */
function configCandidates(directory?: string): string[] {
  const out: string[] = []
  const explicit = process.env.OPENCODE_CONFIG
  if (explicit) out.push(explicit)

  const base = process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config")
  out.push(join(base, "opencode", "opencode.jsonc"))
  out.push(join(base, "opencode", "opencode.json"))

  if (directory) {
    out.push(join(directory, "opencode.jsonc"))
    out.push(join(directory, "opencode.json"))
    out.push(join(directory, ".opencode", "opencode.jsonc"))
    out.push(join(directory, ".opencode", "opencode.json"))
  }
  return out
}

/** Quita comentarios `//` y `/* *\/` y comas finales, respetando las cadenas. */
function stripJsonc(input: string): string {
  let withoutComments = ""
  let inString = false
  let escape = false
  const chars = Array.from(input)

  for (let i = 0; i < chars.length; i++) {
    const c = chars[i]

    if (inString) {
      withoutComments += c
      if (escape) escape = false
      else if (c === "\\") escape = true
      else if (c === '"') inString = false
      continue
    }

    if (c === '"') {
      inString = true
      withoutComments += c
      continue
    }

    if (c === "/" && chars[i + 1] === "/") {
      while (i < chars.length && chars[i] !== "\n") i++
      if (i < chars.length) withoutComments += "\n"
      continue
    }

    if (c === "/" && chars[i + 1] === "*") {
      i += 2
      while (i + 1 < chars.length && !(chars[i] === "*" && chars[i + 1] === "/")) i++
      i += 1
      continue
    }

    withoutComments += c
  }

  let result = ""
  inString = false
  escape = false
  const clean = Array.from(withoutComments)

  for (let i = 0; i < clean.length; i++) {
    const c = clean[i]

    if (inString) {
      result += c
      if (escape) escape = false
      else if (c === "\\") escape = true
      else if (c === '"') inString = false
      continue
    }

    if (c === '"') {
      inString = true
      result += c
      continue
    }

    if (c === ",") {
      let j = i + 1
      while (j < clean.length && /\s/.test(clean[j])) j++
      if (j < clean.length && (clean[j] === "}" || clean[j] === "]")) continue
    }

    result += c
  }

  return result
}

/** ¿Hay una API key de InferX utilizable? */
function configured(directory?: string): boolean {
  const fromEnv = process.env[ENV_KEY]
  if (fromEnv && fromEnv.trim().length > 0) return true

  for (const file of configCandidates(directory)) {
    let text: string
    try {
      text = readFileSync(file, "utf8")
    } catch {
      continue
    }

    let config: any
    try {
      config = JSON.parse(stripJsonc(text))
    } catch {
      continue
    }

    const provider = config?.providers?.[PROVIDER]
    if (!provider) continue

    const key = provider?.settings?.apiKey
    if (typeof key === "string") {
      const value = key.trim()
      if (value && value !== PLACEHOLDER) {
        const match = value.match(/^\{env:([A-Za-z_][A-Za-z0-9_]*)\}$/)
        if (!match) return true
        if (process.env[match[1]]) return true
      }
    }

    const envs = Array.isArray(provider?.env) ? provider.env : []
    for (const name of envs) {
      if (typeof name === "string" && process.env[name]) return true
    }
  }

  return false
}

export default Plugin.define({
  id: "inferx.reminder",
  setup(context) {
    const directory = context.location?.directory
    const [settings, updateSettings] = context.storage.store("inferx-reminder", {
      initial: { dismissed: false },
    })
    const [state, updateState] = context.storage.memory("inferx-reminder-state", {
      initial: { missing: !configured(directory) },
    })

    const visible = () => state.missing && !settings.dismissed

    async function review() {
      if (configured(directory)) {
        updateState((draft) => {
          draft.missing = false
        })
        context.ui.toast.show({
          title: "InferX",
          message: "La API key de InferX ya está configurada.",
          variant: "success",
          duration: 4000,
        })
        return
      }

      const confirmed = await context.ui.dialog.confirm({
        title: "InferX",
        message: [
          "OpenCode tiene el proveedor InferX, pero no detecto su API key.",
          `Variable: ${ENV_KEY}`,
          "Endpoint: https://model.inferx.net/endpoints/v1",
          "",
          "Puedes configurarla con `compass --task opencode-configure`",
          "o editando ~/.config/opencode/opencode.jsonc.",
          "",
          "¿Ya la has configurado?",
        ].join("\n"),
        label: { confirm: "Ya la configuré", cancel: "Recordar luego" },
      })
      if (!confirmed) return

      if (!configured(directory)) {
        updateState((draft) => {
          draft.missing = true
        })
        context.ui.toast.show({
          title: "InferX",
          message: `Todavía no detecto ${ENV_KEY}. Configúrala y vuelve a intentarlo.`,
          variant: "warning",
          duration: 8000,
        })
        return
      }

      await updateSettings((draft) => {
        draft.dismissed = true
      })
      updateState((draft) => {
        draft.missing = false
      })
      context.ui.toast.show({
        title: "InferX",
        message: "API key detectada. Recordatorio descartado.",
        variant: "success",
        duration: 4000,
      })
    }

    const unregisterSlot = context.ui.slot({
      append: "home.footer.status",
      render: () =>
        visible() ? (
          <box flexDirection="row" onMouseDown={review}>
            <text fg="yellow">{"\u26a0 InferX: falta la API"}</text>
            <text fg="gray">{"  [configurar]"}</text>
          </box>
        ) : null,
    })

    // Alternativa de teclado al clic: `/inferx` o el comando en la paleta.
    const unregisterKeymap = context.keymap.layer(() => ({
      mode: "global",
      commands: [
        {
          id: "inferx.check",
          title: "InferX: revisar la API key",
          group: "InferX",
          palette: true,
          slash: { name: "inferx" },
          run: review,
        },
      ],
    }))

    // OpenCode no expone un evento de cambio de configuración: se relee el
    // archivo cada pocos segundos para retirar el aviso en cuanto aparezca la
    // clave, sin reiniciar la TUI.
    const timer = setInterval(() => {
      const missing = !configured(directory)
      if (missing !== state.missing) {
        updateState((draft) => {
          draft.missing = missing
        })
      }
      // Si la clave ya está, el descarte deja de aplicar: así, si algún día se
      // borra, el recordatorio vuelve a aparecer.
      if (!missing && settings.dismissed) {
        void updateSettings((draft) => {
          draft.dismissed = false
        })
      }
    }, 10_000)

    if (visible()) {
      context.ui.toast.show({
        title: "InferX",
        message: `Falta la API key de InferX (${ENV_KEY}). Haz clic en el aviso del pie o ejecuta /inferx.`,
        variant: "warning",
        duration: 9000,
      })
    }

    return () => {
      unregisterSlot()
      unregisterKeymap()
      clearInterval(timer)
    }
  },
})
