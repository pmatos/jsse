import { marked } from './node_modules/marked/lib/marked.esm.js';

let passed = 0;
let failed = 0;

function test(name, markdown, expected) {
  let result = marked.parse(markdown);
  if (result === expected) {
    console.log("PASS:", name);
    passed++;
  } else {
    console.log("FAIL:", name);
    console.log("  Expected:", JSON.stringify(expected));
    console.log("  Got:     ", JSON.stringify(result));
    failed++;
  }
}

// --- Headings ---
test("H1", "# Hello", "<h1>Hello</h1>\n");
test("H2", "## Hello", "<h2>Hello</h2>\n");
test("H3", "### Hello", "<h3>Hello</h3>\n");
test("H4", "#### Hello", "<h4>Hello</h4>\n");
test("H5", "##### Hello", "<h5>Hello</h5>\n");
test("H6", "###### Hello", "<h6>Hello</h6>\n");

// --- Emphasis ---
test("Bold", "**bold**", "<p><strong>bold</strong></p>\n");
test("Italic", "*italic*", "<p><em>italic</em></p>\n");
test("Bold + Italic", "***both***", "<p><em><strong>both</strong></em></p>\n");
test("Strikethrough", "~~strike~~", "<p><del>strike</del></p>\n");

// --- Links ---
test("Link", "[Google](https://google.com)", '<p><a href="https://google.com">Google</a></p>\n');
test("Link with title",
  '[Google](https://google.com "Search")',
  '<p><a href="https://google.com" title="Search">Google</a></p>\n');

// --- Images ---
test("Image", "![Alt](img.png)", '<p><img src="img.png" alt="Alt"></p>\n');
test("Image with title", '![Alt](img.png "Title")',
  '<p><img src="img.png" alt="Alt" title="Title"></p>\n');

// --- Code ---
test("Inline code", "`code`", "<p><code>code</code></p>\n");
test("Fenced code block", "```\ncode\n```", "<pre><code>code\n</code></pre>\n");
test("Fenced code with lang", "```js\nvar x = 1;\n```",
  '<pre><code class="language-js">var x = 1;\n</code></pre>\n');

// --- Lists ---
test("Unordered list", "- a\n- b\n- c",
  "<ul>\n<li>a</li>\n<li>b</li>\n<li>c</li>\n</ul>\n");
test("Ordered list", "1. a\n2. b\n3. c",
  "<ol>\n<li>a</li>\n<li>b</li>\n<li>c</li>\n</ol>\n");

// --- Blockquotes ---
test("Blockquote", "> Hello", "<blockquote>\n<p>Hello</p>\n</blockquote>\n");
test("Nested blockquote", "> > Nested",
  "<blockquote>\n<blockquote>\n<p>Nested</p>\n</blockquote>\n</blockquote>\n");

// --- Horizontal rule ---
test("HR", "---", "<hr>\n");

// --- Paragraphs ---
test("Paragraph", "Hello world", "<p>Hello world</p>\n");
test("Two paragraphs", "Hello\n\nWorld", "<p>Hello</p>\n<p>World</p>\n");

// --- HTML passthrough ---
test("HTML passthrough", "<div>hello</div>", "<div>hello</div>");

// --- Tables (GFM) ---
test("Table",
  "| a | b |\n| - | - |\n| 1 | 2 |",
  "<table>\n<thead>\n<tr>\n<th>a</th>\n<th>b</th>\n</tr>\n</thead>\n<tbody><tr>\n<td>1</td>\n<td>2</td>\n</tr>\n</tbody></table>\n");

// --- Mixed inline ---
test("Bold in paragraph", "Hello **world**", "<p>Hello <strong>world</strong></p>\n");
test("Link in paragraph", "Go to [site](http://x.com).",
  '<p>Go to <a href="http://x.com">site</a>.</p>\n');
test("Code in paragraph", "Use `foo()` here", "<p>Use <code>foo()</code> here</p>\n");

// --- Escaping ---
test("Escape HTML entities", "1 < 2 & 3 > 0",
  "<p>1 &lt; 2 &amp; 3 &gt; 0</p>\n");

// --- Line break ---
test("Hard line break", "a  \nb", "<p>a<br>b</p>\n");

console.log(`\n${passed}/${passed + failed} tests passed`);
if (failed > 0) {
  throw new Error(`${failed} test(s) failed`);
}
