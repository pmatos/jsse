import { Counter } from "./lib.js";

function Item(p: { n: number; key?: string }) {
    return <span>{p.n}</span>;
}

export function View(props: { c: Counter; label?: string }) {
    return (
        <div className="v" {...props}>
            <Item key="a" n={1} />
            {props.label} text <>frag</>
        </div>
    );
}
