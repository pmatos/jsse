# Architecture Decision Records

## Legacy numbering (0001-0003)

`0001-*.md` through `0003-*.md` use sequential numbers assigned by hand.
This scheme is collision-prone under concurrent PRs — s11 hit it once
(two files both numbered `0011`) and symphonika hit it a dozen+ times —
which is why the scheme changed. These files are frozen: don't renumber
or reuse a number from this range — they're referenced by number (e.g.
`ADR 0001`, `ADR 0003`) in source comments and design docs under
`docs/specs/`.

## Current naming

New ADRs use `docs/adr/YYYY-MM-DD-slug.md`, dated the day the ADR is
authored. Reference one in prose or comments as `ADR-YYYY-MM-DD` (add the
slug too if more than one ADR shares a date). Two authors can't
independently pick the same real-world date-and-slug pair the way they
could pick the same next integer, so there's no more numbering-collision
class to check for in CI.
