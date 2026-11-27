// Evaluate natural-English sources through the exact browser WebAssembly bridge.
//
// Input: a JSON file containing [{"id": 1, "path": "..."}] and an output path.
// Output: a JSON array of tc_eval payloads. No external packages are used.

import fs from "node:fs";

const wasmPath = process.argv[2];
const jobsPath = process.argv[3];
const outPath = process.argv[4];

const jobs = JSON.parse(fs.readFileSync(jobsPath, "utf8"));
const bytes = fs.readFileSync(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const exports = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function invoke(source, call) {
  const encoded = encoder.encode(source);
  const pointer = exports.tc_alloc(encoded.length);
  if (!pointer) {
    throw new Error("source allocation failed");
  }
  if (encoded.length > 0) {
    new Uint8Array(exports.memory.buffer, pointer, encoded.length).set(encoded);
  }
  let packed;
  try {
    packed = call(exports, pointer, encoded.length);
  } finally {
    exports.tc_dealloc(pointer, encoded.length);
  }
  if (typeof packed !== "bigint") {
    throw new Error("unexpected ABI result type");
  }
  const resultPointer = Number(packed >> 32n);
  const resultLength = Number(packed & 0xffffffffn);
  if (!resultPointer || !resultLength) {
    throw new Error("empty result buffer");
  }
  const copy = new Uint8Array(exports.memory.buffer, resultPointer, resultLength).slice();
  exports.tc_dealloc(resultPointer, resultLength);
  return JSON.parse(decoder.decode(copy));
}

const results = [];
for (const job of jobs) {
  const source = fs.readFileSync(job.path, "utf8");
  try {
    const payload = invoke(source, (api, pointer, length) =>
      api.tc_eval(pointer, length, 20_000_000),
    );
    results.push({ id: job.id, payload });
  } catch (error) {
    results.push({ id: job.id, error: String(error) });
  }
}
fs.writeFileSync(outPath, JSON.stringify(results));
