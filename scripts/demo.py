"""Render the README demo (docs/demo.gif, docs/card.png) from a fictional week.

Writes a synthetic home with Claude Code, Codex, Pi and OpenCode histories in
their native formats (no real history is read), then renders frames with the
real binary (`agentflex --png PATH --frame T`) and encodes a GIF.

    cargo build --release
    python3 scripts/demo.py            # needs Pillow for GIF encoding

Maintainer tool only; the product has no dependency on it.
"""

from __future__ import annotations

import json
import math
import os
import random
import sqlite3
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target", "release", "agentflex")
FPS = 20
REVEAL_END = 3.8
SHIMMER = 0.7


def iso(ts: float) -> str:
    return datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


def day_sessions(rng: random.Random, now: float) -> list[tuple[float, float]]:
    """(start, end) of fictional work sessions over the last 7 days."""
    out = []
    today = now - (now % 86400)
    for d in range(7):
        weekend = d in (1, 2)
        for _ in range(rng.randint(2, 4) if weekend else rng.randint(6, 10)):
            hour = rng.choice([9, 10, 10, 11, 13, 14, 14, 15, 16, 17, 20, 21, 22])
            start = today - d * 86400 + hour * 3600 + rng.randint(0, 3000)
            if start >= now - 600:
                continue
            out.append((start, min(start + rng.randint(900, 5400), now - 60)))
    return out


def write_claude(home: str, cwd: str, sid: str, span: tuple[float, float], rng: random.Random) -> None:
    proj = os.path.join(home, ".claude", "projects", cwd.replace("/", "-"))
    os.makedirs(proj, exist_ok=True)
    rows = []
    t, end = span
    rows.append({"type": "user", "message": {"role": "user", "content": "Refactor the session parser"},
                 "timestamp": iso(t), "cwd": cwd, "sessionId": sid})
    i = 0
    while t < end:
        t += rng.randint(20, 120)
        i += 1
        rows.append({"type": "assistant", "timestamp": iso(t), "cwd": cwd, "sessionId": sid,
                     "message": {"id": f"msg_{sid}_{i}", "role": "assistant", "model": "claude-opus",
                                 "content": [{"type": "text", "text": "Working on it."}],
                                 "usage": {"input_tokens": rng.randint(5, 50),
                                           "cache_read_input_tokens": rng.randint(40_000, 180_000),
                                           "cache_creation_input_tokens": rng.randint(0, 8_000),
                                           "output_tokens": rng.randint(200, 2_500)}}})
    with open(os.path.join(proj, f"{sid}.jsonl"), "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")


def write_codex(home: str, cwd: str, sid: str, span: tuple[float, float], rng: random.Random) -> None:
    t, end = span
    day = datetime.fromtimestamp(t, tz=timezone.utc)
    d = os.path.join(home, ".codex", "sessions", f"{day:%Y}", f"{day:%m}", f"{day:%d}")
    os.makedirs(d, exist_ok=True)
    rows = [{"type": "session_meta", "timestamp": iso(t), "payload": {"id": sid, "cwd": cwd, "thread_source": "user"}},
            {"type": "event_msg", "timestamp": iso(t), "payload": {"type": "user_message", "message": "Add tests"}}]
    i = 0
    while t < end:
        t += rng.randint(30, 150)
        i += 1
        inp = rng.randint(30_000, 150_000)
        out = rng.randint(300, 3_000)
        rows.append({"type": "response_item", "timestamp": iso(t), "payload": {"type": "message", "role": "assistant"}})
        rows.append({"type": "token_usage_record", "timestamp": iso(t),
                     "payload": {"response_id": f"resp_{sid}_{i}", "turn_id": f"turn_{sid}_{i}",
                                 "usage": {"input_tokens": inp, "cached_input_tokens": inp - 2_000,
                                           "output_tokens": out, "total_tokens": inp + out}}})
    name = f"rollout-{day:%Y-%m-%dT%H-%M-%S}-{sid}.jsonl"
    with open(os.path.join(d, name), "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")


def write_pi(home: str, cwd: str, sid: str, span: tuple[float, float], rng: random.Random) -> None:
    d = os.path.join(home, ".pi", "agent", "sessions", "--" + cwd.strip("/").replace("/", "-") + "--")
    os.makedirs(d, exist_ok=True)
    t, end = span
    rows = [{"type": "session", "version": 3, "id": sid, "timestamp": iso(t), "cwd": cwd}]
    parent = None
    i = 0
    while t < end:
        i += 1
        uid, aid = f"{sid}u{i}", f"{sid}a{i}"
        rows.append({"type": "message", "id": uid, "parentId": parent, "timestamp": iso(t),
                     "message": {"role": "user", "content": [{"type": "text", "text": "next step"}]}})
        t += rng.randint(30, 200)
        rows.append({"type": "message", "id": aid, "parentId": uid, "timestamp": iso(t),
                     "message": {"role": "assistant", "model": "gpt-sol", "content": [{"type": "text", "text": "Done."}],
                                 "usage": {"input": rng.randint(500, 3_000), "cacheRead": rng.randint(30_000, 120_000),
                                           "cacheWrite": 0, "output": rng.randint(200, 2_000)}}})
        parent = aid
    with open(os.path.join(d, f"{datetime.fromtimestamp(span[0], tz=timezone.utc):%Y-%m-%dT%H-%M-%S-000Z}_{sid}.jsonl"), "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")


