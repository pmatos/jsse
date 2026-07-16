// Patch Luxon 3.7.2 tests whose literal CLDR data no longer matches current
// Node ICU. These are oracle portability fixes, not jsse skips: the test count
// remains unchanged, and jsse still runs the original locale parser assertions
// when its shim does not advertise Node's CLDR version.

"use strict";

const fs = require("fs");

function replaceOnce(file, before, after) {
  const source = fs.readFileSync(file, "utf8");
  const first = source.indexOf(before);
  if (first === -1) throw new Error(`patch-luxon-icu: no match in ${file}`);
  if (source.indexOf(before, first + before.length) !== -1) {
    throw new Error(`patch-luxon-icu: ambiguous match in ${file}`);
  }
  fs.writeFileSync(file, source.slice(0, first) + after + source.slice(first + before.length));
}

const tokenParse = "test/datetime/tokenParse.test.js";
replaceOnce(
  tokenParse,
  'test("DateTime.fromFormat() makes dots optional and handles non breakable spaces", () => {\n',
  'test("DateTime.fromFormat() makes dots optional and handles non breakable spaces", () => {\n' +
    "  // CLDR 47 changed the es-ES day-period data this historical fixture encodes.\n" +
    "  if ((cldrMajorVersion() || 0) >= 47) return;\n"
);

const thaiBefore = `  const ex18 = DateTime.fromFormatExplain(
    "๐๓-เมษายน-๒๐๑๙ ๐๔:๐๒:๒๔ หลังเที่ยง",
    "dd-MMMM-yyyy hh:mm:ss a",
    {
      locale: "th",
      numberingSystem: "thai",
    }
  );
  expect(ex18.rawMatches).toBeInstanceOf(Array);
  expect(ex18.matches).toBeInstanceOf(Object);
  expect(keyCount(ex18.matches)).toBe(7);
  expect(ex18.result).toBeInstanceOf(Object);
  expect(keyCount(ex18.result)).toBe(6);
`;
replaceOnce(
  tokenParse,
  thaiBefore,
  `  // CLDR 47 changed the Thai day-period phrase in this literal fixture.
  if ((cldrMajorVersion() || 0) < 47) {
${thaiBefore
  .split("\n")
  .map((line) => (line ? "  " + line : line))
  .join("\n")}  }
`
);

replaceOnce(
  "test/datetime/format.test.js",
  `test("DateTime#toLocaleString can override the dateTime's output calendar", () => {
  expect(
    dt.reconfigure({ outputCalendar: "islamic" }).toLocaleString({}, { outputCalendar: "coptic" })
  ).toBe("9/17/1698 ERA1");
});`,
  `test("DateTime#toLocaleString can override the dateTime's output calendar", () => {
  const formatted = dt
    .reconfigure({ outputCalendar: "islamic" })
    .toLocaleString({}, { outputCalendar: "coptic" });
  // ICU 78 reports the short Coptic era as "AM"; older data used "ERA1".
  expect(["9/17/1698 ERA1", "9/17/1698 AM"].includes(formatted)).toBe(true);
});`
);

replaceOnce(
  "test/interval/format.test.js",
  `test("Interval#toLocaleString can override the start DateTime's output calendar", () => {
  expect(
    Interval.fromDateTimes(
      interval.start.reconfigure({ outputCalendar: "islamic" }),
      interval.end
    ).toLocaleString({}, { outputCalendar: "coptic" })
  ).toBe("9/17/1698 – 2/3/1700 ERA1");
});`,
  `test("Interval#toLocaleString can override the start DateTime's output calendar", () => {
  const formatted = Interval.fromDateTimes(
    interval.start.reconfigure({ outputCalendar: "islamic" }),
    interval.end
  ).toLocaleString({}, { outputCalendar: "coptic" });
  // ICU 78 reports the short Coptic era as "AM"; older data used "ERA1".
  expect(["9/17/1698 – 2/3/1700 ERA1", "9/17/1698 – 2/3/1700 AM"].includes(formatted)).toBe(
    true
  );
});`
);

console.log("patch-luxon-icu: applied 4 CLDR portability patches");
