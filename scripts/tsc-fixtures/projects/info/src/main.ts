import { add } from "./util";
import { deep } from "./deep/deep";
import { pkgValue } from "pkg";
import { dualValue } from "dual";
import data from "./data.json";

export const total: number = add(deep, pkgValue) + dualValue + data.count;
