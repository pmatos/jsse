import { helper } from "#internal/helper";
import { feature } from "info-more/feature";
import { typed } from "typed";
import { scoped } from "@scope/lib";
import config from "./json/config.json";
import "./refs";

export const out: number = helper() + feature + typed + scoped + config.size;
