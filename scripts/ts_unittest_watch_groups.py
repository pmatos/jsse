"""Groups of TypeScript tscWatch and tsbuildWatch unit tests run by run-ts-unittests-watch-oracle.py (issue #1265).

One group per file of src/testRunner/unittests/tscWatch/ (titles `unittests:: tscWatch:: ...`) and of
unittests/tsbuildWatch/ (`unittests:: tsbuildWatch:: ...`). The tests drive `ts.createWatchProgram` and
`ts.createSolutionBuilderWithWatch` on the virtual host of unittests/helpers/virtualFileSystemWithWatch.ts, whose
timers and file-system events are virtual (no real fs.watch), and compare emitted baselines, so they are CPU-bound.
`watchAPI::` is shared by watchApi.ts and sourceOfProjectReferenceRedirect.ts and told apart by its suffix.
"""

TSC_WATCH = r"^unittests:: tscWatch:: "
TSBUILD_WATCH = r"^unittests:: tsbuildWatch:: "

GROUPS = {
    "tscWatch-consoleClearing": TSC_WATCH + r"consoleClearing::",
    "tscWatch-emit": TSC_WATCH + r"emit::",
    "tscWatch-emitAndErrorUpdates": TSC_WATCH + r"emitAndErrorUpdates::",
    "tscWatch-extends": TSC_WATCH + r"extends::",
    "tscWatch-forceConsistentCasingInFileNames": TSC_WATCH
    + r"forceConsistentCasingInFileNames::",
    "tscWatch-incremental": TSC_WATCH + r"incremental::",
    "tscWatch-libraryResolution": TSC_WATCH + r"libraryResolution::",
    "tscWatch-listFilesOnly": TSC_WATCH + r"listFilesOnly::",
    "tscWatch-moduleResolution": TSC_WATCH + r"moduleResolution::",
    "tscWatch-nodeNextWatch": TSC_WATCH + r"nodeNextWatch::",
    "tscWatch-noEmit": TSC_WATCH + r"noEmit::",
    "tscWatch-noEmitOnError": TSC_WATCH + r"noEmitOnError::",
    "tscWatch-programUpdates": TSC_WATCH + r"programUpdates::",
    "tscWatch-projectsWithReferences": TSC_WATCH + r"projectsWithReferences::",
    "tscWatch-resolutionCache": TSC_WATCH + r"resolutionCache::",
    "tscWatch-resolveJsonModule": TSC_WATCH + r"resolveJsonModule::",
    "tscWatch-sourceOfProjectReferenceRedirect": TSC_WATCH
    + r"watchAPI:: with sourceOfProjectReferenceRedirect::",
    "tscWatch-symlinks": TSC_WATCH + r"symlinks::",
    "tscWatch-watchApi": TSC_WATCH
    + r"watchAPI:: (?!with sourceOfProjectReferenceRedirect::)",
    "tscWatch-watchEnvironment": TSC_WATCH + r"watchEnvironment::",
    "tsbuildWatch-configFileErrors": TSBUILD_WATCH + r"watchMode:: configFileErrors::",
    "tsbuildWatch-demo": TSBUILD_WATCH + r"watchMode:: with demo::",
    "tsbuildWatch-extends": TSBUILD_WATCH + r"watchMode:: extends::",
    "tsbuildWatch-libraryResolution": TSBUILD_WATCH
    + r"watchMode:: libraryResolution::",
    "tsbuildWatch-moduleResolution": TSBUILD_WATCH + r"watchMode:: moduleResolution::",
    "tsbuildWatch-noEmit": TSBUILD_WATCH + r"watchMode:: with noEmit::",
    "tsbuildWatch-noEmitOnError": TSBUILD_WATCH + r"watchMode:: with noEmitOnError::",
    "tsbuildWatch-programUpdates": TSBUILD_WATCH + r"watchMode:: programUpdates::",
    "tsbuildWatch-projectsBuilding": TSBUILD_WATCH + r"watchMode:: projectsBuilding::",
    "tsbuildWatch-publicApi": TSBUILD_WATCH + r"watchMode:: publicAPI::",
    "tsbuildWatch-reexport": TSBUILD_WATCH + r"watchMode:: reexport::",
    "tsbuildWatch-roots": TSBUILD_WATCH + r"watchMode:: roots::",
    "tsbuildWatch-watchEnvironment": TSBUILD_WATCH + r"watchEnvironment::",
}
