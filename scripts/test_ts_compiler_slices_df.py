import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as slices  # noqa: E402
import ts_compiler_slices_df as df  # noqa: E402

FILES = (
    "DeclarationErrorsNoEmitOnError.ts ExportAssignment7.ts FunctionDeclaration3.ts "
    "dateAndMathOperations.ts debugger.ts decoratorMetadata.ts decrementAndIncrementOperators.ts "
    "declFileTypeAnnotationVisibilityErrorAccessors.ts declarationEmitAliasFromIndirectFile.ts "
    "declarationEmitDefaultExport1.ts declarationEmitNameConflicts.ts declarationEmitUsingTypeAlias1.ts "
    "declarationFiles.ts declareDottedExtend.ts defaultArgsInFunctionExpressions.ts dottedModuleName.ts "
    "dtsEmit.ts duplicateIdentifier.ts dynamicNames.ts eachMethodSuggests.ts elidedImport.ts emitThis.ts "
    "enumBasics.ts errorOnInitializerInInterfaceProperty.ts es5ExportDefaultClassDeclaration.ts "
    "es6ClassTest.ts esDecorators.ts esModuleInterop.ts evalAfter.ts exactSpellingSuggestion.ts "
    "exportDefault.ts extendsUntypedModule.ts fakeInfinity1.ts fieldAndGetterWithSameName.ts "
    "flowAssignment.ts forLoop.ts functionOverloads.ts fuzzy.ts"
).split()


def suites():
    entries = []
    for f in FILES:
        title = f"compiler tests compiler tests for tests/cases/compiler/{f}"
        entries.append((f"{title} Correct errors for {f}", f"Correct errors for {f}"))
    return slices.suites_from_titles(entries)


class SliceRows(unittest.TestCase):
    def test_chunks_partition_d_to_f(self):
        every = suites()
        whole = [s.key for s in slices.select("d-f", every, df.SLICES)]
        self.assertEqual(len(whole), len(FILES))
        seen = []
        for name in df.CHUNKS:
            seen += [s.key for s in slices.select(name, every, df.SLICES)]
        self.assertEqual(sorted(seen), sorted(whole))

    def test_d_f_is_the_three_buckets(self):
        every = suites()
        got = {s.base[0].lower() for s in slices.select("d-f", every, df.SLICES)}
        self.assertEqual(got, set("def"))


if __name__ == "__main__":
    unittest.main()
