/* openplace anti-automation collector (self-hosted, no third parties).
   Collects coarse browser traits, hashes them client-side where possible;
   the server only stores derived hashes + coarse fields. */
(function () {
  "use strict";
  if (window.__opFp) return; window.__opFp = true;

  function sha256Hex(str) {
    if (!(window.crypto && crypto.subtle)) return Promise.resolve("unavailable");
    var bytes = new TextEncoder().encode(str);
    return crypto.subtle.digest("SHA-256", bytes).then(function (buf) {
      return Array.from(new Uint8Array(buf)).map(function (b) { return b.toString(16).padStart(2, "0"); }).join("");
    }).catch(function () { return "unavailable"; });
  }

  function canvasHash() {
    try {
      var c = document.createElement("canvas"); c.width = 240; c.height = 60;
      var g = c.getContext("2d"); if (!g) return "no-canvas";
      g.textBaseline = "top"; g.font = "16px 'Arial'"; g.fillStyle = "#f60";
      g.fillRect(2, 2, 90, 18); g.fillStyle = "#069";
      g.fillText("openplace-fp-\u2713 \u4e2d\u6587", 4, 8);
      g.strokeStyle = "rgba(102,204,0,0.7)"; g.beginPath(); g.arc(180, 30, 24, 0, 6.3); g.stroke();
      return c.toDataURL();
    } catch (e) { return "error"; }
  }

  function webglInfo() {
    try {
      var c = document.createElement("canvas");
      var gl = c.getContext("webgl") || c.getContext("experimental-webgl");
      if (!gl) return "no-webgl";
      var ext = gl.getExtension("WEBGL_debug_renderer_info");
      var vendor = ext ? gl.getParameter(ext.UNMASKED_VENDOR_WEBGL) : "hidden";
      var renderer = ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : "hidden";
      return String(vendor) + "|" + String(renderer) + "|maxtex" + gl.getParameter(gl.MAX_TEXTURE_SIZE);
    } catch (e) { return "error"; }
  }

  function audioHash() {
    return new Promise(function (resolve) {
      try {
        var Ctx = window.OfflineAudioContext || window.webkitOfflineAudioContext;
        if (!Ctx) return resolve("no-audio");
        var ctx = new Ctx(1, 44100, 44100);
        var osc = ctx.createOscillator(); osc.type = "triangle"; osc.frequency.value = 10000;
        var comp = ctx.createDynamicsCompressor();
        osc.connect(comp); comp.connect(ctx.destination); osc.start(0);
        ctx.startRendering().then(function (buf) {
          var d = buf.getChannelData(0); var sum = 0;
          for (var i = 4500; i < 5000; i++) sum += Math.abs(d[i]);
          resolve(String(sum));
        }).catch(function () { resolve("error"); });
      } catch (e) { resolve("error"); }
    });
  }

  function fontProbe() {
    try {
      var canvas = document.createElement("canvas"); var g = canvas.getContext("2d");
      var base = "72px monospace"; var families = ["Arial", "Times New Roman", "Georgia", "Verdana", "Courier New", "Comic Sans MS", "Impact"];
      g.font = base; var w0 = g.measureText("openplace0123").width; var found = 0;
      families.forEach(function (f) {
        g.font = "72px '" + f + "', monospace";
        if (Math.abs(g.measureText("openplace0123").width - w0) > 0.5) found++;
      });
      return String(found);
    } catch (e) { return "error"; }
  }

  var behavioral = { moves: 0, keys: 0, intervals: [] };
  var lastMove = 0;
  document.addEventListener("mousemove", function () {
    behavioral.moves++;
    var now = Date.now();
    if (lastMove && now - lastMove < 5000) behavioral.intervals.push(now - lastMove);
    if (behavioral.intervals.length > 40) behavioral.intervals.shift();
    lastMove = now;
  }, { passive: true });
  document.addEventListener("keydown", function () { behavioral.keys++; }, { passive: true });

  function traits() {
    var n = navigator, s = screen;
    return {
      ua: n.userAgent, platform: n.platform || "", languages: (n.languages || [n.language || ""]).join(","),
      tz: Intl.DateTimeFormat().resolvedOptions().timeZone || "",
      screen: s.width + "x" + s.height + "x" + s.colorDepth,
      mem: n.deviceMemory || 0, cores: n.hardwareConcurrency || 0,
      touch: (n.maxTouchPoints || 0), dnt: n.doNotTrack || "",
      webdriver: n.webdriver ? "true" : "false",
      plugins: n.plugins ? n.plugins.length : -1,
      chrome: !!window.chrome, cdc: !!(window.cdc_adoQpoasnfa76pfcZLmcfl_Array || window.cdc_adoQpoasnfa76pfcZLmcfl_Promise),
      phone: /Android|iPhone|iPad|Mobile/i.test(n.userAgent) ? "1" : "0"
    };
  }

  function varianceSignal(arr) {
    if (!arr || arr.length < 6) return "insufficient";
    var mean = arr.reduce(function (a, b) { return a + b; }, 0) / arr.length;
    var v = arr.reduce(function (a, b) { return a + (b - mean) * (b - mean); }, 0) / arr.length;
    return mean > 0 ? String(v / (mean * mean)) : "insufficient";
  }

  function collect() {
    var t = traits();
    Promise.all([sha256Hex(canvasHash()), sha256Hex(webglInfo()), audioHash()]).then(function (h) {
      var payload = {
        traits: Object.assign({}, t, {
          canvas: h[0].slice(0, 16), webgl: h[1].slice(0, 16),
          audio: h[2].slice(0, 16), fonts: fontProbe(),
          math: String((3.14 * Math.PI) % 1e-6) // JS engine nuance
        }),
        behavioral: {
          moves: behavioral.moves, keys: behavioral.keys,
          moveIntervalVar: varianceSignal(behavioral.intervals)
        }
      };
      fetch("/fp/collect", {
        method: "POST", credentials: "include", keepalive: true,
        headers: { "Content-Type": "text/plain" },
        body: JSON.stringify(payload)
      }).catch(function () {});
    });
  }

  if (document.readyState === "complete") setTimeout(collect, 1500);
  else window.addEventListener("load", function () { setTimeout(collect, 1500); });
})();
