function mark(...args: any[]): any {
    return undefined;
}

@mark
export class Model {
    @mark accessor id = 1;
    @mark static count = 0;
    @mark #secret = 2;
    @mark compute(): number {
        return this.id + this.#secret;
    }
}

export const Anon = @mark class {
    @mark run() {}
};
