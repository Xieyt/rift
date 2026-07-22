#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["pyobjc-framework-Quartz"]
# ///
"""wm-probe — inspect REAL on-screen window geometry from the macOS window server.

Rift is a tiling WM that positions windows with private move APIs. The only source
of truth for where a window *actually* ended up is the window server itself, not
Rift's internal model. This probe reads that ground truth (Quartz CGWindowList) so
you can answer the two questions that matter when a layout looks wrong:

  1. Do managed tiles OVERLAP? (they never should)
  2. Did Rift's INTENDED frame actually land on screen? (--rift cross-check)

Ground truth comes from Quartz. Intent (optional) comes from `rift-cli query windows`,
whose JSON carries `window_server_id` — the exact CGWindowNumber used to join the two.

Examples
    uv run scripts/wm-probe.py                     # every managed-looking window + overlaps
    uv run scripts/wm-probe.py --app Emacs alacritty Firefox
    uv run scripts/wm-probe.py --rift              # diff intent vs reality (auto-find rift-cli)
    uv run scripts/wm-probe.py --rift ./result/bin/rift-cli --json
    just probe --rift                              # via the justfile recipe

Exit status is nonzero when an anomaly is found (overlap, or drift under --rift),
so it doubles as an assertion in scripts and CI.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from dataclasses import dataclass, field

import Quartz


@dataclass
class Win:
    num: int  # kCGWindowNumber == rift's window_server_id
    owner: str
    layer: int
    x: float
    y: float
    w: float
    h: float
    # filled in from `rift-cli query windows` when --rift is used
    intent: tuple[float, float, float, float] | None = None
    floating: bool | None = None
    title: str = ""

    @property
    def x1(self) -> float:
        return self.x + self.w

    @property
    def y1(self) -> float:
        return self.y + self.h

    def overlap_area(self, o: "Win") -> float:
        ox = max(0.0, min(self.x1, o.x1) - max(self.x, o.x))
        oy = max(0.0, min(self.y1, o.y1) - max(self.y, o.y))
        return ox * oy

    def drift(self) -> tuple[float, float, float, float] | None:
        """Per-edge |intended - actual| in px, or None if no intent to compare."""
        if self.intent is None:
            return None
        ix, iy, iw, ih = self.intent
        return (abs(ix - self.x), abs(iy - self.y), abs(iw - self.w), abs(ih - self.h))


def real_windows(min_w: float, apps: list[str] | None) -> list[Win]:
    """On-screen, non-desktop, layer-0 (normal) windows from the window server."""
    raw = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly
        | Quartz.kCGWindowListExcludeDesktopElements,
        Quartz.kCGNullWindowID,
    )
    want = {a.casefold() for a in apps} if apps else None
    out: list[Win] = []
    for w in raw:
        owner = w.get("kCGWindowOwnerName", "") or ""
        layer = int(w.get("kCGWindowLayer", 0) or 0)
        b = w.get("kCGWindowBounds", {}) or {}
        width = float(b.get("Width", 0) or 0)
        if layer != 0 or width < min_w:
            continue
        if want is not None and owner.casefold() not in want:
            continue
        out.append(
            Win(
                num=int(w.get("kCGWindowNumber", 0) or 0),
                owner=owner,
                layer=layer,
                x=float(b.get("X", 0) or 0),
                y=float(b.get("Y", 0) or 0),
                w=width,
                h=float(b.get("Height", 0) or 0),
                title=w.get("kCGWindowName", "") or "",
            )
        )
    out.sort(key=lambda v: (round(v.y), round(v.x)))
    return out


def annotate_with_rift(wins: list[Win], rift_cli: str) -> str | None:
    """Attach Rift's intended frame/floating flag by joining on window_server_id.

    Returns an error string on failure, else None. Windows Rift doesn't manage
    keep intent=None and are excluded from the drift report.
    """
    try:
        proc = subprocess.run(
            [rift_cli, "query", "windows"],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as e:
        return f"failed to run {rift_cli!r}: {e}"
    if proc.returncode != 0:
        return f"{rift_cli} query windows exited {proc.returncode}: {proc.stderr.strip()}"
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        return f"could not parse rift-cli JSON: {e}"

    by_num = {w.num: w for w in wins}
    for item in data:
        wsid = item.get("window_server_id")
        w = by_num.get(wsid)
        if w is None:
            continue
        fr = item.get("frame") or {}
        origin = fr.get("origin") or {}
        size = fr.get("size") or {}
        w.intent = (
            float(origin.get("x", 0)),
            float(origin.get("y", 0)),
            float(size.get("width", 0)),
            float(size.get("height", 0)),
        )
        w.floating = bool(item.get("is_floating", False))
        if item.get("title"):
            w.title = item["title"]
    return None


def find_overlaps(wins: list[Win], min_px: float, managed_only: bool) -> list[tuple[Win, Win, float]]:
    pool = [w for w in wins if not (managed_only and w.floating)]
    hits = []
    for i in range(len(pool)):
        for j in range(i + 1, len(pool)):
            a, b = pool[i], pool[j]
            area = a.overlap_area(b)
            if area > min_px:
                hits.append((a, b, area))
    hits.sort(key=lambda t: t[2], reverse=True)
    return hits


def to_json(wins: list[Win], overlaps, drifts, tol: float) -> dict:
    return {
        "windows": [
            {
                "num": w.num,
                "owner": w.owner,
                "title": w.title,
                "actual": {"x": w.x, "y": w.y, "w": w.w, "h": w.h},
                "intent": (
                    None
                    if w.intent is None
                    else {"x": w.intent[0], "y": w.intent[1], "w": w.intent[2], "h": w.intent[3]}
                ),
                "floating": w.floating,
            }
            for w in wins
        ],
        "overlaps": [
            {"a": a.num, "b": b.num, "a_owner": a.owner, "b_owner": b.owner, "area_px": round(area)}
            for a, b, area in overlaps
        ],
        "drift_tolerance_px": tol,
        "drift": [
            {
                "num": w.num,
                "owner": w.owner,
                "intent": {"x": w.intent[0], "y": w.intent[1], "w": w.intent[2], "h": w.intent[3]},
                "actual": {"x": w.x, "y": w.y, "w": w.w, "h": w.h},
                "delta": {"x": d[0], "y": d[1], "w": d[2], "h": d[3]},
            }
            for w, d in drifts
        ],
    }


def print_table(wins: list[Win]) -> None:
    print("REAL on-screen bounds (window server):")
    if not wins:
        print("  (no matching windows)")
        return
    for w in wins:
        line = (
            f"  {w.owner:<12.12} num={w.num:<6} "
            f"x=[{w.x:.0f}..{w.x1:.0f}] w={w.w:.0f} y=[{w.y:.0f}..{w.y1:.0f}] h={w.h:.0f}"
        )
        if w.floating is not None:
            line += "  [floating]" if w.floating else "  [tiled]"
        print(line)


def main() -> int:
    ap = argparse.ArgumentParser(
        prog="wm-probe",
        description="Inspect real on-screen window geometry vs Rift's intended layout.",
    )
    ap.add_argument(
        "--app",
        nargs="+",
        metavar="NAME",
        help="only include these owner apps (case-insensitive). Default: all normal windows.",
    )
    ap.add_argument(
        "--min-width", type=float, default=200.0, help="ignore windows narrower than this (px)."
    )
    ap.add_argument(
        "--overlap-px",
        type=float,
        default=25.0,
        help="report overlaps whose intersection area exceeds this (px^2).",
    )
    ap.add_argument(
        "--rift",
        nargs="?",
        const="",
        metavar="RIFT_CLI",
        help="cross-check against `rift-cli query windows`. Optionally pass the rift-cli path; "
        "otherwise ./result/bin/rift-cli then $PATH are tried.",
    )
    ap.add_argument(
        "--tolerance",
        type=float,
        default=2.0,
        help="drift threshold: report windows whose actual edge differs from intent by more (px).",
    )
    ap.add_argument("--json", action="store_true", help="emit machine-readable JSON.")
    args = ap.parse_args()

    wins = real_windows(args.min_width, args.app)

    using_rift = args.rift is not None
    if using_rift:
        rift_cli = args.rift or shutil.which("rift-cli") or "./result/bin/rift-cli"
        err = annotate_with_rift(wins, rift_cli)
        if err:
            print(f"warning: --rift cross-check skipped: {err}", file=sys.stderr)
            using_rift = False

    # Overlaps: restrict to Rift-managed tiled windows when we know which those are.
    overlaps = find_overlaps(wins, args.overlap_px, managed_only=using_rift)

    drifts: list[tuple[Win, tuple[float, float, float, float]]] = []
    if using_rift:
        for w in wins:
            if w.floating:
                continue
            d = w.drift()
            if d and max(d) > args.tolerance:
                drifts.append((w, d))

    if args.json:
        print(json.dumps(to_json(wins, overlaps, drifts, args.tolerance), indent=2))
    else:
        print_table(wins)
        print()
        if using_rift:
            print(f"DRIFT (intended vs actual, tolerance {args.tolerance:.0f}px):")
            if not drifts:
                print("  none — every managed window landed where Rift intended.")
            for w, d in drifts:
                ix, iy, iw, ih = w.intent
                print(
                    f"  {w.owner:<12.12} num={w.num:<6} "
                    f"intent x={ix:.0f} y={iy:.0f} w={iw:.0f} h={ih:.0f} | "
                    f"actual x={w.x:.0f} y={w.y:.0f} w={w.w:.0f} h={w.h:.0f} | "
                    f"Δx={d[0]:.0f} Δy={d[1]:.0f} Δw={d[2]:.0f} Δh={d[3]:.0f}"
                )
            print()
        scope = "managed tiles" if using_rift else "windows"
        print(f"OVERLAPS among {scope} (>{args.overlap_px:.0f}px^2):")
        if not overlaps:
            print("  none.")
        for a, b, area in overlaps:
            ox = max(0.0, min(a.x1, b.x1) - max(a.x, b.x))
            oy = max(0.0, min(a.y1, b.y1) - max(a.y, b.y))
            print(
                f"  {a.owner}({a.num}) <> {b.owner}({b.num}): "
                f"{ox:.0f}x{oy:.0f}px = {area:.0f}px^2"
            )

    return 1 if (overlaps or drifts) else 0


if __name__ == "__main__":
    sys.exit(main())
