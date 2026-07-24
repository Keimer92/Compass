import { execSync } from "child_process";
import { networkInterfaces } from "os";
import { readFileSync, writeFileSync, existsSync, mkdirSync, copyFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const emulatorIp = "10.0.2.2";

const args = process.argv.slice(2);
const mode = args.includes("--device") ? "device" : "emulator";
const customIp = args.find(a => a.startsWith("--ip="))?.split("=")[1];

function getLanIp() {
  const nets = networkInterfaces();
  for (const name of Object.keys(nets)) {
    for (const net of nets[name] || []) {
      if (net.family === "IPv4" && !net.internal && !name.toLowerCase().includes("docker")) {
        return net.address;
      }
    }
  }
  return null;
}

function ensureDir(p) {
  if (!existsSync(p)) mkdirSync(p, { recursive: true });
}

async function main() {
  const hostIp = customIp || (mode === "emulator" ? emulatorIp : getLanIp());

  if (!hostIp) {
    console.error("❌ No se pudo detectar la IP del host.");
    console.error("   Usa --ip=192.168.x.x para especificarla manualmente.");
    process.exit(1);
  }

  console.log(`📍 Dispositivo: ${mode === "emulator" ? "Emulador" : "Físico"}`);
  console.log(`📍 Host IP: ${hostIp}`);
  console.log(`📍 Dev URL: http://${hostIp}:1420`);

  const baseConfig = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf-8"));
  const androidConfig = {
    build: { devUrl: `http://${hostIp}:1420` },
  };

  const tempDir = join(root, "src-tauri", "gen", "android");
  ensureDir(tempDir);
  const tempConfigPath = join(tempDir, "tauri.android.dev.json");
  writeFileSync(tempConfigPath, JSON.stringify(androidConfig, null, 2));

  const env = {
    ...process.env,
    TAURI_DEV_HOST: hostIp,
  };

  try {
    execSync(`npx tauri android dev --config "${tempConfigPath}" --no-dev-server-wait`, {
      cwd: root,
      stdio: "inherit",
      env,
    });
  } catch (e) {
    console.error("❌ Error:", e.message);
    process.exit(1);
  }
}

main();
