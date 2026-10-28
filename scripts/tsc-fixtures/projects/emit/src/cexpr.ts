function tag(...args: any[]): any {
    return undefined;
}

export const Plain = @tag class {};

export const Named = @tag class Inner {
    static make() {
        return new Inner();
    }
    @tag field = 1;
};

export function build() {
    return [@tag class { @tag static x = 1; }, @tag class extends Object {}];
}
