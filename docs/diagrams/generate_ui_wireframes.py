#!/usr/bin/env python3
"""Generate structural UI wireframes for the visual-overhaul plan.

Plain SVG from Python; no third-party dependencies. These are layout and
hierarchy studies, not final art.
"""

from pathlib import Path
from xml.sax.saxutils import escape

OUT = Path(__file__).resolve().parent

INK = "#0A0E12"
PANEL = "#111820"
PANEL2 = "#161F29"
LINE = "#26323D"
TEXT = "#E8EDF2"
MUTED = "#93A1AE"
FAINT = "#5F6C78"
OLD = "#F0A830"
OLD_DIM = "#B97A22"
BETA = "#59D8E6"
MONEY = "#E6B85C"
DANGER = "#E05A5F"
SUCCESS = "#62C98A"
VEIL = "#A78BFA"
PAPER = "#E8E1D0"
FONT = "DejaVu Sans, Inter, ui-sans-serif, sans-serif"
MONO = "DejaVu Sans Mono, ui-monospace, monospace"


def esc(s: object) -> str:
    return escape(str(s), {"'": "&apos;", '"': "&quot;"})


def text(x, y, s, size=13, weight=400, fill=TEXT, anchor="start", family=FONT, opacity=1.0):
    op = f' opacity="{opacity}"' if opacity != 1.0 else ""
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" text-anchor="{anchor}" '
        f'font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}"{op}>'
        f"{esc(s)}</text>"
    )


def lines(x, y, items, size=12, weight=400, fill=MUTED, anchor="start", family=FONT, lh=None):
    lh = lh or size * 1.35
    parts = []
    for i, s in enumerate(items):
        dy = 0 if i == 0 else lh
        parts.append(f'<tspan x="{x:.1f}" dy="{dy:.1f}">{esc(s)}</tspan>')
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" text-anchor="{anchor}" '
        f'font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}">'
        + "".join(parts)
        + "</text>"
    )


def rect(x, y, w, h, fill=PANEL, stroke=LINE, rx=8, dash=False, width=1.0, opacity=1.0):
    da = ' stroke-dasharray="6 4"' if dash else ""
    return (
        f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}" '
        f'fill="{fill}" stroke="{stroke}" stroke-width="{width}"{da} opacity="{opacity}"/>'
    )


def line(x1, y1, x2, y2, color=LINE, width=1.0, dash=False):
    da = ' stroke-dasharray="4 4"' if dash else ""
    return (
        f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
        f'stroke="{color}" stroke-width="{width}"{da}/>'
    )


def button(x, y, w, h, label, fill=PANEL2, stroke=LINE, color=TEXT, size=12, weight=600):
    return "\n".join(
        [
            rect(x, y, w, h, fill, stroke, rx=5, width=1.0),
            text(x + w / 2, y + h / 2 + size * 0.35, label, size=size, weight=weight, fill=color, anchor="middle"),
        ]
    )


def chip(x, y, w, label, color=BETA, fill=PANEL2, size=11):
    return "\n".join(
        [
            rect(x, y, w, 22, fill, color, rx=11, width=1.0),
            text(x + w / 2, y + 15, label, size=size, weight=600, fill=color, anchor="middle"),
        ]
    )


def bar(x, y, w, h, frac, color, label, value):
    out = [
        rect(x, y, w, h, "#0D141A", LINE, rx=4, width=1.0),
        rect(x, y, max(2, w * frac), h, color, color, rx=4, width=0.0),
        text(x, y - 7, label, size=10, weight=600, fill=MUTED),
        text(x + w, y - 7, value, size=10, weight=600, fill=color, anchor="end"),
    ]
    return "\n".join(out)


def frame(x, y, w, h, title, hint=""):
    out = [rect(x, y, w, h, PANEL, LINE, rx=10)]
    out.append(text(x + 14, y + 24, title.upper(), size=10, weight=800, fill=MUTED))
    if hint:
        out.append(text(x + w - 14, y + 24, hint, size=10, weight=500, fill=FAINT, anchor="end"))
    return "\n".join(out)


