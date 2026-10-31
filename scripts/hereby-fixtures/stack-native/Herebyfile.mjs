import { task } from "hereby";
import { Loader } from "./loader.mjs";

export const t = task({ name: "t", run: () => {} });
Loader.create().load();
