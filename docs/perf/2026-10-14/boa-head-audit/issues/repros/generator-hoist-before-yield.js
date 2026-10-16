function* g(){yield s();function s(){return 42;}}console.log(g().next().value);