def top_bar(w=1600, active=0, location="S(0,0) · Street"):
    out = [
        rect(0, 0, w, 54, "#0D141A", LINE, rx=0, width=0.0),
        text(18, 34, "FIDOM", size=20, weight=800, fill=PAPER),
        text(108, 33, "CIVIC NETWORK TERMINAL", size=9, weight=700, fill=OLD),
        text(286, 33, location, size=12, weight=500, fill=MUTED),
        text(500, 33, "Day 1 · 10:42", size=12, weight=500, fill=MUTED, family=MONO),
        text(680, 33, "$12.40", size=13, weight=700, fill=MONEY, family=MONO),
        text(782, 33, "◈ 2.4", size=12, weight=700, fill=DANGER),
        text(860, 33, "✉ 3", size=12, weight=600, fill=BETA),
    ]
    tabs = [("1", "STREET"), ("2", "DECK"), ("3", "NET"), ("4", "VEIL")]
    x = 1040
    for i, (key, name) in enumerate(tabs):
        col = OLD if i == active else MUTED
        out.append(text(x, 32, key, size=10, weight=700, fill=FAINT, family=MONO))
        out.append(text(x + 16, 32, name, size=11, weight=700, fill=col))
        if i == active:
            out.append(line(x, 47, x + 66, 47, OLD, 2))
        x += 96
    return "\n".join(out)


