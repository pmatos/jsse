#!/usr/bin/env python3
"""Emit a per-phase-instrumented mandreel driver (issue #526).

Reproduces `runMandreel()` (mandreel.js lines 54-73) statement for statement,
with a `Date.now()` marker around each phase, so a run prints one PHASE line
per phase instead of a single total. Regenerable: the 5 MB driver is a build
artifact, never edited by hand.
"""

import argparse
import sys
from pathlib import Path

EPILOGUE = r"""
// ==== per-phase instrumentation driver (issue #526) ====
// Mirrors runMandreel() (mandreel.js:54-73) statement for statement, inside a
// function so every `var` keeps the same function-scope binding kind the real
// runMandreel() gives it. Timing top-level copies of these loops would measure
// global-object lookups the benchmark never performs.
function __mark(label, ms) { console.log("PHASE\t" + label + "\t" + ms); }

// setupMandreel() reaches these four C entry points through
// mandreelAppInit() (mandreel.js:1449). Replacing that body with a timed copy
// attributes them without touching the 5 MB benchmark source.
var __g_globalInit = 0, __g_setRes = 0, __g_internalInit = 0, __g_init = 0;
mandreelAppInit = function () {
  if (mandreelAppPlatform == "webgl" || mandreelAppPlatform == "canvas") {
    var t = Date.now();
    global_init(g_stack_pointer + 800 * 1024);
    __g_globalInit = Date.now() - t;
    var sp = g_stack_pointer + 800 * 1024;
    heapU32[sp >> 2] = mandreelAppCanvasWidth;
    heapU32[(sp + 4) >> 2] = mandreelAppCanvasHeight;
    t = Date.now();
    __mandreel_internal_SetResolution(sp);
    __g_setRes = Date.now() - t;
    t = Date.now();
    __mandreel_internal_init(g_stack_pointer + 800 * 1024);
    __g_internalInit = Date.now() - t;
    t = Date.now();
    __init(g_stack_pointer + 800 * 1024);
    __g_init = Date.now() - t;
  }
};

function __runMandreelPhased() {
  Mandreel_currentTime = 0;
  var sp = g_stack_pointer + 800 * 1024;

  var t = Date.now();
  for (var i = 0; i < mandreel_total_memory / 4; i++) {
    heap32[i] = my_heap32[i];
  }
  __mark("run:heapcopy", Date.now() - t);

  tlsf_ptr = 0;
  heapNewPos = my_heapNewPos;

  t = Date.now();
  my_old_constructors(llvm_2E_global_ctors, 5, sp);
  __mark("run:global_ctors", Date.now() - t);

  heapU32[sp >> 2] = 640;
  heapU32[(sp + 4) >> 2] = 480;

  t = Date.now();
  __mandreel_internal_SetResolution(sp);
  __mark("run:SetResolution", Date.now() - t);

  t = Date.now();
  __mandreel_internal_init(g_stack_pointer + 800 * 1024);
  __mark("run:internal_init", Date.now() - t);

  t = Date.now();
  __init(g_stack_pointer + 800 * 1024);
  __mark("run:__init", Date.now() - t);

  var renderTotal = 0, flushTotal = 0;
  for (var k = 0; k < 20; k++) {
    t = Date.now();
    render();
    var tr = Date.now();
    Mandreel_flushTimeouts();
    updateMandreelStats(performance.now());
    var te = Date.now();
    renderTotal += tr - t;
    flushTotal += te - tr;
    __mark("run:render[" + k + "]", tr - t);
  }
  __mark("run:render_total", renderTotal);
  __mark("run:flush+stats_total", flushTotal);
  Mandreel_checkState();
}

__mark("parse+decls", Date.now() - __t0);

var __tSetup = Date.now();
setupMandreel();
__mark("setupMandreel", Date.now() - __tSetup);
__mark("setup:global_init", __g_globalInit);
__mark("setup:SetResolution", __g_setRes);
__mark("setup:internal_init", __g_internalInit);
__mark("setup:__init", __g_init);

var __tRun = Date.now();
__runMandreelPhased();
__mark("runMandreel_total", Date.now() - __tRun);
__mark("TOTAL", Date.now() - __t0);
"""

SUBPHASE_EPILOGUE = r"""
// ==== render() sub-phase driver (issue #526) ====
// render() (mandreel.js:1800) fans out through mandreelAppDraw()
// (mandreel.js:1815) into three C entry points. Replacing that body with a
// timed copy attributes them separately.
var __preT = 0, __drawT = 0, __updT = 0;
mandreelAppDraw = function (elapsed) {
  var sp = g_stack_pointer + 800 * 1024;
  var t = Date.now();
  __mandreel_internal_preupdate(sp);
  var t1 = Date.now();
  heapU32[sp >> 2] = elapsed;
  __draw(sp);
  var t2 = Date.now();
  __mandreel_internal_update(sp);
  var t3 = Date.now();
  __preT += t1 - t;
  __drawT += t2 - t1;
  __updT += t3 - t2;
};
"""

SUBPHASE_REPORT = r"""
__mark("render:preupdate_total", __preT);
__mark("render:__draw_total", __drawT);
__mark("render:update_total", __updT);
"""


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source", type=Path, help="path to JetStream Octane mandreel.js")
    ap.add_argument("-o", "--out", type=Path, required=True, help="driver to write")
    ap.add_argument(
        "--subphases",
        action="store_true",
        help="also time render()'s three C entry points (preupdate/__draw/update)",
    )
    args = ap.parse_args()

    text = args.source.read_text(encoding="utf-8")
    marker = "// End of mandreel.js file."
    if marker not in text:
        print(f"error: {args.source} has no '{marker}' terminator", file=sys.stderr)
        return 1
    # Cut any driver a previous run appended, keeping the benchmark body only.
    body = text.split(marker)[0] + marker + "\n"
    if "const __t0" not in body:
        body = "const __t0 = Date.now();\n" + body

    parts = [body]
    if args.subphases:
        parts.append(SUBPHASE_EPILOGUE)
    parts.append(EPILOGUE)
    if args.subphases:
        parts.append(SUBPHASE_REPORT)
    args.out.write_text("".join(parts), encoding="utf-8")
    print(f"wrote {args.out} ({args.out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
