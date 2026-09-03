#!/usr/bin/env node

import { execFileSync } from "node:child_process";

try {
  const baseRef = readBaseRef(process.argv.slice(2));
  const mergeBase = git("merge-base", baseRef, "HEAD");
  const forkNumbers = readAdrNumbers(mergeBase);
  const headNumbers = readAdrNumbers("HEAD");
  const baseNumbers = readAdrNumbers(baseRef);
  const conflicts = findConflicts({
    baseNumbers,
    forkNumbers,
    headNumbers
  });

  if (conflicts.length === 0) {
    console.log(`ADR numbers remain unambiguous after merge into ${baseRef}.`);
  } else {
    for (const conflict of conflicts) {
      console.error(
        `ADR number ${conflict.number} would be ambiguous after merge (${conflict.projectedCount} files):`
      );
      console.error(`  current base (${baseRef}):`);
      printPaths(conflict.basePaths);
      console.error(`  feature additions since ${mergeBase}:`);
      printPaths(conflict.featurePaths);
    }
    console.error(
      "Choose an unused ADR number after refreshing the pull request's base branch."
    );
    process.exitCode = 1;
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 2;
}

function readBaseRef(args) {
  if (args.length !== 2 || args[0] !== "--base" || args[1] === undefined) {
    throw new Error("Usage: check-adr-numbers.mjs --base <git-ref>");
  }
  git("rev-parse", "--verify", `${args[1]}^{commit}`);
  return args[1];
}

function readAdrNumbers(ref) {
  const paths = git("ls-tree", "-r", "--name-only", ref, "--", "docs/adr");
  const numbers = new Map();

  for (const path of paths.split("\n")) {
    const number = /^docs\/adr\/(\d{4})-[^/]+\.md$/.exec(path)?.[1];
    if (number === undefined) {
      continue;
    }
    const numberedPaths = numbers.get(number) ?? [];
    numberedPaths.push(path);
    numbers.set(number, numberedPaths);
  }

  return numbers;
}

function findConflicts({ baseNumbers, forkNumbers, headNumbers }) {
  const numbers = new Set([...forkNumbers.keys(), ...headNumbers.keys()]);
  const conflicts = [];

  for (const number of [...numbers].sort()) {
    const forkPaths = forkNumbers.get(number) ?? [];
    const headPaths = headNumbers.get(number) ?? [];
    const basePaths = baseNumbers.get(number) ?? [];
    const featureDelta = headPaths.length - forkPaths.length;
    const projectedCount = basePaths.length + featureDelta;

    if (featureDelta > 0 && projectedCount > 1) {
      conflicts.push({
        basePaths: [...basePaths].sort(),
        featurePaths: headPaths
          .filter((path) => !forkPaths.includes(path))
          .sort(),
        number,
        projectedCount
      });
    }
  }

  return conflicts;
}

function printPaths(paths) {
  if (paths.length === 0) {
    console.error("    (none)");
    return;
  }
  for (const path of paths) {
    console.error(`    ${path}`);
  }
}

function git(...args) {
  return execFileSync("git", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"]
  }).trim();
}
