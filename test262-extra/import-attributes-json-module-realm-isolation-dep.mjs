import parsed from './import-attributes-valid-json-dep.mjs' with { type: 'json' };

export function parsedIsOwnRealmObject() {
  return parsed instanceof Object && parsed.answer === 42;
}