def street_wireframe():
    w, h = 1600, 1000
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-labelledby="title desc">',
        "<title id=\"title\">Street UI wireframe</title>",
        "<desc id=\"desc\">Four-zone shell: top bar, vitals rail, map viewport, inspector, and event ticker.</desc>",
        f'<rect width="{w}" height="{h}" fill="{INK}"/>',
        top_bar(w),
    ]

    # Left rail
    out.append(frame(16, 70, 264, 760, "Vitals / obligations", "body"))
    out.append(text(30, 106, "SURVIVAL", size=10, weight=800, fill=DANGER))
    out.append(bar(30, 132, 220, 12, 0.7, SUCCESS, "HP", "14 / 20"))
    out.append(bar(30, 176, 220, 12, 0.58, OLD, "FOOD", "58"))
    out.append(bar(30, 220, 220, 12, 0.64, BETA, "MOOD", "64"))
    out.append(bar(30, 264, 220, 12, 0.62, VEIL, "MINDPOWER", "8"))
    out.append(text(30, 300, "! Trace 4/6 · warden active", size=11, weight=600, fill=DANGER))

    out.append(text(30, 338, "ACTIVE CONTRACT", size=10, weight=800, fill=OLD))
    out.append(rect(30, 350, 220, 88, PANEL2, OLD_DIM, rx=7, width=1.0))
    out.append(text(42, 374, "Clinic medkit run", size=13, weight=700, fill=TEXT))
    out.append(text(42, 394, "Deliver 2x medkit", size=11, fill=MUTED))
    out.append(text(42, 412, "1h 48m left · $2.20", size=11, fill=MONEY, family=MONO))
    out.append(bar(42, 422, 196, 6, 0.62, OLD, "", ""))

    out.append(text(30, 474, "ECONOMY", size=10, weight=800, fill=MONEY))
    out.append(chip(30, 486, 74, "$ 12.40", MONEY))
    out.append(chip(112, 486, 86, "BANK 5.00", MUTED))
    out.append(chip(30, 516, 74, "2 LOOP", SUCCESS))
    out.append(chip(112, 516, 74, "4 DATA", BETA))
    out.append(chip(194, 516, 54, "0 ALGO", VEIL))

    out.append(text(30, 570, "QUICK ACTIONS", size=10, weight=800, fill=MUTED))
    for i, label in enumerate(["Eat", "Watch", "Sleep"]):
        out.append(button(30, 582 + i * 34, 220, 26, label))
    out.append(text(30, 702, "License —  ·  clan none", size=10, fill=FAINT))
    out.append(text(30, 722, "standing Clean · alignment HonestActive", size=10, fill=FAINT))
    out.append(button(30, 750, 220, 28, "Expand details", fill=PANEL, stroke=LINE, color=MUTED))

    # Center viewport
    out.append(frame(296, 70, 940, 760, "Street — section (0,0)", "click / hjkl"))
    out.append(button(312, 88, 64, 26, "Move", OLD, OLD, INK, 11))
    out.append(button(384, 88, 64, 26, "Talk"))
    out.append(button(456, 88, 70, 26, "Trade"))
    out.append(button(534, 88, 72, 26, "Attack"))
    out.append(button(614, 88, 82, 26, "Pick up"))
    out.append(button(704, 88, 82, 26, "Interact"))
    out.append(text(806, 106, "Hover a tile for details · click to select / path", size=11, fill=FAINT))

    # Map: 16x16 at 38px = 608
    mx, my, ts = 316, 140, 38
    for y in range(16):
        for x in range(16):
            # border walls, cross streets, building blocks
            wall = x in (0, 15) or y in (0, 15) or (x in (5, 6, 7) and 2 <= y <= 6) or (x in (9, 10, 11) and 9 <= y <= 13)
            road = y in (7, 8) or x in (7, 8)
            if wall:
                fill = "#1B242C"
            elif road:
                fill = "#202A33"
            else:
                fill = "#141C23" if (x + y) % 3 else "#18212A"
            out.append(rect(mx + x * ts, my + y * ts, ts - 2, ts - 2, fill, "#1E2933", rx=3, width=0.8))
    # doors / shops / NPCs / player
    out.append(rect(mx + 10 * ts, my + 8 * ts, ts - 2, ts - 2, "#2A2114", OLD_DIM, rx=3, width=1.2))
    out.append(text(mx + 10 * ts + ts / 2, my + 8 * ts + 24, "$", size=16, weight=800, fill=OLD, anchor="middle"))
    out.append(rect(mx + 4 * ts, my + 4 * ts, ts - 2, ts - 2, "#1D2A22", SUCCESS, rx=3, width=1.0))
    out.append(text(mx + 4 * ts + ts / 2, my + 4 * ts + 24, "n", size=14, weight=800, fill=SUCCESS, anchor="middle"))
    out.append(rect(mx + 12 * ts, my + 7 * ts, ts - 2, ts - 2, "#231C2A", VEIL, rx=3, width=1.0))
    out.append(text(mx + 12 * ts + ts / 2, my + 7 * ts + 24, "n", size=14, weight=800, fill=VEIL, anchor="middle"))
    # player + path + selection
    for px, py in [(8, 8), (8, 7), (8, 6), (7, 6)]:
        out.append(rect(mx + px * ts + 8, my + py * ts + 8, ts - 18, ts - 18, "#2B3540", BETA, rx=12, width=0.8))
    out.append(rect(mx + 8 * ts, my + 8 * ts, ts - 2, ts - 2, "#18313A", BETA, rx=3, width=1.6))
    out.append(text(mx + 8 * ts + ts / 2, my + 8 * ts + 25, "▲", size=16, weight=800, fill=BETA, anchor="middle"))
    out.append(rect(mx + 10 * ts, my + 8 * ts, ts - 2, ts - 2, "none", OLD, rx=3, width=2.0))
    # tooltip
    out.append(rect(mx + 10 * ts - 120, my + 8 * ts - 84, 210, 66, "#0D141A", OLD, rx=7, width=1.0))
    out.append(text(mx + 10 * ts - 108, my + 8 * ts - 62, "Shopkeeper · counter", size=12, weight=700, fill=TEXT))
    out.append(text(mx + 10 * ts - 108, my + 8 * ts - 44, "Trade, ask for work", size=11, fill=MUTED))
    out.append(text(mx + 10 * ts - 108, my + 8 * ts - 27, "Click to inspect", size=10, fill=OLD, family=MONO))
    # legend panel
    out.append(rect(976, 140, 240, 608, PANEL2, LINE, rx=8))
    out.append(text(990, 166, "SECTION", size=10, weight=800, fill=MUTED))
    out.append(text(990, 188, "Residential / shop", size=12, weight=600, fill=TEXT))
    out.append(text(990, 206, "16x16 · exits W/E", size=11, fill=FAINT))
    out.append(line(990, 222, 1202, 222))
    for i, (glyph, color, label) in enumerate(
        [("▲", BETA, "you"), ("n", SUCCESS, "NPC"), ("$", OLD, "counter"), ("·", BETA, "path"), ("#", FAINT, "blocked")]
    ):
        y = 246 + i * 28
        out.append(text(990, y, glyph, size=15, weight=800, fill=color, family=MONO))
        out.append(text(1016, y, label, size=11, fill=MUTED))
    out.append(text(990, 420, "NEARBY", size=10, weight=800, fill=MUTED))
    out.append(text(990, 442, "Shopkeeper (2 tiles)", size=11, fill=TEXT))
    out.append(text(990, 460, "Office worker (4)", size=11, fill=TEXT))
    out.append(text(990, 478, "Medkit on floor", size=11, fill=SUCCESS))
    out.append(button(990, 694, 210, 28, "Open city map", fill=PANEL, stroke=LINE, color=MUTED))

    # Right inspector
    out.append(frame(1252, 70, 332, 760, "Inspector", "selection"))
    out.append(text(1266, 108, "SHOPKEEPER", size=10, weight=800, fill=OLD))
    out.append(text(1266, 134, "Mara · counter", size=20, weight=700, fill=TEXT))
    out.append(text(1266, 156, "Residential shop · trust +1", size=12, fill=MUTED))
    out.append(line(1266, 174, 1570, 174))
    out.append(text(1266, 200, "Trade", size=12, weight=700, fill=TEXT))
    out.append(text(1266, 220, "ration 100c · medkit 250c", size=11, fill=MUTED))
    out.append(button(1266, 236, 290, 30, "Open counter", OLD, OLD, INK, 12))
    out.append(text(1266, 290, "CONTRACT", size=10, weight=800, fill=MUTED))
    out.append(rect(1266, 302, 290, 92, PANEL2, LINE, rx=7))
    out.append(text(1278, 326, "Clinic medkit run", size=13, weight=700, fill=TEXT))
    out.append(text(1278, 346, "2x medkit · $2.20", size=11, fill=MONEY))
    out.append(text(1278, 364, "1h 48m left · legal", size=11, fill=MUTED))
    out.append(button(1278, 372, 130, 22, "Deliver", SUCCESS, SUCCESS, INK, 10))
    out.append(button(1416, 372, 128, 22, "Abandon", PANEL, DANGER, DANGER, 10))
    out.append(text(1266, 430, "ACTIONS", size=10, weight=800, fill=MUTED))
    for i, label in enumerate(["Talk", "Trade", "Ask for work", "Attack"]):
        out.append(button(1266, 444 + i * 34, 290, 26, label, PANEL2, DANGER if label == "Attack" else LINE, DANGER if label == "Attack" else TEXT))
    out.append(text(1266, 602, "RECENT", size=10, weight=800, fill=MUTED))
    out.append(text(1266, 624, "contract accepted", size=11, fill=SUCCESS))
    out.append(text(1266, 642, "trace started on your program", size=11, fill=DANGER))
    out.append(text(1266, 660, "mail from eve.example", size=11, fill=BETA))

    # Bottom ticker
    out.append(frame(16, 846, 1568, 138, "Console / ticker", "Ctrl-K"))
    out.append(rect(30, 870, 760, 30, "#0D141A", LINE, rx=5))
    out.append(text(42, 890, "> ", size=14, weight=700, fill=OLD, family=MONO))
    out.append(text(58, 890, "type a command…  eat · work 0 · talk · accept 12 · help", size=12, fill=FAINT, family=MONO))
    out.append(text(810, 890, "Tab views · ? help · Esc close", size=10, fill=FAINT))
    events = [
        ("+$2.20", "contract paid", SUCCESS),
        ("! trace 4/6", "warden closing", DANGER),
        ("✉ 3", "mail from eve.example", BETA),
        ("◈ 2.4", "heat rising", DANGER),
    ]
    for i, (tag, body, color) in enumerate(events):
        y = 922 + i * 16
        out.append(text(30, y, tag, size=11, weight=700, fill=color, family=MONO))
        out.append(text(108, y, body, size=11, fill=MUTED))
    out.append(text(900, 922, "FILTER", size=9, weight=800, fill=FAINT))
    out.append(chip(900, 932, 58, "all", BETA))
    out.append(chip(966, 932, 58, "combat", DANGER))
    out.append(chip(1032, 932, 58, "mail", BETA))
    out.append(chip(1098, 932, 58, "money", MONEY))

    out.append("</svg>")
    return "\n".join(out)


