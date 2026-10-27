// RUN: 1
// Own properties of the error are shown after its stack.
class AppError extends Error {
  constructor(message) {
    super(message);
    this.name = "AppError";
    this.code = "E_APP";
    this.details = { retry: false, items: [1, 2] };
  }
}
throw new AppError("custom");
