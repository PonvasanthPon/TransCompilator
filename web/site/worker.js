/* Dedicated WebAssembly worker. The main thread enforces a five-second
   wall-clock limit and terminates this worker if the reference interpreter
   cannot make progress. */
(function () {
  "use strict";

  var instance = null;
  var readyPromise = null;
  var encoder = new TextEncoder();
  var decoder = new TextDecoder();

  function post(message) {
    self.postMessage(message);
  }

  async function boot() {
    try {
      var url = new URL("tc_wasm.wasm", self.location.href);
      var response = await fetch(url, { cache: "no-cache" });
      if (!response.ok) {
        throw new Error("HTTP " + response.status + " while loading " + url.pathname);
      }
      var bytes = await response.arrayBuffer();
      var compiled = await WebAssembly.instantiate(bytes, {});
      instance = compiled.instance;
      post({ type: "ready" });
    } catch (error) {
      post({
        type: "fatal",
        message: "tc_wasm.wasm could not be loaded: " + String(error)
      });
    }
  }

  function evaluate(code, maxSteps) {
    var exports = instance.exports;
    var source = encoder.encode(code);
    var sourcePointer = exports.tc_alloc(source.length);
    if (!sourcePointer) {
      throw new Error("The WebAssembly source buffer could not be allocated.");
    }
    if (source.length > 0) {
      new Uint8Array(exports.memory.buffer, sourcePointer, source.length).set(source);
    }
    var budget = Math.min(Math.max(Math.floor(maxSteps) || 2000000, 1), 50000000);
    var packed;
    try {
      packed = exports.tc_eval(sourcePointer, source.length, budget);
    } finally {
      exports.tc_dealloc(sourcePointer, source.length);
    }
    if (typeof packed !== "bigint") {
      throw new Error("The WebAssembly ABI returned an unexpected result type.");
    }
    var resultPointer = Number(packed >> 32n);
    var resultLength = Number(packed & 0xffffffffn);
    if (!resultPointer || !resultLength) {
      throw new Error("The WebAssembly result buffer was empty.");
    }
    var copy = new Uint8Array(exports.memory.buffer, resultPointer, resultLength).slice();
    exports.tc_dealloc(resultPointer, resultLength);
    return JSON.parse(decoder.decode(copy));
  }

  self.onmessage = async function (event) {
    var data = event.data || {};
    if (data.type !== "run") {
      return;
    }
    try {
      if (!instance) {
        await readyPromise;
      }
      if (!instance) {
        throw new Error("The WebAssembly module is not loaded.");
      }
      var payload = evaluate(String(data.code || ""), data.maxSteps);
      post({ type: "result", payload: payload });
    } catch (error) {
      post({
        type: "error",
        message: "The program could not be evaluated: " + String(error)
      });
    }
  };

  readyPromise = boot();
})();
