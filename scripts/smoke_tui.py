#!/usr/bin/env python3
"""End-to-end smoke test of the waveline TUI in a pseudo-terminal.

Launches the binary with temporary XDG directories (your real config, state
and cache are untouched), drives it with a realistic key sequence and asserts
on the *rendered screen* (via the `pyte` VT100 emulator), on the exit code, on
the terminal being restored, and on the persisted config/state — including a
second launch that must restore queue and history.

Requirements: python3 and `pip install pyte`. No network or audio device is
needed: the one playback attempt is expected to fail cleanly when offline (the
test only checks that the error is reported and the app keeps running).

Usage:
    cargo build --release
    python3 scripts/smoke_tui.py target/release/waveline
"""

import json, os, pty, select, struct, sys, termios, fcntl, time, tempfile
import pyte

BIN = sys.argv[1]
COLS, ROWS = 100, 30
home = tempfile.mkdtemp(prefix="waveline-smoke-")
env = dict(os.environ)
env.update({
    "HOME": home,
    "XDG_CONFIG_HOME": os.path.join(home, ".config"),
    "XDG_DATA_HOME": os.path.join(home, ".local", "share"),
    "XDG_CACHE_HOME": os.path.join(home, ".cache"),
    "TERM": "xterm-256color",
    "DBUS_SESSION_BUS_ADDRESS": "unix:path=/nonexistent",
})

