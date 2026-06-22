#!/usr/bin/env node
"use strict";

// Launcher npm de waveline.
//
// waveline est un binaire Rust (Linux uniquement : sortie audio PipeWire/ALSA).
// Ce script télécharge — à la première exécution — le binaire correspondant à
// la plateforme depuis les Releases GitHub, le met en cache, puis le lance en
// transmettant le terminal (la TUI a besoin d'un vrai tty).

const { spawnSync, execFileSync } = require("child_process");
const https = require("https");
const fs = require("fs");
const os = require("os");
const path = require("path");

const pkg = require("../package.json");
const VERSION = pkg.version;
const REPO = "d101c/waveline";

function target() {
  if (process.platform !== "linux") return null;
  if (process.arch === "x64") return "x86_64-unknown-linux-musl";
  if (process.arch === "arm64") return "aarch64-unknown-linux-musl";
  return null;
}

function cacheDir() {
  const base = process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache");
  return path.join(base, "waveline", "bin-" + VERSION);
}

function download(url, dest, cb, redirects) {
  redirects = redirects || 0;
  https
    .get(url, { headers: { "User-Agent": "waveline-npm" } }, (res) => {
      if (
        [301, 302, 307, 308].includes(res.statusCode) &&
        res.headers.location &&
        redirects < 5
      ) {
        res.resume();
        return download(res.headers.location, dest, cb, redirects + 1);
      }
      if (res.statusCode !== 200) {
        res.resume();
        return cb(new Error("HTTP " + res.statusCode));
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => file.close(() => cb(null)));
      file.on("error", cb);
    })
    .on("error", cb);
}

function ensureBinary(cb) {
  const t = target();
  if (!t) {
    console.error(
      "waveline ne fonctionne que sous Linux (x86_64 ou arm64) — la sortie\n" +
        "audio repose sur PipeWire (pw-play) ou ALSA (aplay).",
    );
    process.exit(1);
  }
  const dir = cacheDir();
  const bin = path.join(dir, "waveline");
  if (fs.existsSync(bin)) return cb(bin);

  fs.mkdirSync(dir, { recursive: true });
  const asset = `waveline-${t}.tar.gz`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${asset}`;
  const tmp = path.join(dir, asset);
  process.stderr.write(`waveline : téléchargement du binaire (${t})…\n`);

  download(url, tmp, (err) => {
    if (err) {
      console.error("Échec du téléchargement :", err.message);
      console.error("URL :", url);
      process.exit(1);
    }
    try {
      // tar et gzip sont toujours présents sous Linux (cible unique de waveline).
      execFileSync("tar", ["-xzf", tmp, "-C", dir]);
      fs.chmodSync(bin, 0o755);
      fs.unlinkSync(tmp);
    } catch (e) {
      console.error("Extraction échouée :", e.message);
      process.exit(1);
    }
    cb(bin);
  });
}

ensureBinary((bin) => {
  const res = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
  if (res.error) {
    console.error("Lancement impossible :", res.error.message);
    process.exit(1);
  }
  process.exit(res.status === null ? 1 : res.status);
});
