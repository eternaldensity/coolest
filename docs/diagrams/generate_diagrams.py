#!/usr/bin/env python3
"""Generate the proposed-architecture and gameplay-loop SVGs.

No third-party dependencies: plain SVG emitted from Python so the diagrams
stay diffable and reproducible.
"""

from __future__ import annotations

from math import hypot
from pathlib import Path
from xml.sax.saxutils import escape

OUT = Path(__file__).resolve().parent

INK = "#0f172a"
MUTED = "#475569"
FAINT = "#94a3b8"
BG = "#f8fafc"
WHITE = "#ffffff"
GRID = "#cbd5e1"

BUILT_FILL = "#dbeafe"
BUILT_STROKE = "#2563eb"
SIM_FILL = "#dcfce7"
SIM_STROKE = "#16a34a"
SERVER_FILL = "#e0e7ff"
SERVER_STROKE = "#4f46e5"
PROP_FILL = "#ffedd5"
PROP_STROKE = "#ea580c"
EDGE_FILL = "#f1f5f9"
EDGE_STROKE = "#64748b"
PDS_FILL = "#f3e8ff"
PDS_STROKE = "#9333ea"
LOG_FILL = "#fef9c3"
LOG_STROKE = "#ca8a04"
WARN_FILL = "#fff1f2"
WARN_STROKE = "#e11d48"
OK_FILL = "#ecfdf5"
OK_STROKE = "#059669"

FONT = "Inter, ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif"
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"


def esc(s: object) -> str:
    return escape(str(s), {"'": "&apos;", '"': "&quot;"})


def text(
    x: float,
    y: float,
    s: str,
    size: int = 14,
    weight: int = 400,
    fill: str = INK,
    anchor: str = "start",
    family: str = FONT,
    italic: bool = False,
    opacity: float = 1.0,
) -> str:
    style = f'font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}"'
    if italic:
        style += ' font-style="italic"'
    if opacity != 1.0:
        style += f' opacity="{opacity}"'
    return f'<text x="{x:.1f}" y="{y:.1f}" text-anchor="{anchor}" {style}>{esc(s)}</text>'


def text_lines(
    x: float,
    y: float,
    lines: list[str] | tuple[str, ...],
    size: int = 13,
    weight: int = 400,
    fill: str = MUTED,
    anchor: str = "middle",
    family: str = FONT,
    line_height: float | None = None,
) -> str:
    if not lines:
        return ""
    lh = line_height or size * 1.35
    parts = []
    for i, line in enumerate(lines):
        dy = 0 if i == 0 else lh
        parts.append(
            f'<tspan x="{x:.1f}" dy="{dy:.1f}">{esc(line)}</tspan>'
        )
    style = f'font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}"'
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" text-anchor="{anchor}" {style}>'
        + "".join(parts)
        + "</text>"
    )


def rect(
    x: float,
    y: float,
    w: float,
    h: float,
    fill: str,
    stroke: str,
    rx: float = 14,
    dash: bool = False,
    width: float = 1.6,
    opacity: float = 1.0,
) -> str:
    da = ' stroke-dasharray="7 5"' if dash else ""
    return (
        f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" '
        f'rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{width}"{da} '
        f'opacity="{opacity}"/>'
    )


def box(
    x: float,
    y: float,
    w: float,
    h: float,
    title: str,
    lines: list[str] | tuple[str, ...] = (),
    fill: str = WHITE,
    stroke: str = GRID,
    dash: bool = False,
    title_size: int = 16,
    body_size: int = 13,
    title_fill: str = INK,
    body_fill: str = MUTED,
) -> str:
    out = [rect(x, y, w, h, fill, stroke, dash=dash)]
    out.append(text(x + w / 2, y + 27, title, size=title_size, weight=700, fill=title_fill, anchor="middle"))
    if lines:
        out.append(text_lines(x + w / 2, y + 49, lines, size=body_size, fill=body_fill, line_height=body_size * 1.3))
    return "\n".join(out)


