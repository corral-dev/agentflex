#!/usr/bin/env python3
"""Synthetic coding-agent histories for agentflex acceptance testing.

Writes a fake home directory with realistic histories for Claude, Codex, Pi
and OpenCode, plus the *expected* card values computed from the generator's
own data model (never by running agentflex).

Usage:
    fixtures.py --home DIR --cwd-root DIR [--now EPOCH]

Rules exercised (mirror sesskit-stats engine / src/sesskit/usage.py):
  - Claude: repeated message.id fragments with identical usage collapse to one;
    same id with differing values -> conflicted -> 0; usage without id -> 0;
    cost-state rows ignored; total = input+cache_read+cache_write+output.
  - Codex: call-level `usage` blocks sum as details; `turn_token_usage`
    revisions fold to latest with residual = latest - details (group dropped
    if details > interval); `thread_token_usage` (session scope) never counts;
    effective total is the native `total_tokens` only.
  - Pi: assistant usage counts (inclusive); toolResult usage never counts.
  - OpenCode: message `tokens` JSON counts (input buckets + output+reasoning);
    old messages are filtered at SQL time; session needs time_updated>=start.

Stdlib only. All timestamps derive from --now (default: current time).
"""

from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
from datetime import datetime, timezone

ACTIVE_GAP = 300.0
EVENT_TAIL = 60.0


