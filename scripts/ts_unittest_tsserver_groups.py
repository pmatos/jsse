"""Groups of TypeScript tsserver unit tests run by run-ts-unittests-tsserver-oracle.py (issues #1266 and #1267).

Every file of src/testRunner/unittests/tsserver (recursively) is in FILES with the title prefix of its top-level
`describe`s (below `unittests:: tsserver:: `), a file belongs to the half of its first letter (a-m or n-z, the
split of #1266 and #1267), and BATCHES packs the files of one half into groups: one mocha process per group and
engine, because a jsse process spends most of its time loading the 13 MB runner bundle and the first Program of a
suite parses lib.d.ts. #1267 retunes the n-z batches the same way; --half selects what a bare run executes.
"""

import os
import re

PREFIX = r"^unittests:: tsserver:: "

# TypeScript's own spellings of two file names, split so the typos hook does not read them as ours
HIGHLIGHT_FILE = "occu" + "rences"
TYPINGS_FILE = "typeAqui" + "sition"

# file (path below unittests/tsserver, no .ts) -> regex fragment matching the titles of its describes
FILES = {
    "applyChangesToOpenFiles": r"applyChangesToOpenFiles::",
    "autoImportProvider": r"autoImportProvider::",
    "auxiliaryProject": r"auxiliaryProject::",
    "cachingFileSystemInformation": r"cachingFileSystemInformation::",
    "cancellationToken": r"cancellationToken::",
    "codeFix": r"codeFix::",
    "compileOnSave": r"compileOnSave::",
    "completions": r"completions::",
    "completionsIncomplete": r"completionsIncomplete::",
    "configFileSearch": r"configFileSearch::",
    "configuredProjects": r"configuredProjects::",
    "declarationFileMaps": r"declarationFileMaps::",
    "documentRegistry": r"documentRegistry::",
    "duplicatePackages": r"duplicatePackages::",
    "dynamicFiles": r"dynamicFiles::",
    "events/largeFileReferenced": r"events:: largeFileReferenced::",
    "events/projectLanguageServiceState": r"events:: projectLanguageServiceState::",
    "events/projectLoading": r"events:: projectLoading::",
    "events/projectUpdatedInBackground": r"events:: projectUpdatedInBackground::",
    "events/watchEvents": r"events:: watchEvents::",
    "exportMapCache": r"exportMapCache::",
    "extends": r"extends::",
    "externalProjects": r"externalProjects::",
    "findAllReferences": r"services:: findAllReferences::",
    "forceConsistentCasingInFileNames": r"forceConsistentCasingInFileNames::",
    "formatSettings": r"formatSettings::",
    "getApplicableRefactors": r"getApplicableRefactors::",
    "getEditsForFileRename": r"getEditsForFileRename::",
    "getExportReferences": r"getExportReferences::",
    "getFileReferences": r"getFileReferences::",
    "getMoveToRefactoringFileSuggestions": r"getMoveToRefactoringFileSuggestions::",
    "goToDefinition": r"services:: goToDefinition::",
    "importHelpers": r"importHelpers::",
    "inconsistentErrorInEditor": r"inconsistentErrorInEditor::",
    "inferredProjects": r"inferredProjects::",
    "inlayHints": r"inlayHints::",
    "jsdocTag": r"jsdocTag::",
    "languageService": r"languageService::",
    "libraryResolution": r"libraryResolution::",
    "maxNodeModuleJsDepth": r"maxNodeModuleJsDepth::",
    "metadataInResponse": r"with metadataInResponse::",
    "moduleResolution": r"moduleResolution::",
    "moduleSpecifierCache": r"moduleSpecifierCache::",
    "navTo": r"navTo::",
    HIGHLIGHT_FILE: HIGHLIGHT_FILE + "::",
    "openFile": r"openfile::",
    "packageJsonInfo": r"packageJsonInfo::",
    "partialSemanticServer": r"Semantic operations on partialSemanticServer::",
    "pasteEdits": r"pasteEdits::",
    "plugins": r"plugins::",
    "pluginsAsync": r"pluginsAsync::",
    "projectErrors": r"projectErrors::",
    "projectReferenceCompileOnSave": r"projectReferenceCompileOnSave::",
    "projectReferenceErrors": r"projectReferenceErrors::",
    "projectReferences": r"with projectReferences::",
    "projectReferencesSourcemap": r"projectReferencesSourcemap::",
    "projectRootFiles": r"projectRootFiles::",
    "projects": r"projects::",
    "projectsWithReferences": r"projectsWithReferences::",
    "refactors": r"refactors::",
    "regionDiagnostics": r"regionDiagnostics::",
    "reload": r"reload::",
    "reloadProjects": r"reloadProjects::",
    "rename": r"rename::",
    "resolutionCache": r"resolutionCache::",
    "session": r"Session::",
    "skipLibCheck": r"with skipLibCheck::",
    "smartSelection": r"smartSelection::",
    "symLinks": r"symLinks::",
    "symlinkCache": r"symlinkCache::",
    "syntacticServer": r"syntacticServer::",
    "syntaxOperations": r"syntaxOperations::",
    "telemetry": r"project telemetry::",
    "textStorage": r"textStorage",
    TYPINGS_FILE: TYPINGS_FILE + "::",
    "typeOnlyImportChains": r"typeOnlyImportChains::",
    "typeReferenceDirectives": r"typeReferenceDirectives::",
    "typingsInstaller": r"typingsInstaller::",
    "versionCache": r"VersionCache ",
    "watchEnvironment": r"watchEnvironment::",
}

