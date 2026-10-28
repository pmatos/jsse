import { shout, counter } from "../../util/src/index";

export const message: string = shout({ name: "world" }) + counter.next();
