// Select jitless validation for the independent Zod bundle copy that follows.
delete globalThis.__zod_globalConfig;
delete globalThis.__zod_globalRegistry;
globalThis.__ZOD_BUNDLE_JITLESS__ = true;