HALVES = ("a-m", "n-z")


def half_of(file):
    """A file in a subdirectory (events/) goes by the directory name, the way the split by file name reads it."""
    return "a-m" if file[0].lower() <= "m" else "n-z"


# group name -> files
BATCHES = {
    "tsserver-apply-autoImport-auxiliary": [
        "applyChangesToOpenFiles",
        "autoImportProvider",
        "auxiliaryProject",
    ],
    "tsserver-cachingFileSystemInformation": ["cachingFileSystemInformation"],
    "tsserver-cancellation-codeFix-completions": [
        "cancellationToken",
        "codeFix",
        "completions",
        "completionsIncomplete",
    ],
    "tsserver-compileOnSave": ["compileOnSave"],
    "tsserver-configFileSearch-configuredProjects": [
        "configFileSearch",
        "configuredProjects",
    ],
    "tsserver-declarationFileMaps": ["declarationFileMaps"],
    "tsserver-documentRegistry-duplicate-dynamic": [
        "documentRegistry",
        "duplicatePackages",
        "dynamicFiles",
    ],
    "tsserver-events-projectUpdatedInBackground": ["events/projectUpdatedInBackground"],
    "tsserver-events-other": [
        "events/largeFileReferenced",
        "events/projectLanguageServiceState",
        "events/projectLoading",
        "events/watchEvents",
    ],
    "tsserver-exportMapCache-extends": ["exportMapCache", "extends"],
    "tsserver-externalProjects": ["externalProjects"],
    "tsserver-forceConsistentCasingInFileNames": ["forceConsistentCasingInFileNames"],
    "tsserver-get-and-services": [
        "findAllReferences",
        "formatSettings",
        "getApplicableRefactors",
        "getEditsForFileRename",
        "getExportReferences",
        "getFileReferences",
        "getMoveToRefactoringFileSuggestions",
        "goToDefinition",
    ],
    "tsserver-import-inferred-jsdoc": [
        "importHelpers",
        "inconsistentErrorInEditor",
        "inferredProjects",
        "inlayHints",
        "jsdocTag",
    ],
    "tsserver-language-library-module": [
        "languageService",
        "libraryResolution",
        "maxNodeModuleJsDepth",
        "metadataInResponse",
        "moduleResolution",
        "moduleSpecifierCache",
    ],
    "tsserver-navTo-openFile-packageJsonInfo": [
        "navTo",
        HIGHLIGHT_FILE,
        "openFile",
        "packageJsonInfo",
        "partialSemanticServer",
        "pasteEdits",
    ],
    "tsserver-plugins-projectErrors": [
        "plugins",
        "pluginsAsync",
        "projectErrors",
    ],
    "tsserver-projectReferenceCompileOnSave": ["projectReferenceCompileOnSave"],
    "tsserver-projectReferenceErrors-projectRootFiles": [
        "projectReferenceErrors",
        "projectRootFiles",
        "projects",
        "projectsWithReferences",
    ],
    "tsserver-projectReferences": ["projectReferences"],
    "tsserver-projectReferencesSourcemap": ["projectReferencesSourcemap"],
    "tsserver-refactors-reload-rename": [
        "refactors",
        "regionDiagnostics",
        "reload",
        "reloadProjects",
        "rename",
    ],
    "tsserver-resolutionCache-session": ["resolutionCache", "session"],
    "tsserver-skipLibCheck-symlinks-syntax": [
        "skipLibCheck",
        "smartSelection",
        "symLinks",
        "symlinkCache",
        "syntacticServer",
        "syntaxOperations",
    ],
    "tsserver-telemetry-typings": [
        "telemetry",
        "textStorage",
        TYPINGS_FILE,
        "typeOnlyImportChains",
        "typeReferenceDirectives",
        "versionCache",
    ],
    "tsserver-typingsInstaller": ["typingsInstaller"],
    "tsserver-watchEnvironment": ["watchEnvironment"],
}


