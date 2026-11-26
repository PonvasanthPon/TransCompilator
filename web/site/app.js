(function () {
  "use strict";

  var MAX_STEPS = 2000000;
  var WALL_LIMIT_MS = 5000;

  var editor = document.getElementById("editor");
  var gutter = document.getElementById("gutter");
  var examplesSelect = document.getElementById("examples");
  var runButton = document.getElementById("run");
  var shareButton = document.getElementById("share");
  var clearButton = document.getElementById("clear");
  var versionLabel = document.getElementById("version");
  var stdout = document.getElementById("stdout");
  var diagnosticsRoot = document.getElementById("diagnostics");
  var traceRoot = document.getElementById("trace");
  var paraphraseOutput = document.getElementById("paraphrase");
  var irOutput = document.getElementById("ir");
  var statusDot = document.getElementById("status-dot");
  var statusText = document.getElementById("status-text");
  var tabs = Array.prototype.slice.call(document.querySelectorAll(".tab"));

  var examples = window.TC_EXAMPLES || [];
  var worker = null;
  var workerReady = false;
  var running = false;
  var pendingRun = false;
  var timeoutId = null;
  var runStarted = 0;
  var lastDiagnostics = [];
  var statusState = "ready";
  var shareResetTimer = null;

  function formatNumber(value) {
    return Number(value || 0).toLocaleString("en-US");
  }

  function plural(count, singular, pluralForm) {
    return count === 1 ? singular : pluralForm;
  }

  function setStatus(text, state) {
    statusState = state || "ready";
    statusText.textContent = text;
    statusDot.className = "dot";
    if (statusState === "ok") {
      statusDot.classList.add("ok");
    } else if (statusState === "error") {
      statusDot.classList.add("error");
    }
  }

  function setRunning(active) {
    running = active;
    runButton.disabled = active;
    runButton.setAttribute("aria-busy", active ? "true" : "false");
  }

  function setPre(element, text, placeholder) {
    if (text) {
      element.textContent = text;
      element.classList.remove("is-empty");
    } else {
      element.textContent = placeholder;
      element.classList.add("is-empty");
    }
  }

  function setTab(name) {
    tabs.forEach(function (tab) {
      var selected = tab.dataset.panel === name;
      tab.setAttribute("aria-selected", selected ? "true" : "false");
      tab.tabIndex = selected ? 0 : -1;
      var panel = document.getElementById("panel-" + tab.dataset.panel);
      if (panel) {
        panel.hidden = !selected;
      }
    });
  }

  function renderGutter() {
    var lineCount = editor.value.split("\n").length;
    var errorLines = {};
    var warningLines = {};
    lastDiagnostics.forEach(function (diagnostic) {
      if (diagnostic.line > 0) {
        if (diagnostic.severity === "warning") {
          warningLines[diagnostic.line] = true;
        } else {
          errorLines[diagnostic.line] = true;
        }
      }
    });
    var fragment = document.createDocumentFragment();
    for (var line = 1; line <= lineCount; line += 1) {
      var row = document.createElement("div");
      row.className = "gutter-line";
      if (errorLines[line]) {
        row.classList.add("has-error");
      } else if (warningLines[line]) {
        row.classList.add("has-warning");
      }
      row.textContent = String(line);
      fragment.appendChild(row);
    }
    gutter.textContent = "";
    gutter.appendChild(fragment);
    gutter.scrollTop = editor.scrollTop;
  }

  function placeCursor(line, column) {
    var lines = editor.value.split("\n");
    var target = Math.min(Math.max(line - 1, 0), lines.length - 1);
    var offset = 0;
    for (var index = 0; index < target; index += 1) {
      offset += lines[index].length + 1;
    }
    var caret = offset + Math.min(Math.max(column - 1, 0), lines[target].length);
    editor.focus();
    editor.setSelectionRange(caret, caret);
    var lineHeight = parseFloat(window.getComputedStyle(editor).lineHeight) || 31;
    var desired = Math.max(0, target * lineHeight - editor.clientHeight / 3);
    editor.scrollTop = desired;
    gutter.scrollTop = desired;
  }

  function renderDiagnostics(diagnostics) {
    diagnosticsRoot.textContent = "";
    diagnostics.forEach(function (diagnostic) {
      var button = document.createElement("button");
      button.type = "button";
      button.className = "diagnostic";
      if (diagnostic.severity === "warning") {
        button.classList.add("warning");
      }
      var location = document.createElement("span");
      location.className = "diagnostic-location";
      location.textContent = diagnostic.line + ":" + diagnostic.col;
      var body = document.createElement("span");
      var message = document.createElement("span");
      message.className = "diagnostic-message";
      message.textContent = diagnostic.message;
      body.appendChild(message);
      if (diagnostic.category) {
        var category = document.createElement("span");
        category.className = "diagnostic-category";
        category.textContent = diagnostic.category;
        body.appendChild(category);
      }
      button.appendChild(location);
      button.appendChild(body);
      button.addEventListener("click", function () {
        placeCursor(diagnostic.line || 1, diagnostic.col || 1);
      });
      diagnosticsRoot.appendChild(button);
    });
  }

  function traceMeta(entry) {
    var parts = [];
    if (entry.lg_used) {
      parts.push("Link Grammar " + entry.lg_words + "w/" + entry.lg_links + "l");
    } else {
      parts.push("closed-class fallback");
    }
    if (entry.voice) {
      parts.push(entry.voice);
    }
    if (entry.verb) {
      parts.push("verb " + entry.verb + (entry.lemma && entry.lemma !== entry.verb ? " (" + entry.lemma + ")" : ""));
    }
    if (entry.predicate) {
      parts.push("predicate " + entry.predicate);
    }
    if (entry.concept) {
      parts.push("concept " + entry.concept);
    }
    if (entry.class_id) {
      parts.push("VerbNet " + entry.class_id);
    }
    if (entry.synset_id) {
      parts.push("WordNet " + entry.synset_id);
    }
    if (entry.fillers && entry.fillers.length) {
      parts.push("fillers " + entry.fillers.join(", "));
    }
    return parts.join(" · ");
  }

  function renderTrace(trace) {
    traceRoot.textContent = "";
    if (!trace.length) {
      var empty = document.createElement("p");
      empty.className = "placeholder";
      empty.textContent = "No sentences were traced.";
      traceRoot.appendChild(empty);
      return;
    }
    trace.forEach(function (entry) {
      var article = document.createElement("article");
      article.className = "trace-entry";

      var head = document.createElement("div");
      head.className = "trace-head";
      var location = document.createElement("span");
      location.className = "trace-location";
      location.textContent = "L" + entry.line + ":" + entry.col;
      var badge = document.createElement("span");
      badge.className = "trace-badge";
      if (entry.intent === "EXEC") {
        badge.classList.add("exec");
      } else if (entry.intent === "COMMENT_OMITTED") {
        badge.classList.add("omitted");
      } else if (entry.intent === "UNSUPPORTED") {
        badge.classList.add("unsupported");
      }
      badge.textContent = entry.intent || "UNKNOWN";
      head.appendChild(location);
      head.appendChild(badge);

      var text = document.createElement("p");
      text.className = "trace-text";
      text.textContent = entry.text;

      var meta = document.createElement("p");
      meta.className = "trace-meta";
      meta.textContent = traceMeta(entry);

      article.appendChild(head);
      article.appendChild(text);
      article.appendChild(meta);
      if (entry.reason) {
        var reason = document.createElement("p");
        reason.className = "trace-reason";
        reason.textContent = entry.reason;
        article.appendChild(reason);
      }
      traceRoot.appendChild(article);
    });
  }

  function renderResult(result, elapsed) {
    if (result.version) {
      versionLabel.textContent = result.version;
    }
    lastDiagnostics = Array.isArray(result.diagnostics) ? result.diagnostics : [];
    if (result.stdout) {
      setPre(stdout, result.stdout, "");
    } else {
      setPre(stdout, "", result.ok ? "The program finished with no output." : "The program produced no output.");
    }
    renderDiagnostics(lastDiagnostics);
    renderTrace(Array.isArray(result.trace) ? result.trace : []);
    setPre(
      paraphraseOutput,
      result.paraphrase,
      "No paraphrase was produced for this program."
    );
    setPre(
      irOutput,
      result.ir,
      "No verified Core-7 IR was produced for this program."
    );
    renderGutter();

    var steps = result.steps || 0;
    var failures = lastDiagnostics.filter(function (diagnostic) {
      return diagnostic.severity !== "warning";
    }).length;
    if (result.ok && failures === 0) {
      setStatus(
        "Executed in " + Math.round(elapsed) + " ms · " + formatNumber(steps) + " IR steps",
        "ok"
      );
    } else {
      setStatus(
        "Stopped in " + Math.round(elapsed) + " ms · " +
          failures + " " + plural(failures, "diagnostic", "diagnostics"),
        "error"
      );
    }
  }

  function clearResult(message) {
    lastDiagnostics = [];
    setPre(stdout, "", message || "Run a program to see its output.");
    renderDiagnostics([]);
    traceRoot.textContent = "";
    var tracePlaceholder = document.createElement("p");
    tracePlaceholder.className = "placeholder";
    tracePlaceholder.textContent =
      "Run a program to inspect sentence classification and discourse resolution.";
    traceRoot.appendChild(tracePlaceholder);
    setPre(
      paraphraseOutput,
      "",
      "Run a program to see the deterministic compiler paraphrase."
    );
    setPre(irOutput, "", "Run a program to see verified Core-7 IR.");
    renderGutter();
    setStatus("Ready", "ready");
  }

  function showTimeout() {
    var seconds = Math.round(WALL_LIMIT_MS / 100) / 10;
    var message =
      "Execution stopped: the " + seconds +
      "-second wall-clock limit was exceeded. A loop may be running without progress. The worker was terminated and restarted.";
    lastDiagnostics = [{
      line: 1,
      col: 1,
      severity: "error",
      category: "TIMEOUT",
      message: message
    }];
    setPre(stdout, "", "No output was returned before the wall-clock limit.");
    renderDiagnostics(lastDiagnostics);
    traceRoot.textContent = "";
    var placeholder = document.createElement("p");
    placeholder.className = "placeholder";
    placeholder.textContent = "No trace was returned before the wall-clock limit.";
    traceRoot.appendChild(placeholder);
    setPre(paraphraseOutput, "", "No paraphrase was returned before the wall-clock limit.");
    setPre(irOutput, "", "No Core-7 IR was returned before the wall-clock limit.");
    renderGutter();
    setStatus("Stopped after " + seconds + " s · worker restarted", "error");
  }

  function showFatal(detail) {
    setTab("output");
    setPre(
      stdout,
      "",
      "The WebAssembly module could not be loaded. Build the site with `make web`, then open the preview URL."
    );
    lastDiagnostics = [{
      line: 1,
      col: 1,
      severity: "error",
      category: "WASM",
      message: detail || "The WebAssembly worker stopped before it was ready."
    }];
    renderDiagnostics(lastDiagnostics);
    traceRoot.textContent = "";
    var placeholder = document.createElement("p");
    placeholder.className = "placeholder";
    placeholder.textContent = "No trace is available because the WebAssembly module did not load.";
    traceRoot.appendChild(placeholder);
    setPre(paraphraseOutput, "", "No paraphrase is available because the WebAssembly module did not load.");
    setPre(irOutput, "", "No Core-7 IR is available because the WebAssembly module did not load.");
    renderGutter();
    setStatus("WebAssembly unavailable", "error");
  }

  function handleWorkerMessage(event) {
    var data = event.data || {};
    if (data.type === "ready") {
      workerReady = true;
      if (pendingRun) {
        pendingRun = false;
        run();
      } else if (statusState === "loading" || statusState === "ready") {
        setStatus("Ready", "ready");
      }
      return;
    }
    if (data.type === "fatal") {
      workerReady = false;
      pendingRun = false;
      if (worker) {
        worker.terminate();
        worker = null;
      }
      running = false;
      setRunning(false);
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
        timeoutId = null;
      }
      showFatal(data.message);
      return;
    }
    if (data.type === "error") {
      running = false;
      setRunning(false);
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
        timeoutId = null;
      }
      setTab("output");
      lastDiagnostics = [{
        line: 1,
        col: 1,
        severity: "error",
        category: "WORKER",
        message: data.message || "The WebAssembly worker failed."
      }];
      setPre(stdout, "", "The program did not finish.");
      renderDiagnostics(lastDiagnostics);
      renderGutter();
      setStatus("Worker error", "error");
      if (worker) {
        worker.terminate();
      }
      worker = spawnWorker();
      return;
    }
    if (data.type === "result") {
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
        timeoutId = null;
      }
      var elapsed = window.performance.now() - runStarted;
      running = false;
      setRunning(false);
      renderResult(data.payload || {}, elapsed);
    }
  }

  function spawnWorker() {
    workerReady = false;
    var next = new Worker("worker.js");
    next.onmessage = handleWorkerMessage;
    next.onerror = function () {
      workerReady = false;
      running = false;
      setRunning(false);
      if (timeoutId !== null) {
        window.clearTimeout(timeoutId);
        timeoutId = null;
      }
      showFatal("The WebAssembly worker stopped unexpectedly. Reload the page to try again.");
    };
    worker = next;
    return next;
  }

  function onTimeout() {
    if (!running) {
      return;
    }
    timeoutId = null;
    running = false;
    setRunning(false);
    if (worker) {
      worker.terminate();
    }
    showTimeout();
    worker = spawnWorker();
  }

  function run() {
    if (running) {
      return;
    }
    if (!worker) {
      spawnWorker();
    }
    if (!workerReady) {
      pendingRun = true;
      setStatus("Loading WebAssembly…", "loading");
      return;
    }
    setTab("output");
    setRunning(true);
    setStatus("Running", "running");
    lastDiagnostics = [];
    renderDiagnostics([]);
    setPre(stdout, "", "Running…");
    renderGutter();
    runStarted = window.performance.now();
    timeoutId = window.setTimeout(onTimeout, WALL_LIMIT_MS);
    worker.postMessage({
      type: "run",
      code: editor.value,
      maxSteps: MAX_STEPS
    });
  }

  function populateExamples() {
    var custom = document.createElement("option");
    custom.value = "custom";
    custom.textContent = "Custom program";
    examplesSelect.appendChild(custom);
    examples.forEach(function (example) {
      var option = document.createElement("option");
      option.value = example.id;
      option.textContent = example.title;
      examplesSelect.appendChild(option);
    });
  }

  function loadExample(id) {
    var example = examples.filter(function (candidate) {
      return candidate.id === id;
    })[0];
    if (!example) {
      return;
    }
    editor.value = example.code;
    examplesSelect.value = example.id;
    clearResult("Run the example to see its output.");
    editor.scrollTop = 0;
    editor.scrollLeft = 0;
    renderGutter();
  }

  function markCustom() {
    examplesSelect.value = "custom";
  }

  function shareUrl() {
    var base = window.location.href.split("#")[0];
    return base + "#code=" + encodeURIComponent(editor.value);
  }

  function copyText(text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text);
    }
    return new Promise(function (resolve, reject) {
      var temporary = document.createElement("textarea");
      temporary.value = text;
      temporary.setAttribute("readonly", "");
      temporary.style.position = "fixed";
      temporary.style.top = "-1000px";
      document.body.appendChild(temporary);
      temporary.select();
      try {
        var copied = document.execCommand("copy");
        document.body.removeChild(temporary);
        if (copied) {
          resolve();
        } else {
          reject(new Error("copy command failed"));
        }
      } catch (error) {
        document.body.removeChild(temporary);
        reject(error);
      }
    });
  }

  function share() {
    copyText(shareUrl()).then(function () {
      shareButton.textContent = "Copied";
      if (shareResetTimer !== null) {
        window.clearTimeout(shareResetTimer);
      }
      shareResetTimer = window.setTimeout(function () {
        shareButton.textContent = "Share";
        shareResetTimer = null;
      }, 1400);
    }).catch(function () {
      shareButton.textContent = "Copy failed";
      window.setTimeout(function () {
        shareButton.textContent = "Share";
      }, 1800);
    });
  }

  function hashCode() {
    if (!window.location.hash.startsWith("#code=")) {
      return null;
    }
    try {
      return decodeURIComponent(window.location.hash.slice(6));
    } catch (error) {
      return null;
    }
  }

  function bindEvents() {
    runButton.addEventListener("click", run);
    shareButton.addEventListener("click", share);
    clearButton.addEventListener("click", function () {
      editor.value = "";
      markCustom();
      clearResult("The editor is empty. Write English, or choose an example.");
      editor.focus();
    });

    examplesSelect.addEventListener("change", function () {
      if (examplesSelect.value === "custom") {
        return;
      }
      loadExample(examplesSelect.value);
    });

    editor.addEventListener("input", function () {
      markCustom();
      renderGutter();
    });

    editor.addEventListener("scroll", function () {
      gutter.scrollTop = editor.scrollTop;
    });

    editor.addEventListener("keydown", function (event) {
      if (event.key === "Tab" && !event.shiftKey) {
        event.preventDefault();
        var start = editor.selectionStart;
        var end = editor.selectionEnd;
        editor.setRangeText("  ", start, end, "end");
        markCustom();
        renderGutter();
      }
    });

    document.addEventListener("keydown", function (event) {
      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        run();
      }
    });

    tabs.forEach(function (tab, index) {
      tab.addEventListener("click", function () {
        setTab(tab.dataset.panel);
      });
      tab.addEventListener("keydown", function (event) {
        var direction = 0;
        if (event.key === "ArrowRight") {
          direction = 1;
        } else if (event.key === "ArrowLeft") {
          direction = -1;
        } else if (event.key === "Home") {
          direction = -index;
        } else if (event.key === "End") {
          direction = tabs.length - 1 - index;
        }
        if (direction !== 0) {
          event.preventDefault();
          var next = tabs[(index + direction + tabs.length) % tabs.length];
          setTab(next.dataset.panel);
          next.focus();
        }
      });
    });
  }

  function initialize() {
    populateExamples();
    var shared = hashCode();
    if (shared !== null) {
      editor.value = shared;
      examplesSelect.value = "custom";
    } else if (examples.length) {
      editor.value = examples[0].code;
      examplesSelect.value = examples[0].id;
    }
    bindEvents();
    renderGutter();
    setTab("output");
    clearResult(
      shared !== null
        ? "Shared program loaded. Press Run to execute it."
        : "Run a program to see its output."
    );
    spawnWorker();
  }

  initialize();
})();
