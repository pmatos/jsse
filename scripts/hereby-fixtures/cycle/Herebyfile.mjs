import { task } from "hereby";

const a = task({ name: "a", dependencies: [], run: () => {} });
const b = task({ name: "b", dependencies: [a], run: () => {} });
a.options.dependencies.push(b);

export { a, b };