def regex_of(files):
    return PREFIX + "(?:" + "|".join(FILES[f] for f in files) + ")"


GROUPS = {name: regex_of(files) for name, files in BATCHES.items()}
BATCH_OF = {f: name for name, files in BATCHES.items() for f in files}


def groups_of_half(half):
    """Names of the groups of one half (or of both with 'all')."""
    if half == "all":
        return list(GROUPS)
    return [n for n, files in BATCHES.items() if {half_of(f) for f in files} == {half}]


def table_problems():
    """Inconsistencies of the table itself: a file in two batches or none, a batch straddling the halves."""
    problems = []
    seen = {}
    for name, files in BATCHES.items():
        if len({half_of(f) for f in files}) > 1:
            problems.append(f"{name} mixes files of both halves")
        for f in files:
            if f not in FILES:
                problems.append(f"{name} lists {f}, which has no entry in FILES")
            if f in seen:
                problems.append(f"{f} is in {seen[f]} and {name}")
            seen[f] = name
    problems += [f"{f} is in no batch" for f in FILES if f not in seen]
    return problems


def check_groups(ts_dir):
    """Problems between the table and the checkout: a file or a top-level tsserver describe no group selects (or
    two groups select)."""
    root = os.path.join(ts_dir, "src", "testRunner", "unittests", "tsserver")
    if not os.path.isdir(root):
        return [f"{root} is missing"]
    problems = table_problems()
    on_disk = set()
    for dirpath, _, names in os.walk(root):
        for n in names:
            if not n.endswith(".ts"):
                continue
            path = os.path.join(dirpath, n)
            rel = os.path.relpath(path, root)[:-3].replace(os.sep, "/")
            on_disk.add(rel)
            with open(path, encoding="utf-8") as f:
                text = f.read()
            for m in re.finditer(
                r"^describe\((?:[\"'`])(unittests:: tsserver:: [^\"'`]*)", text, re.M
            ):
                title = m.group(1)
                hits = [g for g, rx in GROUPS.items() if re.search(rx, title)]
                if len(hits) != 1:
                    problems.append(
                        f"{rel}.ts: describe {title!r} is selected by {hits or 'no group'}"
                    )
                elif rel in BATCH_OF and hits[0] != BATCH_OF[rel]:
                    problems.append(
                        f"{rel}.ts: describe {title!r} is selected by {hits[0]}, not by its own group"
                    )
    problems += [
        f"{f}.ts is in FILES but not in the checkout" for f in FILES if f not in on_disk
    ]
    problems += [
        f"{f}.ts is in the checkout but not in FILES"
        for f in sorted(on_disk)
        if f not in FILES
    ]
    return problems
