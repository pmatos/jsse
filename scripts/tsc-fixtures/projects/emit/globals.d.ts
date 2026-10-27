declare namespace JSX {
    interface Element {}
    interface IntrinsicAttributes { key?: string }
    interface IntrinsicElements { [tag: string]: any }
}
declare const React: { createElement(...args: any[]): any; Fragment: any };
declare function h(...args: any[]): any;
declare const Frag: any;
declare module "react/jsx-runtime" {
    export const jsx: any;
    export const jsxs: any;
    export const Fragment: any;
}
declare module "react/jsx-dev-runtime" {
    export const jsxDEV: any;
    export const Fragment: any;
}
declare module "preact/jsx-runtime" {
    export const jsx: any;
    export const jsxs: any;
    export const Fragment: any;
}
declare module "preact/jsx-dev-runtime" {
    export const jsxDEV: any;
    export const Fragment: any;
}
interface ImportMeta { url: string }