pid, fd = pty.fork()
if pid == 0:
    os.execve(BIN, [BIN], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

screen = pyte.Screen(COLS, ROWS)
stream = pyte.ByteStream(screen)
raw = bytearray()

def pump(t=0.4):
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                data = os.read(fd, 65536)
            except OSError:
                return
            if not data:
                return
            raw.extend(data)
            stream.feed(data)

def send(keys, wait=0.35):
    os.write(fd, keys)
    pump(wait)

def text():
    return "\n".join(screen.display)

def alive():
    return os.waitpid(pid, os.WNOHANG) == (0, 0)

checks = []
def check(name, cond):
    checks.append((name, bool(cond)))
    print(("ok   " if cond else "FAIL ") + name)

pump(1.0)
t = text()
check("première frame : titre, Sources, barre de lecture", "waveline" in t and "Sources" in t and "Nothing playing" in t)
check("démarre sur Search avec les morceaux de démo", "Search  ·  All" in t and "J-POP" in t and "Andy Butler" in t)
check("statut d'accueil", "Welcome" in t)

send(b"?")
t = text()
check("aide affichée, deux colonnes, souris", "Help" in t and "Navigate" in t and "Playback" in t and "Mouse" in t and "add selection to queue" in t)
send(b"q")
check("q ferme l'aide sans quitter", "Help" not in text() and alive())

send(b"j")
send(b"a")
t = text()
check("statut Queued + compteur Queue (1)", "Queued: Andy Butler" in t and "Queue (1)" in t)
send(b"a")
check("compteur Queue (2)", "Queue (2)" in text())

send(b"\t")          # focus sidebar
send(b"G")           # Queue
send(b"\r")          # ouvre
t = text()
check("vue File : titre et 2 entrées", "Queue  ·  All" in t and t.count("Andy Butler") >= 2)
send(b"x")
t = text()
check("x retire une entrée → Queue (1)", "Removed:" in t and "Queue (1)" in t)
send(b"X")
t = text()
check("X vide la file → indication de liste vide", "cleared" in t and "queue is empty" in t and "Queue (" not in t)

send(b"\t"); send(b"k"); send(b"\r")   # History (vide)
check("vue Historique vide avec indication", "History  ·  All" in text() and "nothing played yet" in text())

send(b"\t"); send(b"k"); send(b"\r")   # Search : ouvre l'invite, résultats derrière
check("retour sur Search : invite ouverte, liste intacte", "type your search" in text() and "J-POP" in text())
send(b"\x1b")
check("Esc referme l'invite", "type your search" not in text())

send(b"/")
send(b"four tet live")
check("invite de recherche", "four tet live" in text())
send(b"\x17")        # Ctrl-W
check("Ctrl-W efface le dernier mot", "four tet " in text() and "live" not in text())
send(b"\x15")        # Ctrl-U
check("Ctrl-U vide la ligne", "four" not in text())
send(b"\x1b")
check("Esc annule la saisie", "type your search" not in text())

send(b":")
send(b"https://www.mixcloud.com/NTSRadio/some-show/")
send(b"\r", wait=0.5)
deadline = time.time() + 20
while "⚠" not in text() and time.time() < deadline:
    pump(0.5)
t = text()
check("lecture impossible (réseau bloqué) → erreur propre ⚠, pas de crash", "⚠" in t and alive())

send(b"2")
check("filtre SC : seul le morceau SoundCloud reste", "J-POP" in text() and "Andy Butler" not in text() and "Filter: SoundCloud" in text())
send(b"3")
check("filtre MC", "Andy Butler" in text() and "J-POP" not in text())
send(b"1")
check("filtre Tout", "J-POP" in text() and "Andy Butler" in text())

send(b"L")
check("bascule en français (sidebar + statut)", "Recherche" in text() and "Langue" in text())
send(b"v")
check("visualiseur : miroir", "miroir" in text())
send(b"\x04"); send(b"\x15")
check("^d/^u sans crash", alive())
send(b"G"); send(b"g")
send(b"x")
check("x hors file/historique : indication, liste intacte", "'x' retire" in text() and "J-POP" in text())

send(b"q")
pump(1.0)
try:
    _, status = os.waitpid(pid, 0)
except ChildProcessError:
    status = 0
check("sortie propre (code 0)", os.waitstatus_to_exitcode(status) == 0)
check("écran alternatif quitté", b"\x1b[?1049l" in bytes(raw))
check("capture souris désactivée", b"\x1b[?1000l" in bytes(raw) or b"\x1b[?1006l" in bytes(raw))
check("bracketed paste désactivé", b"\x1b[?2004l" in bytes(raw))

cfg_path = os.path.join(home, ".config", "waveline", "config.json")
st_path = os.path.join(home, ".local", "share", "waveline", "state.json")
cfg = json.load(open(cfg_path)) if os.path.exists(cfg_path) else {}
st = json.load(open(st_path)) if os.path.exists(st_path) else None
check("config persistée : lang=Fr, viz=Mirror, volume=80", cfg.get("lang") == "Fr" and cfg.get("viz") == "Mirror" and cfg.get("volume") == 80)
check("état persisté : file vidée, historique vide", st == {"queue": [], "history": []})
check("aucun .tmp résiduel", not os.path.exists(st_path + ".tmp") and not os.path.exists(cfg_path + ".tmp"))

# Second lancement : l'état est relu (file pré-remplie à la main → compteur).
with open(st_path, "w") as f:
    json.dump({"queue": [{"platform": "Mixcloud", "id": "a/b", "title": "Saved mix", "artist": "Someone", "permalink": "https://www.mixcloud.com/a/b/", "duration_ms": 1000}],
               "history": [{"platform": "SoundCloud", "id": "h", "title": "Played before", "artist": "X", "permalink": "https://soundcloud.com/x/h", "duration_ms": 2000}]}, f)
pid, fd = pty.fork()
if pid == 0:
    os.execve(BIN, [BIN], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
screen = pyte.Screen(COLS, ROWS); stream = pyte.ByteStream(screen); raw = bytearray()
pump(1.0)
t = text()
check("relance : démarre sur l'Historique (reprise), en français, File (1)", "Historique  ·  Tout" in t and "Played before" in t and "File (1)" in t)
send(b"q"); pump(0.5)
try:
    os.waitpid(pid, 0)
except ChildProcessError:
    pass

failed = [n for n, ok in checks if not ok]
print(f"\n{len(checks) - len(failed)}/{len(checks)} checks ok")
if failed:
    print("--- écran ---")
    print(text())
    sys.exit(1)
