declare function h(tag: string, props: unknown, ...children: unknown[]): string;

export function view(label: string): string {
    return h("span", { class: "label" }, label);
}