def diamond(cx: float, cy: float, hw: float, hh: float, lines: list[str], fill: str = "#fef3c7", stroke: str = "#d97706") -> str:
    pts = f"{cx:.1f},{cy-hh:.1f} {cx+hw:.1f},{cy:.1f} {cx:.1f},{cy+hh:.1f} {cx-hw:.1f},{cy:.1f}"
    out = [f'<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="1.8"/>']
    n = len(lines)
    start = cy - (n - 1) * 9 + 5
    out.append(text_lines(cx, start, lines, size=13, weight=600, fill=INK, anchor="middle", line_height=18))
    return "\n".join(out)


def marker_defs() -> str:
    defs = ["<defs>"]
    for name, color in [
        ("ink", INK),
        ("muted", MUTED),
        ("blue", BUILT_STROKE),
        ("orange", PROP_STROKE),
        ("gray", EDGE_STROKE),
        ("red", WARN_STROKE),
        ("green", OK_STROKE),
    ]:
        defs.append(
            f'<marker id="arrow-{name}" viewBox="0 0 10 10" refX="9" refY="3" '
            f'markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M0,0 L0,6 L9,3 z" fill="{color}"/></marker>'
        )
    defs.append("</defs>")
    return "\n".join(defs)


def line(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    color: str = INK,
    dash: bool = False,
    marker: str = "ink",
    width: float = 1.8,
) -> str:
    da = ' stroke-dasharray="7 5"' if dash else ""
    return (
        f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
        f'stroke="{color}" stroke-width="{width}"{da} marker-end="url(#arrow-{marker})"/>'
    )


def two_way(x1: float, y1: float, x2: float, y2: float, color: str = INK, marker: str = "ink", offset: float = 7.0) -> str:
    dx, dy = x2 - x1, y2 - y1
    length = hypot(dx, dy) or 1.0
    px, py = -dy / length * offset / 2, dx / length * offset / 2
    a1 = line(x1 + px, y1 + py, x2 + px, y2 + py, color=color, marker=marker)
    a2 = line(x2 - px, y2 - py, x1 - px, y1 - py, color=color, marker=marker)
    return a1 + "\n" + a2


def poly(points: list[tuple[float, float]], color: str = INK, dash: bool = False, marker: str = "ink", width: float = 1.8) -> str:
    da = ' stroke-dasharray="7 5"' if dash else ""
    pts = " ".join(f"{x:.1f},{y:.1f}" for x, y in points)
    return (
        f'<polyline points="{pts}" fill="none" stroke="{color}" stroke-width="{width}"'
        f'{da} marker-end="url(#arrow-{marker})"/>'
    )


def label(x: float, y: float, s: str, size: int = 12, fill: str = MUTED, anchor: str = "middle", rotate: float | None = None) -> str:
    rot = f' transform="rotate({rotate} {x:.1f} {y:.1f})"' if rotate is not None else ""
    return (
        f'<text x="{x:.1f}" y="{y:.1f}" text-anchor="{anchor}"{rot} '
        f'font-family="{FONT}" font-size="{size}" font-weight="600" fill="{fill}" '
        f'paint-order="stroke" stroke="{BG}" stroke-width="4">{esc(s)}</text>'
    )


def frame(x: float, y: float, w: float, h: float, title: str, subtitle: str = "") -> str:
    out = [
        rect(x, y, w, h, BG, GRID, rx=20, dash=True, width=1.4),
        text(x + 20, y + 30, title, size=13, weight=800, fill=MUTED, anchor="start"),
    ]
    if subtitle:
        out.append(text(x + 20, y + 48, subtitle, size=11, weight=500, fill=FAINT, anchor="start"))
    return "\n".join(out)


