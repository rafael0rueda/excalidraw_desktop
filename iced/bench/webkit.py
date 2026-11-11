#!/usr/bin/env python3
"""Excalidraw 0.18.1 in WebKitGTK, the engine the Tauri app runs it in.

The reference side of the phase 1 gate (docs/iced-port/PLAN.md):

  webkit.py shot  DRAWING OUT.png X0 Y0 X1 Y1 [ZOOM]   same view as the iced --screenshot
  webkit.py bench DRAWING SECONDS X0 Y0 X1 Y1          same animation as the iced --bench

X0..Y1 are the scene bounds the iced viewer prints, so both fit the drawing
the same way. Build the page first: node iced/bench/webkit/build.mjs
"""
import json
import os
import sys

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
TICKS = os.sysconf("SC_CLK_TCK")


def tree_cpu_seconds(root):
    """User + system CPU time of `root` and every process below it: WebKit
    renders in child processes (web, GPU), which all count."""
    children = {}
    for pid in filter(str.isdigit, os.listdir("/proc")):
        try:
            stat = open(f"/proc/{pid}/stat").read()
        except OSError:
            continue
        fields = stat[stat.rindex(")") + 2:].split()
        children.setdefault(int(fields[1]), []).append((int(pid), int(fields[11]) + int(fields[12])))
    total, todo = 0, [root]
    own = open(f"/proc/{root}/stat").read()
    own = own[own.rindex(")") + 2:].split()
    total += int(own[11]) + int(own[12])
    while todo:
        for pid, ticks in children.get(todo.pop(), []):
            total += ticks
            todo.append(pid)
    return total / TICKS
PAGE = os.path.join(HERE, "../target/bench/webkit/index.html")
W, H = 1280, 800


LAUNCHED = __import__("time").monotonic()


def main():
    mode, drawing = sys.argv[1], sys.argv[2]
    data = json.load(open(drawing))
    if mode == "shot":
        out = sys.argv[3]
        bounds = [float(v) for v in sys.argv[4:8]]
        zoom = float(sys.argv[8]) if len(sys.argv) > 8 else 1.0
        seconds = 0
    else:
        seconds = float(sys.argv[3])
        bounds = [float(v) for v in sys.argv[4:8]]
        zoom = 1.0

    # A real window: an offscreen one gets no GL context, and WebKit aborts.
    win = Gtk.Window()
    win.set_default_size(W, H)
    manager = WebKit2.UserContentManager()
    manager.register_script_message_handler("harness")
    view = WebKit2.WebView.new_with_user_content_manager(manager)
    settings = view.get_settings()
    settings.set_allow_file_access_from_file_urls(True)
    settings.set_allow_universal_access_from_file_urls(True)
    settings.set_enable_write_console_messages_to_stdout(bool(os.environ.get("BENCH_CONSOLE")))
    win.add(view)
    win.show_all()

    def js(code):
        view.evaluate_javascript(code, -1, None, None, None, None, None)

    def done(code=0):
        Gtk.main_quit()
        sys.exit(code)

    def snapshot():
        def got(v, result):
            surface = v.get_snapshot_finish(result)
            surface.write_to_png(out)
            print(f"saved {out}")
            done()

        view.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, got)
        return False

    def on_message(_manager, message):
        msg = json.loads(message.to_json(0) if hasattr(message, "to_json") else message.get_js_value().to_string())
        if isinstance(msg, str):
            msg = json.loads(msg)
        if msg.get("ready"):
            payload = json.dumps(data)
            if mode == "shot":
                # The bench with zero seconds puts the view where the iced
                # viewer's fit (and zoom) puts it, then holds it.
                js(f"window.bench({payload}, 0, {json.dumps(bounds)}, {zoom})")
                GLib.timeout_add(4000, snapshot)
            else:
                js(f"window.bench({payload}, {seconds}, {json.dumps(bounds)}, 1)")
        elif "loadMs" in msg:
            since = (__import__("time").monotonic() - LAUNCHED) * 1000
            print(f"scene loaded {msg['loadMs']:.0f} ms after the editor mounted, {since:.0f} ms after launch")
        elif "started" in msg:
            cpu_at_start[0] = tree_cpu_seconds(os.getpid())
            if os.environ.get("BENCH_SNAPSHOTS"):
                # Two views a quarter of a second apart in the pan: if they
                # are the same picture, nothing is being repainted.
                for i, ms in enumerate((1000, 1250)):
                    def take(i=i):
                        view.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None,
                                          lambda v, r, i=i: v.get_snapshot_finish(r).write_to_png(f"/tmp/bench-snap-{i}.png"))
                        return False
                    GLib.timeout_add(ms, take)
        elif "intervals" in msg:
            cpu = tree_cpu_seconds(os.getpid()) - cpu_at_start[0]
            report(msg["intervals"], seconds)
            frames = len(msg["intervals"])
            print(f"CPU, all processes          {cpu:.2f} s over {frames} frames = {cpu * 1000 / max(frames, 1):.2f} ms/frame")
            d = msg.get("draws", {})
            print("canvas calls per frame      " + "  ".join(f"{k} {v / max(frames, 1):.0f}" for k, v in d.items()))
            done()

    cpu_at_start = [0.0]
    manager.connect("script-message-received::harness", on_message)
    view.load_uri("file://" + os.path.abspath(PAGE))
    GLib.timeout_add_seconds(int(seconds) + int(os.environ.get("BENCH_TIMEOUT", "60")), lambda: done(1))
    Gtk.main()


def report(intervals, seconds):
    def stats(name, xs):
        if not xs:
            return
        xs = sorted(xs)
        mean = sum(xs) / len(xs)
        p = lambda q: xs[int((len(xs) - 1) * q)]  # noqa: E731
        print(f"{name:<28} n={len(xs):<5} mean {mean:6.2f} ms  p50 {p(0.5):6.2f}  p95 {p(0.95):6.2f}  max {xs[-1]:6.2f}  ({1000 / mean:.0f} fps)")

    half = seconds / 2
    stats("pan: frame interval", [d for t, d in intervals if t < half])
    stats("zoom: frame interval", [d for t, d in intervals if t >= half])


if __name__ == "__main__":
    main()
