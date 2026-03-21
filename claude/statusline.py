#!/usr/bin/env python
# Claude Code statusline: 2-line, 3-column (Model/Acc, 5h/7d usage + elapsed)
import json, sys, os, re, math
from datetime import datetime, timezone

j = None
try:
    j = json.load(sys.stdin)
except Exception:
    pass

def bar(pct, w=6):
    pct = max(0, min(100, pct))
    f = math.floor(pct * w / 100)
    if pct > 0 and f == 0:
        f = 1
    return "\u2593" * f + "\u2591" * (w - f)

def stat(label, pct):
    p = max(0, min(100, pct))
    return f"{label}{bar(p)} {p:3d}%"

claude_dir = os.environ.get("CLAUDE_CONFIG_DIR", os.path.expanduser("~/.claude"))
tail = os.path.basename(claude_dir.rstrip("/\\"))
m = re.search(r"(\d+)$", tail)
acc = int(m.group(1)) if m else 0

LOADING = " " * 6 + "    \u29d7"  #           ⧗

has_rl = j and "rate_limits" in j
usage5h = elapsed5h = usage7d = elapsed7d = 0
def parse_resets_at(ra):
    """resets_at: Unix timestamp (int/float) or ISO string"""
    if isinstance(ra, (int, float)):
        return datetime.fromtimestamp(ra, tz=timezone.utc)
    return datetime.fromisoformat(ra)

if has_rl and (rl := j.get("rate_limits")):
    try:
        if fh := rl.get("five_hour"):
            usage5h = math.floor(fh.get("used_percentage") or 0)
            if ra := fh.get("resets_at"):
                remaining = max(0, (parse_resets_at(ra) - datetime.now(timezone.utc)).total_seconds())
                elapsed5h = math.floor((18000 - remaining) * 100 / 18000)
        if sd := rl.get("seven_day"):
            usage7d = math.floor(sd.get("used_percentage") or 0)
            if ra := sd.get("resets_at"):
                remaining = max(0, (parse_resets_at(ra) - datetime.now(timezone.utc)).total_seconds())
                elapsed7d = math.floor((604800 - remaining) * 100 / 604800)
    except Exception:
        pass

model = j["model"]["display_name"] if j else "?"
cx_pct = j.get("context_window", {}).get("used_percentage") if j else None
cx = stat("Cx", int(cx_pct)) if cx_pct is not None else (stat("Cx", 0) if has_rl else "Cx" + LOADING)
col_l = [f"{model} Acc:{acc}", cx]
col_c = [stat("5h", usage5h) if has_rl else "5h" + LOADING,
         stat("5t", elapsed5h) if has_rl else "5t" + LOADING]
col_r = [stat("7d", usage7d) if has_rl else "7d" + LOADING,
         stat("7t", elapsed7d) if has_rl else "7t" + LOADING]

pad_l = max(len(s) for s in col_l)
pad_c = max(len(s) for s in col_c)
pad_r = max(len(s) for s in col_r)

for i in range(2):
    line = f"{col_l[i]:<{pad_l}}   {col_c[i]:<{pad_c}}   {col_r[i]:<{pad_r}}"
    print(line, end="" if i == 1 else "\n")
