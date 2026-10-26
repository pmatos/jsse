globalThis.log.push('tla-delay start');
await new Promise((resolve) => setTimeout(resolve, 5));
globalThis.log.push('tla-delay end');
export const delayed = 'delayed';