def rfc3339(ts: float) -> str:
    return datetime.fromtimestamp(ts, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def wline(path: str, obj: dict) -> None:
    with open(path, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(obj, separators=(",", ":")) + "\n")


# ---------- expected-value model ----------

class Model:
    """Independent re-implementation of the card arithmetic from raw facts."""

    def __init__(self) -> None:
        self.sessions: dict[str, dict] = {}  # sid -> {runtime, activity:[ts], tokens:[(ts,val)]}

    def add_session(self, sid: str, runtime: str) -> None:
        self.sessions[sid] = {"runtime": runtime, "activity": [], "tokens": []}

    def event(self, sid: str, ts: float) -> None:
        self.sessions[sid]["activity"].append(ts)

    def credit(self, sid: str, ts: float, val: int) -> None:
        self.sessions[sid]["tokens"].append((ts, val))

    @staticmethod
    def merge(times: list[float]) -> list[tuple[float, float]]:
        # faithful port of stats::merge_intervals
        merged: list[tuple[float, float]] = []
        for t in sorted(times):
            if merged and t - merged[-1][1] <= ACTIVE_GAP:
                merged[-1] = (merged[-1][0], t)
            else:
                merged.append((t, t))
        return [(a, b + EVENT_TAIL) for a, b in merged]

    def expected(self, start: float, end: float) -> dict:
        live = {}
        for sid, s in self.sessions.items():
            act = sorted(t for t in s["activity"] if start <= t <= end)
            if not act:
                continue
            toks = sum(v for (t, v) in s["tokens"] if start <= t <= end)
            live[sid] = (s["runtime"], act, toks)
        by: dict[str, int] = {}
        intervals: list[tuple[float, float]] = []
        tokens = 0
        for sid, (rt, act, toks) in live.items():
            by[rt] = by.get(rt, 0) + 1
            tokens += toks
            intervals.extend(self.merge(act))
        # peak_overlap port: ends before starts at same instant
        pts: list[tuple[float, int]] = []
        for a, b in intervals:
            pts.append((a, 1))
            pts.append((b, -1))
        pts.sort(key=lambda p: (p[0], p[1]))
        cur = peak = 0
        at = None
        for t, d in pts:
            cur += d
            if cur > peak:
                peak, at = cur, t
        by_rt = sorted(by.items(), key=lambda kv: (-kv[1], kv[0]))
        active_s = sum(b - a for a, b in intervals)
        return {"sessions": len(live), "tokens": tokens,
                "by_runtime": dict(by_rt), "peak_parallel": peak,
                "peak_at": at, "heatmap_active_s": active_s}


# ---------- writers ----------

def claude_msg(cwd: str, ts: float, role: str, mid: str | None,
               usage: dict | None, model: str = "claude-opus-4-6",
               text: str = "ok") -> dict:
    msg: dict = {"role": role, "content": text}
    if mid:
        msg["id"] = mid
    msg["model"] = model
    if usage is not None:
        msg["usage"] = usage
    return {"type": role, "timestamp": rfc3339(ts), "cwd": cwd, "message": msg}


def claude_usage(inp: int, out: int, read: int = 0, write: int = 0,
                 think: int | None = None) -> dict:
    u = {"input_tokens": inp, "output_tokens": out,
         "cache_read_input_tokens": read, "cache_creation_input_tokens": write}
    if think is not None:
        u["output_tokens_details"] = {"thinking_tokens": think}
    return u


def codex_meta(cwd: str, ts: float) -> dict:
    return {"type": "session_meta", "timestamp": rfc3339(ts),
            "payload": {"cwd": cwd, "thread_source": "user"}}


def codex_response(ts: float, text: str = "done") -> dict:
    return {"type": "response_item", "timestamp": rfc3339(ts),
            "payload": {"type": "agent_message", "text": text}}


def codex_user(ts: float, text: str = "do the task") -> dict:
    return {"type": "event_msg", "timestamp": rfc3339(ts),
            "payload": {"type": "user_message", "text": text}}


def codex_turn_record(ts: float, turn: str | None, resp: str | None,
                      call: dict | None, snap: dict | None,
                      snap_rev2: dict | None = None,
                      thread: str | None = None, thread_total: int | None = None,
                      model: str = "gpt-5.6") -> list[dict]:
    payload: dict = {"model": model}
    if turn:
        payload["turn_id"] = turn
    if resp:
        payload["response_id"] = resp
    if call is not None:
        payload["usage"] = call
    if snap is not None:
        payload["turn_token_usage"] = snap
    if thread and thread_total is not None:
        payload["thread_id"] = thread
        payload["thread_token_usage"] = {"total_tokens": thread_total,
                                         "input_tokens": thread_total - 100,
                                         "output_tokens": 100}
    rows = [{"type": "token_usage_record", "timestamp": rfc3339(ts),
             "payload": payload}]
    if snap_rev2 is not None:
        p2 = dict(payload)
        p2["turn_token_usage"] = snap_rev2
        rows.append({"type": "token_usage_record", "timestamp": rfc3339(ts),
                     "payload": p2})
    return rows


def pi_header(sid: str, cwd: str, ts: float) -> dict:
    return {"type": "session", "id": sid, "cwd": cwd,
            "timestamp": rfc3339(ts)}


def pi_msg(eid: str, ts: float, role: str, usage: dict | None,
           model: str = "pi-model-x", parent: str | None = None) -> dict:
    msg: dict = {"role": role, "content": "hello"}
    if model:
        msg["model"] = model
    if usage is not None:
        msg["usage"] = usage
    row: dict = {"type": "message", "id": eid, "timestamp": rfc3339(ts),
                 "message": msg}
    if parent is not None:
        row["parentId"] = parent
    return row


def build(home: str, cwd_root: str, now: float) -> dict:
    m = Model()
    day = 86400
    T0 = float(int(now - 2 * day) // 3600 * 3600)  # designed overlap cluster
    projs = {}
    for p in ("proj-alpha", "proj-beta", "proj-gamma", "proj-delta",
              "proj-epsilon"):
        d = os.path.join(cwd_root, p)
        os.makedirs(d, exist_ok=True)
        projs[p] = d

    # ---- Claude ----
    cdir = os.path.join(home, ".claude", "projects", "proj-alpha")
    os.makedirs(cdir, exist_ok=True)

    # cl-a-dedup: one call split in two identical fragments (count once) + normal msg
    sid = "cl-a-dedup"
    m.add_session(sid, "claude")
    p = os.path.join(cdir, sid + ".jsonl")
    if os.path.exists(p):
        os.remove(p)
    u1 = claude_usage(100, 50, 20, 30)
    wline(p, {"type": "user", "timestamp": rfc3339(T0), "cwd": projs["proj-alpha"],
              "message": {"role": "user", "content": "fix the bug"}})
    m.event(sid, T0)
    wline(p, claude_msg(projs["proj-alpha"], T0 + 5, "assistant", "m1", u1))
    m.event(sid, T0 + 5)
    m.credit(sid, T0 + 5, 100 + 20 + 30 + 50)  # fragments collapse -> 200 once
    wline(p, claude_msg(projs["proj-alpha"], T0 + 6, "assistant", "m1", dict(u1)))
    m.event(sid, T0 + 6)
    # (no second credit: duplicate)
    wline(p, {"type": "user", "timestamp": rfc3339(T0 + 3600),
              "cwd": projs["proj-alpha"],
              "message": {"role": "user", "content": "thanks"}})
    m.event(sid, T0 + 3600)
    wline(p, claude_msg(projs["proj-alpha"], T0 + 3605, "assistant", "m2",
                        claude_usage(10, 5)))
    m.event(sid, T0 + 3605)
    m.credit(sid, T0 + 3605, 15)

    # cl-b-conflict: same id differing values -> 0, plus a normal message
    sid = "cl-b-conflict"
    m.add_session(sid, "claude")
    p = os.path.join(cdir, sid + ".jsonl")
    if os.path.exists(p):
        os.remove(p)
    B = now - 5 * day
    wline(p, {"type": "user", "timestamp": rfc3339(B), "cwd": projs["proj-alpha"],
              "message": {"role": "user", "content": "refactor"}})
    m.event(sid, B)
    wline(p, claude_msg(projs["proj-alpha"], B + 5, "assistant", "n1",
                        claude_usage(100, 50)))
    m.event(sid, B + 5)
    wline(p, claude_msg(projs["proj-alpha"], B + 6, "assistant", "n1",
                        claude_usage(200, 50)))
    m.event(sid, B + 6)
    # conflicted -> no credit
    wline(p, claude_msg(projs["proj-alpha"], B + 610, "assistant", "n2",
                        claude_usage(1000, 500)))
    m.event(sid, B + 610)
    m.credit(sid, B + 610, 1500)

    # cl-c-coststate: cost-state row ignored
    sid = "cl-c-coststate"
    m.add_session(sid, "claude")
    p = os.path.join(cdir, sid + ".jsonl")
    if os.path.exists(p):
        os.remove(p)
    C = now - 4 * day
    wline(p, {"type": "user", "timestamp": rfc3339(C), "cwd": projs["proj-alpha"],
              "message": {"role": "user", "content": "add tests"}})
    m.event(sid, C)
    wline(p, claude_msg(projs["proj-alpha"], C + 5, "assistant", "k1",
                        claude_usage(300, 100)))
    m.event(sid, C + 5)
    m.credit(sid, C + 5, 400)
    wline(p, {"type": "cost-state", "timestamp": rfc3339(C + 6),
              "sessionId": sid,
              "modelUsage": {"claude-opus-4-6": {"inputTokens": 99999,
                                                "outputTokens": 99999,
                                                "costUSD": 99.0}}})
    # no credit: engine never sums session rollups

    # cl-old: entirely before the window
    sid = "cl-old"
    m.add_session(sid, "claude")
    p = os.path.join(cdir, sid + ".jsonl")
    if os.path.exists(p):
        os.remove(p)
    O = now - 10 * day
    wline(p, {"type": "user", "timestamp": rfc3339(O), "cwd": projs["proj-alpha"],
              "message": {"role": "user", "content": "old task"}})
    m.event(sid, O)
    wline(p, claude_msg(projs["proj-alpha"], O + 5, "assistant", "o1",
                        claude_usage(5000, 2000)))
    m.event(sid, O + 5)
    m.credit(sid, O + 5, 7000)  # outside window -> pruned

    # ---- Codex ----
    def codex_path(ts: float, uuid: str) -> str:
        dt = datetime.fromtimestamp(ts, timezone.utc)
        d = os.path.join(home, ".codex", "sessions",
                         f"{dt.year:04d}", f"{dt.month:02d}", f"{dt.day:02d}")
        os.makedirs(d, exist_ok=True)
        tag = dt.strftime("%Y-%m-%dT%H-%M-%S")
        return os.path.join(d, f"rollout-{tag}-{uuid}.jsonl")

    U1 = T0 + 30  # inside cl-a tail [T0, T0+66] -> peak 2
    sid = "codex-turn"
    m.add_session(sid, "codex")
    p = codex_path(U1, "11111111-2222-4333-8444-555555555555")
    if os.path.exists(p):
        os.remove(p)
    wline(p, codex_meta(projs["proj-beta"], U1))
    wline(p, codex_user(U1))
    m.event(sid, U1)  # same ts as response below: no interval change
    wline(p, codex_response(U1))
    m.event(sid, U1)
    for row in codex_turn_record(
            U1, "t1", "r1",
            {"input_tokens": 60, "output_tokens": 40, "total_tokens": 100},
            {"input_tokens": 120, "output_tokens": 80, "total_tokens": 200}):
        wline(p, row)
    m.credit(sid, U1, 100)  # detail r1; interval rev1 superseded
    wline(p, codex_response(U1 + 10))
    m.event(sid, U1 + 10)
    for row in codex_turn_record(
            U1 + 10, "t1", "r2",
            {"input_tokens": 90, "output_tokens": 60, "total_tokens": 150},
            {"input_tokens": 180, "output_tokens": 120, "total_tokens": 300}):
        wline(p, row)
    # details 100+150=250, latest interval 300 -> residual 50 at U1+10
    m.credit(sid, U1 + 10, 150 + 50)
    wline(p, codex_response(U1 + 700))
    m.event(sid, U1 + 700)
    for row in codex_turn_record(
            U1 + 700, None, "r3",
            {"input_tokens": 40, "output_tokens": 30, "total_tokens": 70},
            None):
        wline(p, row)
    m.credit(sid, U1 + 700, 70)

    sid = "codex-thread"
    m.add_session(sid, "codex")
    H = now - 3 * day
    # keep clear of the T0 cluster (T0 = now-2d): H = now-3d is 24h away, fine
    p = codex_path(H, "22222222-3333-4444-8555-666666666666")
    if os.path.exists(p):
        os.remove(p)
    wline(p, codex_meta(projs["proj-beta"], H))
    wline(p, codex_user(H))
    m.event(sid, H)
    wline(p, codex_response(H))
    m.event(sid, H)
    for row in codex_turn_record(
            H, None, "r9",
            {"input_tokens": 50, "output_tokens": 30, "total_tokens": 80},
            None, thread="thr-1", thread_total=9999):
        wline(p, row)
    m.credit(sid, H, 80)  # thread_token_usage (session rollup) never counts

    sid = "codex-old"
    m.add_session(sid, "codex")
    p = codex_path(O + 100, "33333333-4444-4555-8666-777777777777")
    if os.path.exists(p):
        os.remove(p)
    wline(p, codex_meta(projs["proj-beta"], O + 100))
    wline(p, codex_user(O + 100))
    m.event(sid, O + 100)
    wline(p, codex_response(O + 100))
    m.event(sid, O + 100)
    for row in codex_turn_record(
            O + 100, None, "ro",
            {"input_tokens": 4000, "output_tokens": 1000,
             "total_tokens": 5000}, None):
        wline(p, row)
    m.credit(sid, O + 100, 5000)  # outside window -> pruned

    # codex-noprompt: in-window activity+usage but no user prompt anywhere ->
    # excluded as empty_history; sessions/tokens must NOT include it.
    sid = "codex-noprompt"
    m.add_session(sid, "codex")
    NP = T0 + 5400
    p = codex_path(NP, "44444444-5555-4666-8777-888888888888")
    if os.path.exists(p):
        os.remove(p)
    wline(p, codex_meta(projs["proj-beta"], NP))
    wline(p, codex_response(NP))
    for row in codex_turn_record(
            NP, None, "rn",
            {"input_tokens": 300, "output_tokens": 200,
             "total_tokens": 500}, None):
        wline(p, row)
    # NOTE: no m.event / m.credit: the engine drops the whole session.

    # ---- Pi ----
    def pi_path(slug: str, ts: float, pid: str) -> str:
        d = os.path.join(home, ".pi", "agent", "sessions", slug)
        os.makedirs(d, exist_ok=True)
        return os.path.join(d, f"{int(ts * 1000)}_{pid}.jsonl")

    sid = "pi-main"
    m.add_session(sid, "pi")
    V = now - 1 * day
    p = pi_path("proj-gamma", V, sid)
    if os.path.exists(p):
        os.remove(p)
    wline(p, pi_header(sid, projs["proj-gamma"], V))
    wline(p, {"type": "message", "id": "e1", "timestamp": rfc3339(V),
              "message": {"role": "user", "content": "hi"}})
    m.event(sid, V)
    wline(p, pi_msg("e2", V + 5, "assistant",
                    {"input": 200, "output": 100, "cacheRead": 10,
                     "cacheWrite": 5, "totalTokens": 315}, parent="e1"))
    m.event(sid, V + 5)
    m.credit(sid, V + 5, 200 + 10 + 5 + 100)
    wline(p, pi_msg("e3", V + 610, "assistant", {"input": 50, "output": 20},
                    parent="e2"))
    m.event(sid, V + 610)
    m.credit(sid, V + 610, 70)
    wline(p, pi_msg("e4", V + 615, "toolResult",
                    {"input": 999, "output": 999}, parent="e3"))
    m.event(sid, V + 615)
    # toolResult usage: unknown scope -> no credit

    sid = "pi-span"  # started before window, continues into it
    m.add_session(sid, "pi")
    p = pi_path("proj-gamma", O + 200, sid)
    if os.path.exists(p):
        os.remove(p)
    wline(p, pi_header(sid, projs["proj-gamma"], O + 200))
    wline(p, {"type": "message", "id": "s1", "timestamp": rfc3339(O + 200),
              "message": {"role": "user", "content": "start big task"}})
    m.event(sid, O + 200)
    wline(p, pi_msg("s2", O + 205, "assistant",
                    {"input": 1000, "output": 1000}, parent="s1"))
    m.event(sid, O + 205)
    m.credit(sid, O + 205, 2000)  # outside window -> pruned
    S2 = T0 + 7200
    wline(p, {"type": "message", "id": "s3", "parentId": "s2",
              "timestamp": rfc3339(S2),
              "message": {"role": "user", "content": "continue"}})
    m.event(sid, S2)
    wline(p, pi_msg("s4", S2 + 5, "assistant",
                    {"input": 40, "output": 10}, parent="s3"))
    m.event(sid, S2 + 5)
    m.credit(sid, S2 + 5, 50)

    # ---- OpenCode (v1 schema) ----
    odb_dir = os.path.join(home, ".local", "share", "opencode")
    os.makedirs(odb_dir, exist_ok=True)
    odb = os.path.join(odb_dir, "opencode.db")
    if os.path.exists(odb):
        os.remove(odb)
    conn = sqlite3.connect(odb)
    conn.execute("CREATE TABLE session (id TEXT, directory TEXT, "
                 "parent_id TEXT, time_archived INTEGER, time_updated INTEGER)")
    conn.execute("CREATE TABLE message (session_id TEXT, id TEXT, "
                 "time_created INTEGER, data TEXT)")
    now_ms = int(now * 1000)

    sid = "oc-1"
    m.add_session(sid, "opencode")
    conn.execute("INSERT INTO session VALUES (?,?,?,?,?)",
                 (sid, projs["proj-delta"], None, None, now_ms))
    G1, G2 = T0 + 10800, T0 + 11400
    conn.execute(
        "INSERT INTO message VALUES (?,?,?,?)",
        (sid, "om1", int(G1 * 1000), json.dumps({
            "id": "om1", "modelID": "opencode-model-a",
            "tokens": {"input": 500, "output": 200, "reasoning": 30,
                       "cache": {"read": 100, "write": 50}}})))
    m.event(sid, G1)
    m.credit(sid, G1, (500 + 100 + 50) + (200 + 30))
    conn.execute(
        "INSERT INTO message VALUES (?,?,?,?)",
        (sid, "om2", int(G2 * 1000), json.dumps({
            "id": "om2", "modelID": "opencode-model-a",
            "tokens": {"input": 70, "output": 30}})))
    m.event(sid, G2)
    m.credit(sid, G2, 100)

    sid = "oc-2"
    m.add_session(sid, "opencode")
    conn.execute("INSERT INTO session VALUES (?,?,?,?,?)",
                 (sid, projs["proj-epsilon"], None, None, now_ms))
    W = now - 3600.0
    conn.execute("INSERT INTO message VALUES (?,?,?,?)",
                 (sid, "om9", int(W * 1000), json.dumps({
                     "id": "om9", "modelID": "opencode-model-b"})))
    m.event(sid, W)
    # model-only message: no countable tokens
    conn.execute("INSERT INTO message VALUES (?,?,?,?)",
                 (sid, "om10", int((W + 5) * 1000), json.dumps({
                     "id": "om10", "modelID": "opencode-model-b",
                     "tokens": {"input": 20, "output": 10}})))
    m.event(sid, W + 5)
    m.credit(sid, W + 5, 30)
    conn.commit()
    conn.close()

    # safety: peak must be exactly the designed overlap
    Provisional = m.expected(now - 7 * day, now)
    assert Provisional["peak_parallel"] == 2, Provisional
    assert Provisional["sessions"] == 9, Provisional
    return m


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--home", required=True)
    ap.add_argument("--cwd-root", required=True)
    ap.add_argument("--now", type=float, default=None)
    ap.add_argument("--window-start", type=float, default=None,
                    help="recompute expected for an explicit window")
    ap.add_argument("--window-end", type=float, default=None)
    args = ap.parse_args()
    now = args.now if args.now is not None else __import__("time").time()
    os.makedirs(args.home, exist_ok=True)
    m = build(args.home, args.cwd_root, now)
    if args.window_start is not None and args.window_end is not None:
        exp = m.expected(args.window_start, args.window_end)
    else:
        # mirror agentflex default: local midnight 7 days back (6 days before
        # today's midnight), computed in this machine's local timezone
        local = datetime.fromtimestamp(now).replace(hour=0, minute=0,
                                                   second=0, microsecond=0)
        exp = m.expected(local.timestamp() - 6 * 86400.0, now)
    exp["now"] = now
    print(json.dumps(exp, indent=2))


if __name__ == "__main__":
    main()
