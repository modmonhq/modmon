#!/usr/bin/env node
import path from "node:path";
import process from "node:process";
import { fileRecord } from "./lib.mjs";

const [file, target, format, ...binaries] = process.argv.slice(2);
if (!file || !target || !format || binaries.length === 0) throw new Error("package metadata arguments are incomplete");
const record = await fileRecord(file, path.basename(file));
process.stdout.write(`${JSON.stringify({ ...record, target, format, binaries })}\n`);