def deck_wireframe():
    w, h = 1600, 1000
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-labelledby="title desc">',
        "<title id=\"title\">Deck UI wireframe</title>",
        "<desc id=\"desc\">Deck view: CPU rail, RAM arena, trace fuse, program inspector, card hand, and event ticker.</desc>",
        f'<rect width="{w}" height="{h}" fill="{INK}"/>',
        top_bar(w, active=1, location="host 1 · Deck"),
    ]

    # Left rail: CPU / loadout / programs
    out.append(frame(16, 70, 264, 760, "Rig", "host 1"))
    out.append(text(30, 106, "CPU RAIL", size=10, weight=800, fill=BETA))
    out.append(bar(30, 132, 220, 14, 0.46, BETA, "", ""))
    out.append(text(30, 166, "46 / 100 · headroom 54", size=11, fill=MUTED, family=MONO))
    out.append(text(30, 202, "LOADOUT", size=10, weight=800, fill=MUTED))
    kinds = [("K", "Killer", OLD), ("S", "Shield", SUCCESS), ("Sc", "Scanner", BETA), ("Cr", "Cracker", VEIL)]
    for i, (g, name, color) in enumerate(kinds):
        y = 216 + i * 34
        out.append(rect(30, y, 220, 26, PANEL2, LINE, rx=5))
        out.append(text(42, y + 17, g, size=11, weight=800, fill=color, family=MONO))
        out.append(text(66, y + 17, name, size=11, weight=600, fill=TEXT))
        out.append(text(232, y + 17, "1cpu" if i != 1 else "2cpu", size=10, fill=FAINT, family=MONO, anchor="end"))
    out.append(text(30, 380, "PROGRAMS", size=10, weight=800, fill=MUTED))
    for i, (name, state, color) in enumerate([("killer-1", "head 3/3", OLD), ("shield-2", "guard", SUCCESS), ("scanner-1", "scan ready", BETA)]):
        y = 394 + i * 34
        out.append(rect(30, y, 220, 26, PANEL2, LINE, rx=5))
        out.append(text(42, y + 17, name, size=11, weight=600, fill=TEXT, family=MONO))
        out.append(text(232, y + 17, state, size=10, fill=color, anchor="end"))
    out.append(text(30, 520, "TRACE", size=10, weight=800, fill=DANGER))
    out.append(bar(30, 534, 220, 8, 0.66, DANGER, "", ""))
    out.append(text(30, 558, "4/6 · 3 acts to burn", size=10, fill=DANGER, family=MONO))
    out.append(button(30, 760, 220, 28, "Disconnect / eject", fill=PANEL, stroke=LINE, color=MUTED))

    # Center
    out.append(frame(296, 70, 940, 760, "Deck — host 1 · office-ws", "trace active"))
    out.append(text(312, 104, "OFFICE-WS", size=10, weight=800, fill=OLD))
    out.append(text(312, 126, "world host 1 · 12x8 RAM · 46/100 CPU", size=12, fill=MUTED))
    out.append(text(312, 150, "monitor armed · firewall on · privileges none", size=11, fill=DANGER))
    # trace fuse
    out.append(text(312, 178, "TRACE FUSE", size=9, weight=800, fill=DANGER))
    out.append(rect(312, 186, 600, 14, "#2A1416", DANGER, rx=4))
    out.append(rect(312, 186, 400, 14, DANGER, DANGER, rx=4))
    out.append(text(926, 198, "4/6", size=11, weight=700, fill=DANGER, anchor="end", family=MONO))

    # Arena 12x8 at 50
    mx, my, ts = 326, 220, 50
    out.append(rect(mx - 10, my - 10, 12 * ts + 20, 8 * ts + 20, "#0D141A", LINE, rx=8))
    for y in range(8):
        for x in range(12):
            out.append(rect(mx + x * ts, my + y * ts, ts - 2, ts - 2, "#101A20", "#1E2A33", rx=2, width=0.7))
    # copper traces / selection
    for x in range(12):
        out.append(line(mx + x * ts + ts / 2, my, mx + x * ts + ts / 2, my + 8 * ts, "#16232C", 0.5, True))
    # serpentine killer: head at 7,3; tail 6,3 5,3 5,4
    segs = [(5, 4, "o"), (5, 3, "o"), (6, 3, "o"), (7, 3, "H")]
    for x, y, g in segs:
        col = OLD if g == "H" else "#8A5A1B"
        out.append(rect(mx + x * ts + 4, my + y * ts + 4, ts - 10, ts - 10, "#2A2114", col, rx=7, width=1.4))
        out.append(text(mx + x * ts + ts / 2, my + y * ts + 29, g, size=13, weight=800, fill=OLD if g == "H" else MUTED, anchor="middle", family=MONO))
    # shield at 2,5
    out.append(rect(mx + 2 * ts + 4, my + 5 * ts + 4, ts - 10, ts - 10, "#14251D", SUCCESS, rx=7, width=1.4))
    out.append(text(mx + 2 * ts + ts / 2, my + 5 * ts + 29, "S", size=13, weight=800, fill=SUCCESS, anchor="middle", family=MONO))
    # scanner selected at 9,2
    out.append(rect(mx + 9 * ts + 4, my + 2 * ts + 4, ts - 10, ts - 10, "#10252A", BETA, rx=7, width=2.0))
    out.append(text(mx + 9 * ts + ts / 2, my + 2 * ts + 29, "Sc", size=12, weight=800, fill=BETA, anchor="middle", family=MONO))
    # vulnerability ports
    for i, (label, color) in enumerate([("cve-1", DANGER), ("cve-2", DANGER), ("cve-3", MUTED)]):
        out.append(chip(312 + i * 88, 642, 80, label, color, size=10))
    out.append(text(312, 690, "Selected: scanner-1 · kind Scanner · affinity Data · scan ready", size=11, fill=MUTED))

    # Card hand
    out.append(text(312, 722, "HAND", size=9, weight=800, fill=MUTED))
    cards = [("spike", "2 LOOP", OLD), ("patch", "1 DATA", BETA), ("tank", "1 LOOP", SUCCESS), ("copy", "—", MUTED), ("jam", "1 LOOP", VEIL)]
    for i, (name, cost, color) in enumerate(cards):
        x = 312 + i * 118
        out.append(rect(x, 734, 106, 76, "#121B22", color, rx=7, width=1.2))
        out.append(text(x + 53, 758, name, size=13, weight=700, fill=TEXT, anchor="middle"))
        out.append(text(x + 53, 778, cost, size=10, fill=color, anchor="middle", family=MONO))
        out.append(text(x + 53, 798, "hover for rules", size=9, fill=FAINT, anchor="middle"))

    # Right inspector
    out.append(frame(1252, 70, 332, 760, "Inspector", "program"))
    out.append(text(1266, 108, "SCANNER-1", size=10, weight=800, fill=BETA))
    out.append(text(1266, 134, "Scanner", size=20, weight=700, fill=TEXT))
    out.append(text(1266, 156, "head 9,2 · 1 segment · scan ready", size=11, fill=MUTED, family=MONO))
    out.append(line(1266, 174, 1570, 174))
    for i, (k, v) in enumerate([("CPU", "2"), ("MAX LEN", "2"), ("AFFINITY", "Data"), ("MODS", "none")]):
        y = 202 + i * 26
        out.append(text(1266, y, k, size=10, weight=800, fill=FAINT))
        out.append(text(1570, y, v, size=11, weight=600, fill=TEXT, anchor="end"))
    out.append(text(1266, 324, "ACTIONS", size=10, weight=800, fill=MUTED))
    for i, (label, color) in enumerate([("Scan host", BETA), ("Crack lock", VEIL), ("Patch vuln", SUCCESS), ("Clone", OLD)]):
        out.append(button(1266, 338 + i * 34, 290, 26, label, PANEL2, color, color))
    out.append(text(1266, 506, "SCAN REPORT", size=10, weight=800, fill=MUTED))
    out.append(rect(1266, 518, 290, 92, PANEL2, LINE, rx=7))
    out.append(text(1278, 542, "vulns [cve-1, cve-2]", size=11, fill=DANGER, family=MONO))
    out.append(text(1278, 562, "logs 2 · trace 4/6", size=11, fill=MUTED, family=MONO))
    out.append(text(1278, 582, "breached no · filter on", size=11, fill=MUTED, family=MONO))
    out.append(text(1266, 644, "TRACE", size=10, weight=800, fill=DANGER))
    out.append(text(1266, 666, "warden closing · scrub or outrun", size=11, fill=DANGER))
    out.append(button(1266, 690, 290, 28, "Scrub log", PANEL, DANGER, DANGER))

    # Bottom ticker
    out.append(frame(16, 846, 1568, 138, "Console / ticker", "Ctrl-K"))
    out.append(rect(30, 870, 760, 30, "#0D141A", LINE, rx=5))
    out.append(text(42, 890, "> ", size=14, weight=700, fill=BETA, family=MONO))
    out.append(text(58, 890, "load scanner · scan · crack · patch · clone · strike", size=12, fill=FAINT, family=MONO))
    out.append(text(810, 890, "drag program to step · 1-7 kind · Esc inspect", size=10, fill=FAINT))
    for i, (tag, body, color) in enumerate(
        [("SCAN", "2 vulns · trace 4/6", BETA), ("TRACE", "warden closing", DANGER), ("CPU", "54 headroom", MUTED), ("CARD", "spike costs 2 LOOP", OLD)]
    ):
        y = 922 + i * 16
        out.append(text(30, y, tag, size=10, weight=800, fill=color, family=MONO))
        out.append(text(108, y, body, size=11, fill=MUTED))
    out.append(chip(900, 932, 70, "deck", BETA))
    out.append(chip(978, 932, 70, "trace", DANGER))
    out.append(chip(1056, 932, 70, "cards", OLD))

    out.append("</svg>")
    return "\n".join(out)


def main():
    (OUT / "ui-street-wireframe.svg").write_text(street_wireframe(), encoding="utf-8")
    (OUT / "ui-deck-wireframe.svg").write_text(deck_wireframe(), encoding="utf-8")
    print("wrote", OUT / "ui-street-wireframe.svg")
    print("wrote", OUT / "ui-deck-wireframe.svg")


if __name__ == "__main__":
    main()
