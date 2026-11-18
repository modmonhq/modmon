#!/usr/bin/env node
import process from "node:process";
import { parseReleaseTag, releaseNotes } from "./lib.mjs";

const requested = process.argv[2] ?? "";
if (requested === "Unreleased") {
  process.stdout.write(await releaseNotes(process.cwd(), requested));
} else {
  const { tag, version } = parseReleaseTag(requested.startsWith("v") ? requested : `v${requested}`);
  process.stdout.write(await releaseNotes(process.cwd(), version, tag));
}
