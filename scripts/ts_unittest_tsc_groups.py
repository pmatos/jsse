"""Groups of TypeScript `tsc` CLI unit tests run by run-ts-unittests-tsc-oracle.py (issue #1263).

One group per file of src/testRunner/unittests/tsc/ (titles `unittests:: tsc:: <file>:: ...`). The tests drive
`ts.executeCommandLine` through the virtual file system and host of unittests/helpers/tsc.ts and compare the
emitted baselines. tscWatch (`unittests:: tscWatch::`) and tsbuild are other tickets.
"""

GROUPS = {
    "tsc-cancellationToken": r"^unittests:: tsc:: builder cancellationToken::",
    "tsc-commandLine": r"^unittests:: tsc:: commandLine::",
    "tsc-composite": r"^unittests:: tsc:: composite::",
    "tsc-declarationEmit": r"^unittests:: tsc:: declarationEmit::",
    "tsc-extends": r"^unittests:: tsc:: extends::",
    "tsc-forceConsistentCasingInFileNames": r"^unittests:: tsc:: forceConsistentCasingInFileNames::",
    "tsc-ignoreConfig": r"^unittests:: tsc:: ignoreConfig::",
    "tsc-incremental": r"^unittests:: tsc:: incremental::",
    "tsc-libraryResolution": r"^unittests:: tsc:: libraryResolution::",
    "tsc-listFilesOnly": r"^unittests:: tsc:: listFilesOnly::",
    "tsc-moduleResolution": r"^unittests:: tsc:: moduleResolution::",
    "tsc-noCheck": r"^unittests:: tsc:: noCheck::",
    "tsc-noEmit": r"^unittests:: tsc:: noEmit::",
    "tsc-noEmitOnError": r"^unittests:: tsc:: noEmitOnError::",
    "tsc-projectReferences": r"^unittests:: tsc:: projectReferences::",
    "tsc-projectReferencesConfig": r"^unittests:: tsc:: projectReferencesConfig::",
    "tsc-redirect": r"^unittests:: tsc:: redirect::",
}
