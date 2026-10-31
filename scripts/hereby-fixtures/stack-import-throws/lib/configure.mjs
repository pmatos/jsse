function validate(options) {
  if (options.mode === "strict") {
    throw new TypeError(`unsupported mode ${options.mode}`);
  }
}

export function configure(options) {
  validate(options);
}
