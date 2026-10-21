#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");
const { resolveBinary } = require("../lib/resolve");

let bin;
try {
  bin = resolveBinary();
} catch (error) {
  console.error(`[tessera] ${error.message}`);
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`[tessera] failed to launch ${bin}: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
