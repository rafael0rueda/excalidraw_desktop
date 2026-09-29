var KanagawaERD = (function () {
  var THEMES = {
    wave: { name: 'Wave', bg: '#1F1F28', surface: '#2A2A37', border: '#54546D', divider: '#363646', text: '#DCD7BA', muted: '#727169', title: '#7E9CD8', pk: '#E6C384', fk: '#7FB4CA', rel: '#C8C093', weak: '#FFA066', view: '#957FB8', enum: '#98BB6C', noteBg: '#49443C', noteText: '#E6C384', danger: '#E46876' },
    dragon: { name: 'Dragon', bg: '#181616', surface: '#282727', border: '#625E5A', divider: '#393836', text: '#C5C9C5', muted: '#7A8382', title: '#8BA4B0', pk: '#C4B28A', fk: '#8EA4A2', rel: '#A6A69C', weak: '#B6927B', view: '#A292A3', enum: '#8A9A7B', noteBg: '#393836', noteText: '#C4B28A', danger: '#C4746E' },
    lotus: { name: 'Lotus', bg: '#F2ECBC', surface: '#E5DDB0', border: '#8A8980', divider: '#D5CEA3', text: '#545464', muted: '#716E61', title: '#4D699B', pk: '#CC6D00', fk: '#4E8CA2', rel: '#716E61', weak: '#B35B79', view: '#624C83', enum: '#6F894E', noteBg: '#E4D794', noteText: '#545464', danger: '#C84053' }
  };
  var HEAD = 6, MONO = 3;
  var s = 1;
  function rnd() { s = (s * 16807) % 2147483647; return s; }
  function uid() { return 'k' + rnd().toString(36) + rnd().toString(36); }
  function base(type, x, y, w, h, o) {
    var e = { id: uid(), type: type, x: x, y: y, width: w, height: h, angle: 0, strokeColor: '#000000', backgroundColor: 'transparent', fillStyle: 'solid', strokeWidth: 1, strokeStyle: 'solid', roughness: 0, opacity: 100, groupIds: [], frameId: null, roundness: null, seed: rnd(), version: 1, versionNonce: rnd(), isDeleted: false, boundElements: null, updated: 1, link: null, locked: false };
    for (var k in o) e[k] = o[k];
    return e;
  }
  function cw(size, fam) { return size * (fam === MONO ? 0.6 : 0.56); }
  function tw(str, size, fam) { var m = 0; String(str).split('\n').forEach(function (l) { m = Math.max(m, l.length); }); return Math.ceil(m * cw(size, fam)); }
  function text(x, y, str, o) {
    o = o || {}; var size = o.size || 16, fam = o.fam || MONO, lines = String(str).split('\n').length;
    var w = tw(str, size, fam), h = Math.round(size * 1.25 * lines);
    var ax = o.align === 'center' ? x - w / 2 : o.align === 'right' ? x - w : x;
    return base('text', ax, y, w, h, { strokeColor: o.color, text: str, originalText: str, fontSize: size, fontFamily: fam, textAlign: o.align || 'left', verticalAlign: 'top', containerId: null, autoResize: true, lineHeight: 1.25 });
  }
  function shape(type, x, y, w, h, o) {
    o = o || {};
    return base(type, x, y, w, h, { strokeColor: o.stroke || 'transparent', backgroundColor: o.fill || 'transparent', strokeWidth: o.sw || 1, strokeStyle: o.dash || 'solid', roundness: o.round ? { type: 3, value: o.round } : (type === 'rectangle' ? null : { type: 2 }) });
  }
  function rect(x, y, w, h, o) { return shape('rectangle', x, y, w, h, o); }
  function line(x, y, pts, o) {
    o = o || {};
    var xs = pts.map(function (p) { return p[0]; }), ys = pts.map(function (p) { return p[1]; });
    var e = base(o.arrow ? 'arrow' : 'line', x, y, Math.max.apply(0, xs) - Math.min.apply(0, xs), Math.max.apply(0, ys) - Math.min.apply(0, ys), { strokeColor: o.stroke, strokeWidth: o.sw || 1, strokeStyle: o.dash || 'solid', roundness: null, points: pts, lastCommittedPoint: null, startBinding: null, endBinding: null, startArrowhead: o.start || null, endArrowhead: o.end || null });
    if (o.arrow) e.elbowed = false;
    return e;
  }
  function group(els) { var g = uid(); els.forEach(function (e) { e.groupIds = e.groupIds.concat([g]); }); return els; }

  var ROW = 28, HDR = 44, PAD = 16;
  function entity(T, d, ox, oy) {
    ox = ox || 0; oy = oy || 0;
    var kind = d.kind || 'table';
    var color = { table: T.title, view: T.view, weak: T.weak, enum: T.enum, junction: T.fk }[kind];
    var cols = d.cols || [];
    var hasKeys = cols.some(function (c) { return c.k; });
    var keyW = hasKeys ? 34 : 0;
    var nameW = Math.max.apply(0, [0].concat(cols.map(function (c) { return tw(c.n, 15, MONO); })));
    var typeW = Math.max.apply(0, [0].concat(cols.map(function (c) { return c.t ? tw(c.t, 13, MONO) : 0; })));
    var label = kind === 'junction' ? 'join' : kind;
    var titleW = tw(d.name, 19, HEAD) + tw(label, 11, MONO) + 32;
    var w = Math.max(PAD + keyW + nameW + (typeW ? 28 + typeW : 0) + PAD, titleW + PAD * 2, 200);
    var h = cols.length ? HDR + 6 + cols.length * ROW + 8 : HDR;
    var els = [];
    els.push(rect(ox, oy, w, h, { stroke: kind === 'table' ? T.border : color, fill: T.surface, round: 10, dash: kind === 'view' ? 'dashed' : 'solid', sw: 1 }));
    if (kind === 'weak') els.push(rect(ox + 4, oy + 4, w - 8, h - 8, { stroke: color, round: 7 }));
    els.push(rect(ox + PAD, oy + 15, 3, 14, { fill: color }));
    els.push(text(ox + PAD + 11, oy + 10, d.name, { size: 19, fam: HEAD, color: T.text }));
    els.push(text(ox + w - PAD, oy + 16, label, { size: 11, color: color, align: 'right' }));
    if (cols.length) els.push(line(ox, oy + HDR, [[0, 0], [w, 0]], { stroke: T.divider }));
    cols.forEach(function (c, i) {
      var ry = oy + HDR + 6 + i * ROW;
      if (c.k) els.push(text(ox + PAD, ry + 7, c.k, { size: 11, color: c.k === 'FK' ? T.fk : c.k === 'UQ' ? T.muted : T.pk }));
      els.push(text(ox + PAD + keyW, ry + 5, c.n, { size: 15, color: c.k === 'PK' ? T.pk : T.text }));
      if (c.t) els.push(text(ox + w - PAD, ry + 6, c.t, { size: 13, color: T.muted, align: 'right' }));
    });
    var g = group(els);
    g.box = { x: ox, y: oy, w: w, h: h };
    return g;
  }
  function rowItem(T, c) {
    var els = [rect(0, 0, 280, ROW, { fill: T.surface, round: 6 })];
    if (c.k) els.push(text(PAD, 7, c.k, { size: 11, color: c.k === 'FK' ? T.fk : c.k === 'UQ' ? T.muted : T.pk }));
    els.push(text(PAD + 34, 5, c.n, { size: 15, color: c.k === 'PK' ? T.pk : T.text }));
    els.push(text(280 - PAD, 6, c.t, { size: 13, color: T.muted, align: 'right' }));
    return group(els);
  }
  function connector(T, start, end, label, zeroStart, zeroEnd) {
    var L = 240, els = [line(0, 0, [[0, 0], [L, 0]], { arrow: true, stroke: T.rel, sw: 2, start: start, end: end })];
    if (zeroStart) els.push(shape('ellipse', 26, -6, 12, 12, { stroke: T.rel, fill: T.bg, sw: 2 }));
    if (zeroEnd) els.push(shape('ellipse', L - 38, -6, 12, 12, { stroke: T.rel, fill: T.bg, sw: 2 }));
    if (label) els.push(text(L / 2, -26, label, { size: 13, color: T.muted, align: 'center' }));
    return group(els);
  }
  function chen(T, type, label, o) {
    o = o || {};
    var els = [], w = o.w || 170, h = o.h || 70;
    var st = o.stroke || T.title;
    if (type === 'rectangle') {
      els.push(rect(0, 0, w, h, { stroke: st, fill: T.surface, round: 8, sw: 2 }));
      if (o.double) els.push(rect(6, 6, w - 12, h - 12, { stroke: st, round: 5, sw: 1 }));
    } else if (type === 'diamond') {
      els.push(shape('diamond', 0, 0, w, h, { stroke: st, fill: T.surface, sw: 2 }));
      if (o.double) els.push(shape('diamond', 14, 8, w - 28, h - 16, { stroke: st, sw: 1 }));
    } else {
      els.push(shape('ellipse', 0, 0, w, h, { stroke: st, fill: T.surface, sw: o.dash ? 1.5 : 2, dash: o.dash }));
      if (o.double) els.push(shape('ellipse', 6, 6, w - 12, h - 12, { stroke: st, sw: 1 }));
    }
    var t = text(w / 2, h / 2 - 10, label, { size: 16, fam: HEAD, color: T.text, align: 'center' });
    els.push(t);
    if (o.underline) els.push(line(t.x, t.y + t.height + 1, [[0, 0], [t.width, 0]], { stroke: o.stroke || T.pk, sw: 1.5 }));
    return group(els);
  }

  function build(key) {
    s = { wave: 11, dragon: 23, lotus: 37 }[key];
    var T = THEMES[key], items = [];
    function add(section, name, els, note) { items.push({ section: section, name: name, note: note || '', elements: els }); }

    add('Entities', 'Table', entity(T, { name: 'users', cols: [{ k: 'PK', n: 'id', t: 'uuid' }, { k: 'UQ', n: 'email', t: 'text' }, { n: 'display_name', t: 'text' }, { n: 'created_at', t: 'timestamptz' }] }));
    add('Entities', 'Table with foreign keys', entity(T, { name: 'orders', cols: [{ k: 'PK', n: 'id', t: 'uuid' }, { k: 'FK', n: 'user_id', t: 'uuid' }, { n: 'status', t: 'order_status' }, { n: 'total_cents', t: 'int8' }, { n: 'placed_at', t: 'timestamptz' }] }));
    add('Entities', 'Join table', entity(T, { name: 'user_roles', kind: 'junction', cols: [{ k: 'PK', n: 'user_id', t: 'uuid' }, { k: 'PK', n: 'role_id', t: 'uuid' }, { n: 'granted_at', t: 'timestamptz' }] }), 'Composite key');
    add('Entities', 'Weak entity', entity(T, { name: 'order_lines', kind: 'weak', cols: [{ k: 'FK', n: 'order_id', t: 'uuid' }, { k: 'PK', n: 'line_no', t: 'int4' }, { n: 'quantity', t: 'int4' }] }), 'Depends on its owner');
    add('Entities', 'View', entity(T, { name: 'active_users', kind: 'view', cols: [{ n: 'id', t: 'uuid' }, { n: 'email', t: 'text' }, { n: 'last_seen', t: 'timestamptz' }] }));
    add('Entities', 'Enum', entity(T, { name: 'order_status', kind: 'enum', cols: [{ n: 'pending' }, { n: 'paid' }, { n: 'shipped' }, { n: 'cancelled' }] }));
    add('Entities', 'Collapsed table', entity(T, { name: 'products', cols: [] }), 'Header only');

    add('Columns', 'Primary key', rowItem(T, { k: 'PK', n: 'id', t: 'uuid' }));
    add('Columns', 'Foreign key', rowItem(T, { k: 'FK', n: 'parent_id', t: 'uuid' }));
    add('Columns', 'Unique', rowItem(T, { k: 'UQ', n: 'slug', t: 'text' }));
    add('Columns', 'Column', rowItem(T, { n: 'column_name', t: 'type' }));

    add("Crow's foot", 'One to one', connector(T, 'crowfoot_one', 'crowfoot_one', 'has'));
    add("Crow's foot", 'One to many', connector(T, 'crowfoot_one', 'crowfoot_many', 'places'));
    add("Crow's foot", 'One to one-or-many', connector(T, 'crowfoot_one', 'crowfoot_one_or_many', 'contains'));
    add("Crow's foot", 'One to zero-or-many', connector(T, 'crowfoot_one', 'crowfoot_many', 'owns', false, true));
    add("Crow's foot", 'Zero-or-one to one', connector(T, 'crowfoot_one', 'crowfoot_one', 'profile', true, false));
    add("Crow's foot", 'Many to many', connector(T, 'crowfoot_many', 'crowfoot_many', 'tagged'));

    add('Chen', 'Entity', chen(T, 'rectangle', 'Customer'));
    add('Chen', 'Weak entity', chen(T, 'rectangle', 'Dependent', { double: true, stroke: T.weak }));
    add('Chen', 'Relationship', chen(T, 'diamond', 'Places', { stroke: T.view, w: 150, h: 84 }));
    add('Chen', 'Identifying relationship', chen(T, 'diamond', 'Has', { stroke: T.weak, w: 150, h: 84, double: true }));
    add('Chen', 'Attribute', chen(T, 'ellipse', 'name', { stroke: T.fk, w: 140, h: 56 }));
    add('Chen', 'Key attribute', chen(T, 'ellipse', 'customer_id', { stroke: T.pk, w: 160, h: 56, underline: true }));
    add('Chen', 'Multivalued attribute', chen(T, 'ellipse', 'phone', { stroke: T.fk, w: 140, h: 56, double: true }));
    add('Chen', 'Derived attribute', chen(T, 'ellipse', 'age', { stroke: T.muted, w: 140, h: 56, dash: 'dashed' }));

    var note = group([rect(0, 0, 240, 84, { fill: T.noteBg, round: 8 }), text(14, 12, 'Soft-deleted rows keep\ndeleted_at; filter them\nin every read path.', { size: 14, color: T.noteText })]);
    add('Annotation', 'Note', note);
    var schema = group([rect(0, 0, 560, 360, { stroke: T.border, dash: 'dashed', round: 14 }), text(18, 14, 'schema · public', { size: 13, color: T.muted })]);
    add('Annotation', 'Schema boundary', schema, 'Drop tables inside');
    var lg = [rect(0, 0, 300, 196, { stroke: T.divider, fill: T.surface, round: 10 }), text(PAD, 14, 'Cardinality', { size: 16, fam: HEAD, color: T.text })];
    [['crowfoot_one', false, 'exactly one'], ['crowfoot_one', true, 'zero or one'], ['crowfoot_one_or_many', false, 'one or many'], ['crowfoot_many', true, 'zero or many']].forEach(function (r, i) {
      var y = 60 + i * 34;
      lg.push(line(PAD, y, [[0, 0], [110, 0]], { arrow: true, stroke: T.rel, sw: 2, end: r[0] }));
      if (r[1]) lg.push(shape('ellipse', PAD + 110 - 38, y - 6, 12, 12, { stroke: T.rel, fill: T.surface, sw: 2 }));
      lg.push(text(PAD + 134, y - 10, r[2], { size: 14, color: T.muted }));
    });
    add('Annotation', 'Legend', group(lg));

    var u = entity(T, { name: 'users', cols: [{ k: 'PK', n: 'id', t: 'uuid' }, { k: 'UQ', n: 'email', t: 'text' }, { n: 'created_at', t: 'timestamptz' }] }, 0, 28);
    var o = entity(T, { name: 'orders', cols: [{ k: 'PK', n: 'id', t: 'uuid' }, { k: 'FK', n: 'user_id', t: 'uuid' }, { n: 'status', t: 'order_status' }, { n: 'placed_at', t: 'timestamptz' }] }, u.box.w + 120, 0);
    var ox = o.box.x + o.box.w + 120;
    var li = entity(T, { name: 'order_lines', kind: 'weak', cols: [{ k: 'PK', n: 'id', t: 'uuid' }, { k: 'FK', n: 'order_id', t: 'uuid' }, { k: 'FK', n: 'product_id', t: 'uuid' }, { n: 'quantity', t: 'int4' }] }, ox, -28);
    var p = entity(T, { name: 'products', cols: [{ k: 'PK', n: 'id', t: 'uuid' }, { n: 'sku', t: 'text' }, { n: 'price_cents', t: 'int8' }] }, ox, li.box.y + li.box.h + 110);
    var rowY = function (b, i) { return b.y + HDR + 6 + i * ROW + ROW / 2; };
    function bind(a, from, to) {
      a.startBinding = { elementId: from[0].id, focus: 0, gap: 1 }; a.endBinding = { elementId: to[0].id, focus: 0, gap: 1 };
      [from[0], to[0]].forEach(function (r) { r.boundElements = (r.boundElements || []).concat([{ id: a.id, type: 'arrow' }]); });
      return a;
    }
    var y1 = rowY(u.box, 0);
    var a1 = bind(line(u.box.w + 1, y1, [[0, 0], [118, 0]], { arrow: true, stroke: T.rel, sw: 2, start: 'crowfoot_one', end: 'crowfoot_many' }), u, o);
    var y2 = rowY(o.box, 0);
    var a2 = bind(line(o.box.x + o.box.w + 1, y2, [[0, 0], [118, 0]], { arrow: true, stroke: T.rel, sw: 2, start: 'crowfoot_one', end: 'crowfoot_one_or_many' }), o, li);
    var cx = ox + li.box.w / 2;
    var a3 = bind(line(cx, p.box.y - 1, [[0, 0], [0, -(p.box.y - (li.box.y + li.box.h)) + 2]], { arrow: true, stroke: T.rel, sw: 2, start: 'crowfoot_one', end: 'crowfoot_many' }), p, li);
    add('Starter', 'Orders schema', [].concat(u, o, li, p, [a1, a2, a3]), 'Bound connectors');
    return items;
  }

  function toLibrary(key) {
    return {
      type: 'excalidrawlib', version: 2, source: 'https://excalidraw.com',
      libraryItems: build(key).map(function (it, i) {
        return { id: key + '-' + i, status: 'published', created: 1790000000000 + i, name: 'Kanagawa ' + THEMES[key].name + ' · ' + it.name, elements: it.elements };
      })
    };
  }
  function toClipboard(els) { return JSON.stringify({ type: 'excalidraw/clipboard', elements: els, files: {} }); }

  var FONT = { 3: "'Cascadia Code','JetBrains Mono',ui-monospace,monospace", 6: "Nunito,system-ui,sans-serif" };
  function esc(t) { return String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;'); }
  function head(px, py, ux, uy, kind, c, sw) {
    var nx = -uy, ny = ux, out = '';
    function P(d, o) { return (px - ux * d + nx * o).toFixed(1) + ',' + (py - uy * d + ny * o).toFixed(1); }
    function L(a, b) { out += '<line x1="' + a.split(',')[0] + '" y1="' + a.split(',')[1] + '" x2="' + b.split(',')[0] + '" y2="' + b.split(',')[1] + '" stroke="' + c + '" stroke-width="' + sw + '" stroke-linecap="round"/>'; }
    if (kind === 'crowfoot_one') L(P(12, -8), P(12, 8));
    if (kind === 'crowfoot_many' || kind === 'crowfoot_one_or_many') { L(P(16, 0), P(0, -8)); L(P(16, 0), P(0, 8)); }
    if (kind === 'crowfoot_one_or_many') L(P(22, -8), P(22, 8));
    return out;
  }
  function toSVG(els, bg) {
    var x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    els.forEach(function (e) {
      if (e.points) e.points.forEach(function (p) { x0 = Math.min(x0, e.x + p[0] - 10); x1 = Math.max(x1, e.x + p[0] + 10); y0 = Math.min(y0, e.y + p[1] - 10); y1 = Math.max(y1, e.y + p[1] + 10); });
      else { x0 = Math.min(x0, e.x); y0 = Math.min(y0, e.y); x1 = Math.max(x1, e.x + e.width); y1 = Math.max(y1, e.y + e.height); }
    });
    var pad = 20, W = x1 - x0 + pad * 2, H = y1 - y0 + pad * 2, o = '';
    els.forEach(function (e) {
      var x = e.x - x0 + pad, y = e.y - y0 + pad, st = e.strokeColor, fill = e.backgroundColor === 'transparent' ? 'none' : e.backgroundColor;
      var dash = e.strokeStyle === 'dashed' ? ' stroke-dasharray="8 6"' : e.strokeStyle === 'dotted' ? ' stroke-dasharray="2 4"' : '';
      var S = st === 'transparent' ? ' stroke="none"' : ' stroke="' + st + '" stroke-width="' + e.strokeWidth + '"' + dash;
      if (e.type === 'rectangle') {
        var m = Math.min(e.width, e.height), r = e.roundness ? (m <= (e.roundness.value || 32) * 4 ? m * 0.25 : (e.roundness.value || 32)) : 0;
        o += '<rect x="' + x + '" y="' + y + '" width="' + e.width + '" height="' + e.height + '" rx="' + r + '" fill="' + fill + '"' + S + '/>';
      } else if (e.type === 'ellipse') {
        o += '<ellipse cx="' + (x + e.width / 2) + '" cy="' + (y + e.height / 2) + '" rx="' + e.width / 2 + '" ry="' + e.height / 2 + '" fill="' + fill + '"' + S + '/>';
      } else if (e.type === 'diamond') {
        o += '<polygon points="' + (x + e.width / 2) + ',' + y + ' ' + (x + e.width) + ',' + (y + e.height / 2) + ' ' + (x + e.width / 2) + ',' + (y + e.height) + ' ' + x + ',' + (y + e.height / 2) + '" stroke-linejoin="round" fill="' + fill + '"' + S + '/>';
      } else if (e.type === 'text') {
        var anchor = e.textAlign === 'center' ? 'middle' : e.textAlign === 'right' ? 'end' : 'start';
        var tx = e.textAlign === 'center' ? x + e.width / 2 : e.textAlign === 'right' ? x + e.width : x;
        e.text.split('\n').forEach(function (l, i) {
          o += '<text x="' + tx + '" y="' + (y + e.fontSize * (0.98 + 1.25 * i)) + '" fill="' + st + '" font-size="' + e.fontSize + '" font-family="' + FONT[e.fontFamily] + '" text-anchor="' + anchor + '">' + esc(l) + '</text>';
        });
      } else if (e.points) {
        var pts = e.points.map(function (p) { return [x + p[0], y + p[1]]; });
        o += '<polyline points="' + pts.map(function (p) { return p.join(','); }).join(' ') + '" fill="none" stroke-linecap="round"' + S + '/>';
        function end(a, b, k) { if (!k) return; var dx = a[0] - b[0], dy = a[1] - b[1], d = Math.hypot(dx, dy) || 1; o += head(a[0], a[1], dx / d, dy / d, k, st, e.strokeWidth); }
        end(pts[pts.length - 1], pts[pts.length - 2], e.endArrowhead);
        end(pts[0], pts[1], e.startArrowhead);
      }
    });
    return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ' + W + ' ' + H + '" width="' + W + '" height="' + H + '" style="max-width:100%;height:auto;display:block">' + o + '</svg>';
  }
  return { THEMES: THEMES, build: build, toLibrary: toLibrary, toClipboard: toClipboard, toSVG: toSVG };
})();
if (typeof window !== 'undefined') window.KanagawaERD = KanagawaERD;