def legend(x: float, y: float) -> str:
    items = [
        (40, SIM_FILL, SIM_STROKE, False, "implemented"),
        (220, PROP_FILL, PROP_STROKE, True, "proposed"),
        (360, EDGE_FILL, EDGE_STROKE, False, "external / optional"),
        (560, WARN_FILL, WARN_STROKE, True, "trust boundary / no direct access"),
    ]
    out = []
    for cx, fill, stroke, dash, name in items:
        out.append(rect(cx, y - 12, 20, 16, fill, stroke, rx=4, dash=dash, width=1.4))
        out.append(text(cx + 28, y, name, size=12, weight=600, fill=MUTED))
    return "\n".join(out)


def architecture_svg() -> str:
    w, h = 1600, 1000
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-labelledby="title desc">',
        "<title id=\"title\">FIDOM proposed core architecture</title>",
        "<desc id=\"desc\">Deterministic sim and server, separate ATProto bridge, PDS edge, and the trust boundary between them.</desc>",
        marker_defs(),
        f'<rect width="{w}" height="{h}" fill="{WHITE}"/>',
        text(40, 44, "FIDOM — proposed core architecture", size=28, weight=800, fill=INK),
        text(40, 72, "v0.4 deterministic sim + proposed PDS bridge. The bridge can submit intents; it never holds &mut World.", size=14, fill=MUTED),
    ]

    # Frames
    out.append(frame(30, 100, 280, 800, "CLIENTS", "render-only"))
    out.append(frame(340, 100, 750, 800, "AUTHORITATIVE FIDOM HOST", "one clock · one state · one log"))
    out.append(frame(1130, 100, 440, 800, "ATPROTO EDGE", "asynchronous · untrusted input"))

    # Clients
    out.append(box(55, 160, 230, 120, "egui client", ["desktop + web", "Street · Deck · Net · Veil"], BUILT_FILL, BUILT_STROKE))
    out.append(box(55, 310, 230, 100, "client-web / tools", ["scripted tour", "fuzz · replay · soak"], BUILT_FILL, BUILT_STROKE))
    out.append(box(55, 450, 230, 90, "render-only", ["submit Intent", "render Delta + Events"], WARN_FILL, WARN_STROKE, dash=True))

    # Authority
    out.append(box(370, 160, 310, 130, "server::Server", ["intent queue · clock", "snapshot · replay"], SERVER_FILL, SERVER_STROKE))
    out.append(box(740, 160, 310, 130, "Intent log + snapshot", ["source of truth for replay", "accepted + refused intents"], LOG_FILL, LOG_STROKE))
    out.append(box(370, 340, 680, 200, "sim::World — deterministic", ["no clock · no network · no filesystem · no threads"], SIM_FILL, SIM_STROKE, title_size=17))
    out.append(box(395, 430, 145, 70, "Street", ["needs · jobs", "contracts · combat", "items"], WHITE, SIM_STROKE, title_size=13, body_size=10.5))
    out.append(box(555, 430, 145, 70, "Deck", ["hosts · cards", "trace · warden"], WHITE, SIM_STROKE, title_size=13, body_size=11))
    out.append(box(715, 430, 145, 70, "Net", ["routes · power", "firewalls"], WHITE, SIM_STROKE, title_size=13, body_size=11))
    out.append(box(875, 430, 145, 70, "Veil", ["mindpower · files", "hazards"], WHITE, SIM_STROKE, title_size=13, body_size=11))

    # Bus + bridge
    out.append(rect(360, 575, 690, 14, "#334155", "#334155", rx=7))
    out.append(text(705, 566, "privileged bridge queue", size=11, weight=700, fill=PROP_STROKE, anchor="middle"))
    out.append(box(370, 625, 310, 150, "ATProto bridge — proposed", ["identity · lexicon", "outbox/inbox · policy"], PROP_FILL, PROP_STROKE, dash=True))
    out.append(box(740, 625, 310, 150, "Bridge DB — proposed", ["cursor · queue", "DID map · board/offer index"], PROP_FILL, PROP_STROKE, dash=True))
    out.append(text(705, 882, "Bridge may submit Intents and receive BridgeEvents; it never holds &mut World.", size=12, weight=600, fill=PROP_STROKE, anchor="middle"))

    # Edge
    out.append(box(1160, 160, 380, 140, "PDS", ["Aurora-Locus / official", "repo · auth · sync · firehose"], PDS_FILL, PDS_STROKE))
    out.append(box(1160, 360, 380, 110, "Relay / Jetstream", ["optional; cross-city fan-out"], EDGE_FILL, EDGE_STROKE))
    out.append(box(1160, 530, 380, 110, "other players' PDS", ["sh.fidom.* records"], EDGE_FILL, EDGE_STROKE))
    out.append(box(1160, 700, 380, 100, "other ATProto clients", ["Bluesky app · web · bots"], EDGE_FILL, EDGE_STROKE))

    # Trust boundary
    out.append(f'<line x1="1120" y1="110" x2="1120" y2="890" stroke="{WARN_STROKE}" stroke-width="2" stroke-dasharray="7 6"/>')
    out.append(label(1120, 920, "trust boundary", size=11, fill=WARN_STROKE))

    # Client -> server
    out.append(line(285, 220, 370, 220, color=SERVER_STROKE, marker="blue"))
    out.append(label(327, 208, "Intent", size=11, fill=SERVER_STROKE))
    out.append(poly([(285, 360), (330, 360), (330, 250), (370, 250)], color=SERVER_STROKE, marker="blue"))
    out.append(label(332, 306, "Intent", size=11, fill=SERVER_STROKE, rotate=-90))

    # Server <-> sim, server -> log
    out.append(two_way(540, 290, 540, 340, color=SERVER_STROKE, marker="blue", offset=8))
    out.append(label(558, 322, "apply / Delta", size=11, fill=SERVER_STROKE, anchor="start"))
    out.append(line(680, 225, 740, 225, color=LOG_STROKE, marker="ink"))
    out.append(label(710, 213, "append", size=11, fill=LOG_STROKE))

    # Server -> bus (route left of sim)
    out.append(poly([(370, 270), (350, 270), (350, 582), (360, 582)], color=PROP_STROKE, dash=True, marker="orange"))
    out.append(label(350, 470, "BridgeEvent / Intent", size=10, fill=PROP_STROKE, rotate=-90))

    # Bus -> bridge
    out.append(line(525, 589, 525, 625, color=PROP_STROKE, dash=True, marker="orange"))
    out.append(line(895, 589, 895, 625, color=PROP_STROKE, dash=True, marker="orange"))

    # Bridge <-> DB
    out.append(two_way(680, 700, 740, 700, color=PROP_STROKE, marker="orange", offset=7))
    out.append(label(710, 688, "state", size=11, fill=PROP_STROKE))

    # Bridge -> PDS (route below, then up the boundary)
    out.append(poly([(525, 775), (525, 850), (1112, 850), (1112, 230), (1160, 230)], color=PROP_STROKE, dash=True, marker="orange"))
    out.append(label(820, 840, "XRPC · OAuth · firehose (both ways)", size=11, fill=PROP_STROKE))

    # Edge links
    out.append(two_way(1350, 300, 1350, 360, color=EDGE_STROKE, marker="gray", offset=8))
    out.append(two_way(1350, 470, 1350, 530, color=EDGE_STROKE, marker="gray", offset=8))
    out.append(two_way(1350, 640, 1350, 700, color=EDGE_STROKE, marker="gray", offset=8))

    out.append(legend(40, 950))
    out.append("</svg>")
    return "\n".join(out)


