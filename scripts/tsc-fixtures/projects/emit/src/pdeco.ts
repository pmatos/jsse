import { Counter } from "./lib.js";

function inject(...args: any[]): any {
    return undefined;
}

export class Client {
    constructor(
        @inject private counter: Counter,
        name: string,
    ) {}
    @inject send(@inject msg: string, count?: number): Promise<void> {
        return Promise.resolve();
    }
}
