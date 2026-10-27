// MODES: pipe file
// INPUT: through the pipe\nsecond line\n
'use strict';
process.stdin.pipe(process.stdout);
process.stdin.on('end', () => process.stderr.write('[end]\n'));
