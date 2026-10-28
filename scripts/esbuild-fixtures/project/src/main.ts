import * as fs from "fs";
import { greet, Counter, type Greeting } from "./util";
import data from "../assets/data.json";
import banner from "../assets/banner.txt";
import { view } from "./view";

declare const BUILD_ID: string;

export const message: Greeting = greet(data.name);
const counter = new Counter(3);
counter.tick();

export async function lazy(): Promise<string> {
    const { later } = await import("./lazy");
    return later(counter.value);
}

if (typeof fs.existsSync === "function") {
    console.log(message.text, BUILD_ID, banner.trim(), counter.value, view("x"));
}
