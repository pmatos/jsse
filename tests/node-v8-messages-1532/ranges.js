// V8 text of typed array, DataView and ArrayBuffer range and detach errors
// (#1532), run verbatim on Node and on `jsse --node`.
const cases = {
  u8Length: () => new Uint8Array(new ArrayBuffer(3), 1, 5),
  u8Offset: () => new Uint8Array(new ArrayBuffer(3), 5),
  u32Length: () => new Uint32Array(new ArrayBuffer(4), 0, 4),
  u16Multiple: () => new Uint16Array(new ArrayBuffer(3)),
  dvOffset: () => new DataView(new ArrayBuffer(1), 5),
  dvLength: () => new DataView(new ArrayBuffer(4), 2, 8),
  dvGetOutside: () => new DataView(new ArrayBuffer(1)).getInt32(0),
  dvGetNegative: () => new DataView(new ArrayBuffer(1)).getInt8(-1),
  dvSetNegative: () => new DataView(new ArrayBuffer(1)).setInt8(-1, 0),
  abLength: () => new ArrayBuffer(-1),
  abMaxLength: () => new ArrayBuffer(8, { maxByteLength: 4 }),
  abResizeRange: () => new ArrayBuffer(8, { maxByteLength: 16 }).resize(32),
  abResizeFixed: () => new ArrayBuffer(8).resize(4),
  taWithRange: () => new Uint8Array(1).with(5, 1),
};
const detached = () => {
  const buffer = new ArrayBuffer(8);
  const view = new Uint8Array(buffer);
  const data = new DataView(buffer);
  buffer.transfer();
  return { buffer, view, data };
};
const { buffer, view, data } = detached();
Object.assign(cases, {
  taFill: () => view.fill(0),
  taAt: () => view.at(0),
  taSet: () => view.set([1]),
  taMap: () => view.map((x) => x),
  taSlice: () => view.slice(0),
  taJoin: () => view.join(),
  taSort: () => view.sort(),
  taEntries: () => view.entries(),
  dvGetDetached: () => data.getInt8(0),
  dvLengthDetached: () => data.byteLength,
  abSliceDetached: () => buffer.slice(0),
  abTransferDetached: () => buffer.transfer(),
  newTaDetached: () => new Uint8Array(buffer),
  newDvDetached: () => new DataView(buffer),
});
for (const [name, fn] of Object.entries(cases)) {
  try {
    fn();
    console.log(name, "-> no error");
  } catch (e) {
    console.log(name, "->", `${e.name}: ${e.message}`);
  }
}
