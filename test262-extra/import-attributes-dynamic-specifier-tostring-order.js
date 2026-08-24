/*---
description: Dynamic import converts the specifier before inspecting import options
esid: sec-evaluate-import-call
info: |
  EvaluateImportCall evaluates both argument expressions before creating its
  promise capability. It then converts the specifier to a string before
  inspecting the already-evaluated options value or validating its attributes.
flags: [module]
features: [dynamic-import, import-defer, source-phase-imports, import-attributes, top-level-await]
---*/

function rejection(promise) {
  return promise.then(() => null, error => error);
}

const forms = [
  ['import()', (specifier, options, events) => import(specifier, (events.push('options'), options))],
  ['import.defer()', (specifier, options, events) => import.defer(specifier, (events.push('options'), options))],
  ['import.source()', (specifier, options, events) => import.source(specifier, (events.push('options'), options))],
];

const cases = forms.map(([name, start]) => {
  const specifierError = name + ' specifier conversion';
  const events = [];
  const specifier = {
    toString() {
      events.push('toString');
      throw specifierError;
    },
  };

  const promise = rejection(start(specifier, { with: { type: 'bogus' } }, events));
  assert.sameValue(
    events.join(','),
    'options,toString',
    name + ' evaluates options before converting the specifier'
  );
  return { name, promise, specifierError };
});

let withInspected = false;
const optionsError = 'options inspection';
const options = {};
Object.defineProperty(options, 'with', {
  get() {
    withInspected = true;
    throw optionsError;
  },
});
const specifierError = 'specifier conversion';
const inspectionPromise = rejection(import({
  toString() {
    throw specifierError;
  },
}, options));
assert.sameValue(withInspected, false, 'options are not inspected after conversion rejects');

const typeErrorPromise = rejection(import(
  './import-attributes-javascript-dep_FIXTURE.mjs',
  { with: { type: 'bogus' } }
));

const results = await Promise.all([
  ...cases.map(testCase => testCase.promise),
  inspectionPromise,
  typeErrorPromise,
]);

for (let i = 0; i < cases.length; i++) {
  assert.sameValue(
    results[i],
    cases[i].specifierError,
    cases[i].name + ' rejects with the specifier conversion error'
  );
}
assert.sameValue(
  results[cases.length],
  specifierError,
  'specifier conversion wins over options inspection'
);
const typeError = results[cases.length + 1];
assert(typeError instanceof TypeError, 'unsupported types still reject after successful ToString');
