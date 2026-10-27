function id<T extends string>(x: T): T { return x; }
id(1);
id<number>("a");
const arr: Array<string> = [1, 2];
