// Excalidraw 0.18.1 in a page, driven by webkit.py: renders a drawing to PNG
// (exportToCanvas), or mounts the editor and times the same pan and zoom the
// iced viewer's --bench runs.
import React from "react";
import { createRoot } from "react-dom/client";
import { Excalidraw, exportToCanvas } from "@excalidraw/excalidraw";
import "@excalidraw/excalidraw/index.css";

// The canvas alone: no toolbar, menus or footer over the drawing.
const style = document.createElement("style");
style.textContent = ".layer-ui__wrapper, .App-menu, .App-toolbar, .footer-center, .layer-ui__wrapper__footer { display: none !important; }";
document.head.appendChild(style);

// Counts canvas draws, to check that every animated frame really repaints.
const draws = { drawImage: 0, stroke: 0, fill: 0 };
for (const name of Object.keys(draws)) {
  const original = CanvasRenderingContext2D.prototype[name];
  CanvasRenderingContext2D.prototype[name] = function (...args) {
    draws[name]++;
    return original.apply(this, args);
  };
}

const post = (msg) => window.webkit.messageHandlers.harness.postMessage(JSON.stringify(msg));

// The iced viewer's fit and zoom, so both run the same animation.
const fit = (b, w, h) => {
  const zoom = Math.max(0.1, Math.min(1, Math.min((w - 80) / Math.max(1, b[2] - b[0]), (h - 80) / Math.max(1, b[3] - b[1]))));
  return { pan: [(w - (b[2] - b[0]) * zoom) / 2 - b[0] * zoom, (h - (b[3] - b[1]) * zoom) / 2 - b[1] * zoom], zoom };
};
const zoomed = (v, factor, ax, ay) => {
  const zoom = Math.max(0.1, Math.min(30, v.zoom * factor));
  const f = zoom / v.zoom;
  return { pan: [ax - (ax - v.pan[0]) * f, ay - (ay - v.pan[1]) * f], zoom };
};

window.exportPng = async (data, scale) => {
  const canvas = await exportToCanvas({
    elements: data.elements,
    appState: { ...data.appState, exportBackground: true, exportScale: scale },
    files: data.files || {},
    exportPadding: 0,
  });
  post({ png: canvas.toDataURL("image/png") });
};

window.bench = (data, seconds, bounds, extraZoom = 1) => {
  const root = createRoot(document.getElementById("root"));
  let api;
  const run = () => {
    const w = window.innerWidth, h = window.innerHeight;
    const base = zoomed(fit(bounds, w, h), extraZoom, w / 2, h / 2);
    const intervals = [];
    let start, last;
    const frame = (now) => {
      if (start === undefined) {
        start = now;
        if (seconds > 0) post({ started: true });
      }
      const t = (now - start) / 1000;
      if (last !== undefined) intervals.push([t, now - last]);
      last = now;
      let v;
      if (t < seconds / 2) {
        const a = t * 2 * Math.PI;
        v = { pan: [base.pan[0] + Math.cos(a) * 150, base.pan[1] + Math.sin(a) * 150], zoom: base.zoom };
      } else {
        const s = (1 - Math.cos((t - seconds / 2) * Math.PI)) / 2;
        v = zoomed(base, Math.pow(4, s), w / 2, h / 2);
      }
      // Excalidraw: screen = (scene + scroll) * zoom.
      api.updateScene({ appState: { zoom: { value: v.zoom }, scrollX: v.pan[0] / v.zoom, scrollY: v.pan[1] / v.zoom } });
      if (seconds === 0) {
        if (intervals.length < 3) requestAnimationFrame(frame);
        else post({ positioned: true });
        return;
      }
      if (t < seconds) requestAnimationFrame(frame);
      else post({ intervals, draws });
    };
    requestAnimationFrame(frame);
  };
  root.render(
    React.createElement(
      "div",
      { style: { position: "fixed", inset: 0 } },
      React.createElement(Excalidraw, {
        initialData: { elements: data.elements, appState: { ...data.appState, viewModeEnabled: true, zenModeEnabled: true }, files: data.files },
        excalidrawAPI: (a) => {
          api = a;
          // The API arrives before the scene does: wait until the loading
          // screen is gone and the elements are in, then a second more for
          // fonts and the first paint. Starting earlier measured a spinner.
          const loadedAt = performance.now();
          const wait = () => {
            const loading = document.querySelector(".LoadingMessage");
            if (!loading && api.getSceneElements().length > 0) {
              post({ loadMs: performance.now() - loadedAt });
              setTimeout(run, 1000);
            } else {
              setTimeout(wait, 50);
            }
          };
          wait();
        },
      }),
    ),
  );
};
post({ ready: true });