def write_opencode(home: str, sessions: list[tuple[str, str, tuple[float, float]]], rng: random.Random) -> None:
    d = os.path.join(home, ".local", "share", "opencode")
    os.makedirs(d, exist_ok=True)
    db = sqlite3.connect(os.path.join(d, "opencode.db"))
    db.executescript(
        "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT, title TEXT, parent_id TEXT,"
        " time_archived INTEGER, time_created INTEGER, time_updated INTEGER);"
        "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);"
        "CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, data TEXT);")
    for sid, cwd, (t, end) in sessions:
        db.execute("INSERT INTO session VALUES (?,?,?,NULL,NULL,?,?)", (sid, cwd, "demo", int(t * 1000), int(end * 1000)))
        i = 0
        db.execute("INSERT INTO message VALUES (?,?,?,?)",
                   (f"{sid}_u", sid, int(t * 1000), json.dumps({"role": "user"})))
        while t < end:
            t += rng.randint(20, 90)
            i += 1
            data = {"role": "assistant", "modelID": "glm",
                    "tokens": {"input": rng.randint(2_000, 9_000), "output": rng.randint(200, 1_500), "reasoning": 0,
                               "cache": {"read": rng.randint(20_000, 90_000), "write": 0}}}
            db.execute("INSERT INTO message VALUES (?,?,?,?)", (f"{sid}_{i}", sid, int(t * 1000), json.dumps(data)))
    db.commit()
    db.close()


def build_home(root: str, now: float) -> str:
    rng = random.Random(42)
    home = os.path.join(root, "home")
    work = os.path.join(root, "work")
    oc = []
    for n, span in enumerate(day_sessions(rng, now)):
        cwd = os.path.join(work, ["api", "web", "cli", "infra"][n % 4])
        os.makedirs(cwd, exist_ok=True)
        sid = f"{n:08x}-demo-4000-8000-{n:012x}"
        kind = rng.choices(["claude", "codex", "pi", "opencode"], weights=[5, 3, 2, 2])[0]
        if kind == "claude":
            write_claude(home, cwd, sid, span, rng)
        elif kind == "codex":
            write_codex(home, cwd, f"{n:08x}-0000-4000-8000-{n:012x}", span, rng)
        elif kind == "pi":
            write_pi(home, cwd, f"{n:08x}", span, rng)
        else:
            oc.append((f"ses_demo{n:04d}", cwd, span))
    write_opencode(home, oc, rng)
    return home


def render(home: str, out_png: str, frame: float | None) -> None:
    args = [BIN, "--png", out_png] + (["--frame", f"{frame:.3f}"] if frame is not None else [])
    env = {**os.environ, "HOME": home, "USERPROFILE": home}
    for key in ("CLAUDE_CONFIG_DIR", "CODEX_HOME", "XDG_DATA_HOME", "OPENCODE_DB", "OPENCODE_DATA_DIR",
                "PI_CODING_AGENT_DIR", "PI_CODING_AGENT_SESSION_DIR"):
        env.pop(key, None)
    subprocess.run(args, env=env, check=True, stdout=subprocess.DEVNULL)


def main() -> int:
    from PIL import Image

    now = time.time()
    docs = os.path.join(ROOT, "docs")
    os.makedirs(docs, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        home = build_home(tmp, now)
        render(home, os.path.join(docs, "card.png"), None)
        frames = []
        times = [i / FPS for i in range(int(REVEAL_END * FPS) + 1)]
        for k, t in enumerate(times):
            path = os.path.join(tmp, f"f{k:03d}.png")
            render(home, path, t)
            frames.append(Image.open(path).convert("RGB").copy())
        final = Image.open(os.path.join(docs, "card.png")).convert("RGB")
        frames.append(final)
        durations = [1000 // FPS] * (len(frames) - 1) + [3000]
        frames[0].save(os.path.join(docs, "demo.gif"), save_all=True, append_images=frames[1:],
                       duration=durations, loop=0, optimize=True, disposal=1)
    size = os.path.getsize(os.path.join(docs, "demo.gif")) // 1024
    print(f"docs/demo.gif {len(frames)} frames {size} KiB; docs/card.png")
    return 0


if __name__ == "__main__":
    sys.exit(main())
