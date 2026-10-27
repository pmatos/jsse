function log(...args: any[]): any {
    return undefined;
}

@log
export class Service {
    @log field = 1;
    @log static shared = 2;
    @log method(): number {
        return this.field;
    }
    @log get prop(): number {
        return 1;
    }
}