def gameplay_svg() -> str:
    w, h = 1500, 1200
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-labelledby="title desc">',
        "<title id=\"title\">FIDOM supported gameplay loop</title>",
        "<desc id=\"desc\">The local turn loop from waking through work, gear, hacking, heat and cops, with the asynchronous ATProto mail/board edge.</desc>",
        marker_defs(),
        f'<rect width="{w}" height="{h}" fill="{WHITE}"/>',
        text(40, 44, "FIDOM — supported gameplay loop", size=28, weight=800, fill=INK),
        text(40, 72, "Solid = local turn loop. Dashed = asynchronous ATProto edge.", size=14, fill=MUTED),
    ]

    # Side panel: view contributions
    out.append(rect(1160, 110, 320, 330, "#f8fafc", GRID, rx=16, dash=True, width=1.3))
    out.append(text(1180, 140, "What each view adds", size=15, weight=800, fill=INK))
    panel = [
        ("Street", "needs · jobs · combat"),
        ("Net", "routes · tolls · power"),
        ("Deck", "hosts · CPU · trace"),
        ("Veil", "mindpower · files"),
        ("Board", "offers · accept · deadline"),
        ("PDS", "mail · board transport"),
    ]
    y = 175
    for name, desc in panel:
        out.append(text(1180, y, name, size=13, weight=800, fill=SERVER_STROKE))
        out.append(text(1250, y, desc, size=11, fill=MUTED))
        y += 36

    # Nodes
    out.append(box(620, 60, 220, 54, "Wake / log in", [], BUILT_FILL, BUILT_STROKE, title_size=15))
    out.append(box(540, 150, 380, 70, "Check mail, board, clan notices", ["mailbox · offers · active contract"], BUILT_FILL, BUILT_STROKE, title_size=15, body_size=12))
    out.append(diamond(730, 290, 140, 60, ["Food / mood / HP OK?"]))
    out.append(box(330, 380, 240, 70, "Recover", ["Eat · Watch · medkit · sleep"], OK_FILL, OK_STROKE, title_size=15))
    out.append(box(600, 390, 260, 60, "Work · Code · Farm", ["components + cash"], BUILT_FILL, BUILT_STROKE, title_size=15))
    out.append(box(940, 390, 240, 60, "Buy / equip gear", ["weapon · armor · meds"], BUILT_FILL, BUILT_STROKE, title_size=15))
    out.append(box(940, 490, 240, 60, "Plan route in Net", ["hops · tolls · trace risk"], BUILT_FILL, BUILT_STROKE, title_size=15))
    out.append(box(940, 590, 240, 60, "Hack host in Deck / Veil", ["breach · cards · trace"], BUILT_FILL, BUILT_STROKE, title_size=15, body_size=12))
    out.append(box(1260, 590, 220, 60, "Loot", ["components · data · files"], OK_FILL, OK_STROKE, title_size=15))
    out.append(diamond(1060, 730, 140, 62, ["Heat / Wanted?"]))
    out.append(diamond(1060, 880, 140, 62, ["Cops nearby?"]))
    out.append(box(760, 980, 220, 60, "Fine or jail", ["pay / surrender", "record cleared"], OK_FILL, OK_STROKE, title_size=15))
    out.append(box(1260, 980, 220, 60, "Street combat", ["resist · revive · escape"], WARN_FILL, WARN_STROKE, title_size=15))
    out.append(box(600, 1080, 220, 60, "Sleep / save", ["producers keep running"], BUILT_FILL, BUILT_STROKE, title_size=15))

    # Social edge
    out.append(box(160, 150, 300, 70, "PDS bridge — future", ["federated mail / board", "validated · queued · logged Intent"], PROP_FILL, PROP_STROKE, dash=True, title_size=15, body_size=12))
    out.append(line(460, 185, 540, 185, color=PROP_STROKE, dash=True, marker="orange"))
    out.append(label(500, 173, "async", size=11, fill=PROP_STROKE))
    out.append(box(120, 250, 340, 100, "Job board accept — proposed", ["offer → Accept → active contract", "deliver / hack / courier", "escrow payout / fail → reputation"], PROP_FILL, PROP_STROKE, dash=True, title_size=14, body_size=11))
    out.append(poly([(460, 300), (500, 300), (500, 200), (540, 200)], color=PROP_STROKE, dash=True, marker="orange"))
    out.append(label(510, 290, "contract", size=11, fill=PROP_STROKE, anchor="start"))

    # Arrows
    out.append(line(730, 114, 730, 150, color=INK, marker="ink"))
    out.append(line(730, 220, 730, 230, color=INK, marker="ink"))
    out.append(line(600, 290, 570, 380, color=OK_STROKE, marker="green"))
    out.append(label(570, 340, "No", size=12, fill=OK_STROKE))
    out.append(line(730, 350, 730, 390, color=INK, marker="ink"))
    out.append(label(748, 376, "Yes", size=12, fill=INK, anchor="start"))
    out.append(poly([(450, 450), (450, 470), (600, 470), (600, 450)], color=OK_STROKE, marker="green"))
    out.append(label(525, 488, "back to earning", size=11, fill=OK_STROKE))
    out.append(line(860, 420, 940, 420, color=INK, marker="ink"))
    out.append(line(1060, 450, 1060, 490, color=INK, marker="ink"))
    out.append(line(1060, 550, 1060, 590, color=INK, marker="ink"))
    out.append(line(1180, 620, 1260, 620, color=OK_STROKE, marker="green"))
    out.append(label(1220, 608, "loot", size=11, fill=OK_STROKE))
    out.append(poly([(1370, 590), (1370, 470), (1180, 470), (1180, 450)], color=OK_STROKE, marker="green"))
    out.append(label(1378, 520, "upgrade", size=11, fill=OK_STROKE, anchor="start"))
    out.append(line(1060, 650, 1060, 668, color=INK, marker="ink"))
    out.append(poly([(920, 730), (710, 730), (710, 1080)], color=OK_STROKE, marker="green"))
    out.append(label(800, 718, "No", size=12, fill=OK_STROKE))
    out.append(line(1060, 792, 1060, 818, color=INK, marker="ink"))
    out.append(label(1078, 810, "Yes", size=12, fill=INK, anchor="start"))
    out.append(poly([(920, 880), (870, 880), (870, 980)], color=OK_STROKE, marker="green"))
    out.append(label(858, 940, "Pay / Surrender", size=11, fill=OK_STROKE, anchor="end"))
    out.append(poly([(1200, 880), (1370, 880), (1370, 980)], color=WARN_STROKE, marker="red"))
    out.append(label(1300, 868, "Resist", size=11, fill=WARN_STROKE))
    out.append(poly([(870, 1040), (870, 1060), (750, 1060), (750, 1080)], color=OK_STROKE, marker="green"))
    out.append(poly([(1370, 1040), (1370, 1110), (820, 1110), (820, 1110)], color=WARN_STROKE, marker="red"))
    out.append(poly([(600, 1110), (40, 1110), (40, 87), (620, 87)], color=SERVER_STROKE, marker="blue"))
    out.append(label(70, 1096, "next day", size=11, fill=SERVER_STROKE, anchor="start"))
    out.append(label(1080, 1098, "escape / revive", size=11, fill=WARN_STROKE, anchor="end"))

    # Legend
    out.append(rect(40, 1168, 22, 16, BUILT_FILL, BUILT_STROKE, rx=4))
    out.append(text(70, 1181, "local turn loop", size=12, weight=600, fill=MUTED))
    out.append(rect(210, 1168, 22, 16, PROP_FILL, PROP_STROKE, rx=4, dash=True))
    out.append(text(240, 1181, "asynchronous ATProto edge", size=12, weight=600, fill=MUTED))
    out.append(rect(455, 1168, 22, 16, WARN_FILL, WARN_STROKE, rx=4))
    out.append(text(485, 1181, "risk / consequence", size=12, weight=600, fill=MUTED))

    out.append("</svg>")
    return "\n".join(out)


def main() -> None:
    (OUT / "core-architecture.svg").write_text(architecture_svg(), encoding="utf-8")
    (OUT / "gameplay-loop.svg").write_text(gameplay_svg(), encoding="utf-8")
    print("wrote", OUT / "core-architecture.svg")
    print("wrote", OUT / "gameplay-loop.svg")


if __name__ == "__main__":
    main()
