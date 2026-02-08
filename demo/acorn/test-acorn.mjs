import { parse } from "./node_modules/acorn/dist/acorn.mjs";

// Test 1: Parse a simple expression
var ast = parse("1 + 2", { ecmaVersion: 2020 });
console.log("Test 1 - Simple expression:");
console.log(JSON.stringify(ast, null, 2));

// Test 2: Parse a function declaration
var ast2 = parse("function hello(name) { return 'Hello, ' + name; }", { ecmaVersion: 2020 });
console.log("\nTest 2 - Function declaration:");
console.log(JSON.stringify(ast2, null, 2));

// Test 3: Parse arrow function with destructuring
var ast3 = parse("const greet = ({ name, age }) => `${name} is ${age}`;", { ecmaVersion: 2020 });
console.log("\nTest 3 - Arrow + destructuring + template literal:");
console.log(JSON.stringify(ast3, null, 2));

console.log("\nAll tests passed!");
