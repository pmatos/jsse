import { marked } from './node_modules/marked/lib/marked.esm.js';

// Test 1: Simple heading
let result1 = marked.parse("# Hello");
console.log("Test 1 - Heading:");
console.log(result1);
console.log("Expected: <h1>Hello</h1>");
console.log("PASS:", result1 === "<h1>Hello</h1>\n");

// Test 2: Paragraph
let result2 = marked.parse("Hello world");
console.log("\nTest 2 - Paragraph:");
console.log(result2);
console.log("Expected: <p>Hello world</p>");
console.log("PASS:", result2 === "<p>Hello world</p>\n");

// Test 3: Bold text
let result3 = marked.parse("**bold**");
console.log("\nTest 3 - Bold:");
console.log(result3);
console.log("Expected: <p><strong>bold</strong></p>");
console.log("PASS:", result3 === "<p><strong>bold</strong></p>\n");
