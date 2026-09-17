"""Record a real Claude Code TUI session: raw bytes off a 150x42 pseudo-terminal."""
import fcntl, os, pty, re, select, signal, struct, sys, termios, time

ESCAPES = re.compile(rb"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]")

def plain(data: bytes) -> bytes:
    """The text a person would see, with the escape codes that split words removed."""
    return ESCAPES.sub(b"", data).replace(b"\r", b"").replace(b"\n", b"")

OUT = sys.argv[1]
PROMPT = sys.argv[2]
COLS, ROWS = 150, 42
IDLE_DONE = 8.0      # quiet for this long after the answer started => finished
HARD_STOP = 240.0

pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.environ["COLUMNS"], os.environ["LINES"] = str(COLS), str(ROWS)
    os.execvp("claude", ["claude", PROMPT])

fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
started = time.time()
last = time.time()
trusted = False
seen = b""
total = 0
with open(OUT, "wb") as out:
    while True:
        ready, _, _ = select.select([fd], [], [], 1.0)
        if ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            out.write(chunk)
            out.flush()
            total += len(chunk)
            last = time.time()
            seen += chunk
            if not trusted and b"Yes,Itrustthisfolder" in plain(seen):
                time.sleep(0.5)
                os.write(fd, b"\x1b[B")   # the dialog opens on "No, exit"
                time.sleep(0.3)
                os.write(fd, b"\r")
                trusted = True
        quiet = time.time() - last
        if trusted and quiet > IDLE_DONE and total > 20000:
            break
        if time.time() - started > HARD_STOP:
            break
os.kill(pid, signal.SIGHUP)
time.sleep(0.5)
os.waitpid(pid, os.WNOHANG)
print(f"recorded {total} bytes to {OUT}")
