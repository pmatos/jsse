"""Slice rows of TypeScript's `conformance` runner for the module directories (issue #1283).

Suites, `dir:` terms and the pin lookup are those of scripts/ts_conformance_slices_1275.py; this module only adds the
rows (`--slices-module ts_conformance_slices_1283`). The group is moduleResolution, node, externalModules,
internalModules, importAssertion, importAttributes, importDefer, dynamicImport and nonjsExtensions, cut into four chunks
so that a crashed or interrupted one loses nothing: `mr` (moduleResolution and the importAssertion, importAttributes,
importDefer and nonjsExtensions directories), `node`, `ext` (externalModules) and `im` (internalModules and
dynamicImport). ts-suite/oracle.json pins the passing tests per directory; `issue1283` is the nine directories whole.
"""

MR_DIRS = (
    "moduleResolution",
    "importAssertion",
    "importAttributes",
    "importDefer",
    "nonjsExtensions",
)
NODE_DIRS = ("node",)
EXT_DIRS = ("externalModules",)
IM_DIRS = ("internalModules", "dynamicImport")
DIRS = MR_DIRS + NODE_DIRS + EXT_DIRS + IM_DIRS

CHUNK_DIRS = {"mr": MR_DIRS, "node": NODE_DIRS, "ext": EXT_DIRS, "im": IM_DIRS}
CHUNKS = {name: "dir:" + ",".join(dirs) for name, dirs in CHUNK_DIRS.items()}

SLICES = {
    "issue1283": "dir:" + ",".join(DIRS),
    **{d: f"dir:{d}" for d in DIRS},
    **CHUNKS,
}
