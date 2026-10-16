function* f(){yield 0;return s();function s(){return 42;}}const it=f();it.next();console.log(it.next().value);
