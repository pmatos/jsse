const s = "The quick brown fox";
show(s.split(" "), s.split(""), s.split(/(o)/), s.split(" ", 2));
show(s.padStart(25, "*"), s.padEnd(22, "-"), s.at(-1), s.slice(-3), s.substring(5, 1));
show(s.replace(/o/g, "0"), s.replaceAll("o", "()"), s.replace("quick", "$&-$&"), s.replace(/(\w+) (\w+)/, "$2 $1"));
show("abc".localeCompare("abz"), "a".codePointAt(0), String.fromCodePoint(128512).length, [..."a😀b"].length);
show("  trim me  ".trim() + "|", "x".repeat(3), "abc".includes("bc"), "abc".indexOf(""), "abc".lastIndexOf("c", -5));
show("ß".toUpperCase(), "İ".toLowerCase().length, "Σ".toLowerCase(), "ǅ".toUpperCase());
show(JSON.stringify("tab\there\nquote\"back\\slash"), escape("a b+c"), encodeURIComponent("a b&c/d"));
show(`template ${1 + 1} ${[1, 2]} ${{}} ${null} ${undefined}`);
