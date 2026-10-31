"""Unit tests for scripts/check_agent_docs_links.py (issue #1299)."""

import contextlib
import importlib.util
import io
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def load():
    spec = importlib.util.spec_from_file_location(
        "check_agent_docs_links", os.path.join(HERE, "check_agent_docs_links.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


links = load()


def write_tree(root, files):
    for rel, text in files.items():
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)


def run_main(root, *args):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = links.main(["--root", root, *args])
    return code, out.getvalue()


class SlugTests(unittest.TestCase):
    def test_github_rules(self):
        self.assertEqual(links.slug("Adding a core module"), "adding-a-core-module")
        self.assertEqual(links.slug("`--node` host (#1299)"), "--node-host-1299")
        self.assertEqual(links.slug("Gate, baseline and CI"), "gate-baseline-and-ci")

    def test_duplicate_headings_get_suffixes(self):
        found = links.anchors("# A\n\n## B\n\n## B\n\n```\n# not a heading\n```\n")
        self.assertEqual(found, {"a", "b", "b-1"})

    def test_suffix_never_repeats_a_slug(self):
        self.assertEqual(links.anchors("# A\n# A\n# A-1\n"), {"a", "a-1", "a-1-1"})

    def test_backticks_stay_in_the_text_of_a_heading(self):
        self.assertEqual(
            links.anchors("## Running `tsc` now\n## `--node` host\n"),
            {"running-tsc-now", "--node-host"},
        )


class ExtractionTests(unittest.TestCase):
    def test_code_is_not_scanned(self):
        text = "[ok](a.md) and `[no](b.md)`\n\n```\n[no](c.md)\n```\n![img](d.png)\n"
        self.assertEqual([t for _, t in links.links(text)], ["a.md"])

    def test_fence_length_and_character(self):
        text = "````\n```\n[no](b.md)\n````\n[yes](a.md)\n~~~\n[no](c.md)\n```\n[no](d.md)\n~~~\n"
        self.assertEqual([t for _, t in links.links(text)], ["a.md"])

    def test_title_and_angle_brackets(self):
        text = '[x](a.md "title") [y](<b.md#frag>)\n'
        self.assertEqual([t for _, t in links.links(text)], ["a.md", "b.md#frag"])


class CheckTests(unittest.TestCase):
    def tree(self, files):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        write_tree(tmp.name, files)
        return tmp.name

    def test_clean_tree(self):
        root = self.tree(
            {
                "AGENTS.md": "[a](docs/agents/a.md#section) [web](https://example.com/x.md)\n",
                "docs/agents/a.md": "# A\n\n## Section\n\n[up](../../AGENTS.md) [self](#section)\n",
            }
        )
        errors, _ = links.check(root)
        self.assertEqual(errors, [])
        self.assertEqual(run_main(root, "--strict")[0], 0)

    def test_missing_file_and_anchor(self):
        root = self.tree(
            {
                "AGENTS.md": "[a](docs/agents/a.md#nope) [b](docs/agents/gone.md) [c](#missing)\n",
                "docs/agents/a.md": "# A\n",
            }
        )
        errors, _ = links.check(root)
        text = "\n".join(errors)
        self.assertEqual(len(errors), 3, text)
        self.assertIn("a.md has no heading #nope", text)
        self.assertIn("gone.md does not exist", text)
        self.assertIn("no heading #missing", text)
        code, out = run_main(root)
        self.assertEqual(code, 1)
        self.assertIn("3 broken links", out)

    def test_non_markdown_target_needs_only_to_exist(self):
        root = self.tree(
            {
                "AGENTS.md": "[s](scripts/x.py) [d](docs/) [m](scripts/y.py)\n",
                "scripts/x.py": "",
                "docs/agents/a.md": "# A\n",
            }
        )
        errors, _ = links.check(root)
        self.assertEqual(len(errors), 1)
        self.assertIn("scripts/y.py", errors[0])

    def test_backticked_heading_anchor_resolves(self):
        root = self.tree(
            {
                "AGENTS.md": "[a](docs/agents/a.md#the-tsc-oracle) [b](docs/agents/a.md#the--tsc--oracle)\n",
                "docs/agents/a.md": "# A\n\n## The `tsc` oracle\n",
            }
        )
        errors, _ = links.check(root)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("#the--tsc--oracle", errors[0])

    def test_orphan_reachable_through_a_page_outside_docs_agents(self):
        root = self.tree(
            {
                "AGENTS.md": "[adr](docs/adr/x.md)\n",
                "docs/adr/x.md": "[a](../agents/a.md)\n",
                "docs/agents/a.md": "# A\n",
            }
        )
        _, edges = links.check(root)
        self.assertEqual(links.orphans(root, edges), [])

    def test_orphans_are_warnings_unless_strict(self):
        root = self.tree(
            {
                "AGENTS.md": "[a](docs/agents/a.md)\n",
                "docs/agents/a.md": "# A\n\n[b](b.md)\n",
                "docs/agents/b.md": "# B\n",
                "docs/agents/lost.md": "# Lost\n\n[a](a.md)\n",
            }
        )
        code, out = run_main(root)
        self.assertEqual(code, 0)
        self.assertIn("warning: docs/agents/lost.md", out)
        self.assertNotIn("b.md is not reachable", out)
        self.assertEqual(run_main(root, "--strict")[0], 1)
        code, out = run_main(root, "--list-orphans")
        self.assertEqual((code, out.split()), (0, ["docs/agents/lost.md"]))


class RepositoryTests(unittest.TestCase):
    def test_agent_docs_links_resolve(self):
        errors, _ = links.check(ROOT)
        self.assertEqual(errors, [])

    def test_node_host_pages_are_reachable_from_agents_md(self):
        _, edges = links.check(ROOT)
        lost = {os.path.basename(p) for p in links.orphans(ROOT, edges)}
        for page in (
            "node-host.md",
            "node-host-add-module.md",
            "typescript-suite-local.md",
            "typescript-suite-triage.md",
            "library-tests-typescript.md",
            "library-tests-typescript-oracles.md",
        ):
            self.assertNotIn(page, lost)


if __name__ == "__main__":
    unittest.main()
