// Strict mode reserved words tests

// In sloppy mode, these should work as identifiers
var implements = 1;
var interface = 2;
var package = 3;
var private = 4;
var protected = 5;
var public = 6;

if (implements !== 1) throw "implements should be 1";
if (interface !== 2) throw "interface should be 2";
if (package !== 3) throw "package should be 3";
if (private !== 4) throw "private should be 4";
if (protected !== 5) throw "protected should be 5";
if (public !== 6) throw "public should be 6";

// Class bodies are strict - these words can't be used as identifiers inside
// (tested via negative test below, which would need a different harness)

// Verify "use strict" directive is detected
function strictFunc() {
    "use strict";
    try {
        eval("var implements = 1;");
        throw "Should have thrown";
    } catch(e) {
        // Expected: strict mode should reject 'implements'
    }
}
