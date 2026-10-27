import { greet, VERSION } from "../../core/src/index";

export const message: string = greet({ name: "world" }) + VERSION;
