// UTC-only: local time zone output depends on the host and is not compared.
const d = new Date(Date.UTC(2024, 1, 29, 13, 45, 30, 123));
show(d.toISOString(), d.getTime(), d.getUTCDay(), JSON.stringify(d), d.toUTCString());
show(Date.UTC(1970, 0, 1), new Date(8.64e15 + 1).getTime(), new Date(NaN).getTime(), String(new Date(NaN)));
show(Date.parse("2024-03-10T12:00:00Z"), Date.parse("2024-03-10T12:00:00.5+02:00"), Date.parse("garbage"));
const m = new Date(Date.UTC(2024, 0, 31));
m.setUTCMonth(1);
show(m.toISOString(), new Date(Date.UTC(-1, 0, 1)).toISOString(), new Date(Date.UTC(275760, 8, 13)).toISOString());
