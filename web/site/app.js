(function () {
  "use strict";

  var MAX_STEPS = 2000000;
  var WALL_LIMIT_MS = 5000;
  var HIGHLIGHT_DELAY_MS = 120;

  var editorInput = document.getElementById("editor");
  var highlight = document.getElementById("highlight");
  var gutter = document.getElementById("gutter");
  var examplesSelect = document.getElementById("examples");
  var runButton = document.getElementById("run");
  var shareButton = document.getElementById("share");
  var resetButton = document.getElementById("reset");
  var clearButton = document.getElementById("clear");
  var copyTabButton = document.getElementById("copy-tab");
  var versionLabel = document.getElementById("version");
  var caption = document.getElementById("example-caption");
  var stdout = document.getElementById("stdout");
  var diagnosticsRoot = document.getElementById("diagnostics");
  var traceRoot = document.getElementById("trace");
  var paraphraseOutput = document.getElementById("paraphrase");
  var irOutput = document.getElementById("ir");
  var statusDot = document.getElementById("status-dot");
  var statusText = document.getElementById("status-text");
  var tabs = Array.prototype.slice.call(document.querySelectorAll(".tab"));

  var examples = window.TC_EXAMPLES || [];
  var currentTokens = [];
  var lastDiagnostics = [];
  var lastResult = null;
  var activeTab = "output";
  var activeExampleId = null;
  var runCode = "";
  var worker = null;
  var workerReady = false;
  var running = false;
  var pendingRun = false;
  var pendingHighlight = false;
  var timeoutId = null;
  var highlightTimer = null;
  var highlightSequence = 0;
  var activeHighlightId = 0;
  var highlightCode = "";
  var runStarted = 0;
  var statusState = "ready";
  var shareResetTimer = null;
  var copyResetTimer = null;
  var resizeTimer = null;

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

  function setCaption(text) {
    caption.textContent = text;
  }

  function setTab(name) {
    activeTab = name;
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

  function updateCopyState() {
    copyTabButton.disabled = !lastResult;
  }

  function syncScroll() {
    highlight.scrollTop = editorInput.scrollTop;
    highlight.scrollLeft = editorInput.scrollLeft;
    gutter.scrollTop = editorInput.scrollTop;
  }

  function lineHeight() {
    var value = parseFloat(window.getComputedStyle(editorInput).lineHeight);
    return Number.isFinite(value) && value > 0 ? value : 31;
  }

  function appendText(parent, text) {
    if (text) {
      parent.appendChild(document.createTextNode(text));
    }
  }

  function appendToken(parent, text, kind) {
    if (!text) {
      return;
    }
    if (kind === "comment" || (kind === "string" && text.length <= 40)) {
      var block = document.createElement("span");
      block.className = "token-" + kind;
      block.textContent = text;
      parent.appendChild(block);
      return;
    }
    if (kind === "anaphor" || kind === "string") {
      var pieces = text.split(/(\s+)/);
      pieces.forEach(function (piece) {
        if (!piece) {
          return;
        }
        if (/^\s+$/.test(piece)) {
          appendText(parent, piece);
          return;
        }
        var span = document.createElement("span");
        span.className = "token-" + kind;
        span.textContent = piece;
        parent.appendChild(span);
      });
      return;
    }
    var token = document.createElement("span");
    token.className = "token-" + kind;
    token.textContent = text;
    parent.appendChild(token);
  }

  function renderEditor() {
    var text = editorInput.value;
    var lines = text.split("\n");
    var tokensByLine = {};
    currentTokens.forEach(function (token) {
      if (!tokensByLine[token.line]) {
        tokensByLine[token.line] = [];
      }
      tokensByLine[token.line].push(token);
    });

    var fragment = document.createDocumentFragment();
    lines.forEach(function (line, lineNumber) {
      var lineElement = document.createElement("div");
      lineElement.className = "highlight-line";
      var lineTokens = (tokensByLine[lineNumber] || []).slice().sort(function (left, right) {
        return left.start - right.start || right.len - left.len;
      });
      var cursor = 0;
      lineTokens.forEach(function (token) {
        if (token.start < cursor || token.start >= line.length || token.len <= 0) {
          return;
        }
        var end = Math.min(line.length, token.start + token.len);
        if (end <= token.start) {
          return;
        }
        appendText(lineElement, line.slice(cursor, token.start));
        appendToken(lineElement, line.slice(token.start, end), token.kind);
        cursor = end;
      });
      appendText(lineElement, line.slice(cursor));
      if (!lineElement.textContent) {
        lineElement.textContent = "\u200b";
      }
      fragment.appendChild(lineElement);
    });

    highlight.textContent = "";
    highlight.appendChild(fragment);
    renderGutter();
    syncScroll();
  }

  function renderGutter() {
    var lineElements = highlight.children;
    var fallback = lineHeight();
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
    for (var index = 0; index < lineElements.length; index += 1) {
      var line = index + 1;
      var row = document.createElement("div");
      row.className = "gutter-line";
      if (errorLines[line]) {
        row.classList.add("has-error");
      } else if (warningLines[line]) {
        row.classList.add("has-warning");
      }
      var element = lineElements[index];
      var height = element ? element.offsetHeight : fallback;
      row.style.height = (height || fallback) + "px";
      row.textContent = String(line);
      fragment.appendChild(row);
    }
    gutter.textContent = "";
    gutter.appendChild(fragment);
    gutter.scrollTop = editorInput.scrollTop;
  }

  function placeCursor(line, column) {
    var lines = editorInput.value.split("\n");
    var target = Math.min(Math.max(line - 1, 0), lines.length - 1);
    var offset = 0;
    for (var index = 0; index < target; index += 1) {
      offset += lines[index].length + 1;
    }
    var caret = offset + Math.min(Math.max(column - 1, 0), lines[target].length);
    editorInput.focus();
    editorInput.setSelectionRange(caret, caret);
    var element = highlight.children[target];
    var desired = element ? element.offsetTop - editorInput.clientHeight / 3 : 0;
    editorInput.scrollTop = Math.max(0, desired);
    syncScroll();
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
      parts.push(
        "verb " + entry.verb +
          (entry.lemma && entry.lemma !== entry.verb ? " (" + entry.lemma + ")" : "")
      );
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
      if (entry.intent === "COMMENT_OMITTED") {
        article.classList.add("omitted");
      }

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
        if (entry.intent === "COMMENT_OMITTED") {
          reason.classList.add("muted");
        }
        reason.textContent = entry.reason;
        article.appendChild(reason);
      }
      traceRoot.appendChild(article);
    });
  }

  function applyTokens(tokens, id) {
    if (typeof id === "number" && id !== activeHighlightId) {
      return;
    }
    currentTokens = Array.isArray(tokens) ? tokens : [];
    renderEditor();
  }

  function renderResult(result, elapsed) {
    lastResult = result;
    if (result.version) {
      versionLabel.textContent = "v" + result.version;
    }
    if (runCode === editorInput.value && Array.isArray(result.tokens)) {
      applyTokens(result.tokens, null);
    } else {
      pendingHighlight = true;
    }

    lastDiagnostics = Array.isArray(result.diagnostics) ? result.diagnostics : [];
    if (result.stdout) {
      setPre(stdout, result.stdout, "");
    } else {
      setPre(
        stdout,
        "",
        result.ok ? "Program finished with no output." : "No output."
      );
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
    updateCopyState();
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

    if (pendingHighlight && !running) {
      pendingHighlight = false;
      sendHighlight();
    }
  }

  function clearResult(message) {
    lastResult = null;
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
    updateCopyState();
    renderGutter();
    setStatus("Ready", "ready");
  }

  function showTimeout() {
    var seconds = Math.round(WALL_LIMIT_MS / 100) / 10;
    var message =
      "Execution stopped: the " + seconds +
      "-second wall-clock limit was exceeded. A loop may be running without progress. The worker was terminated and restarted.";
    lastResult = null;
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
    updateCopyState();
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
    lastResult = null;
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
    updateCopyState();
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
      } else if (pendingHighlight) {
        pendingHighlight = false;
        sendHighlight();
      }
      if (statusState === "loading" || statusState === "ready") {
        setStatus("Ready", "ready");
      }
      return;
    }
    if (data.type === "tokens") {
      if (data.id === activeHighlightId && editorInput.value === highlightCode) {
        applyTokens(data.tokens, data.id);
      }
      return;
    }
    if (data.type === "fatal") {
      workerReady = false;
      pendingRun = false;
      pendingHighlight = false;
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
      lastResult = null;
      lastDiagnostics = [{
        line: 1,
        col: 1,
        severity: "error",
        category: "WORKER",
        message: data.message || "The WebAssembly worker failed."
      }];
      setPre(stdout, "", "The program did not finish.");
      renderDiagnostics(lastDiagnostics);
      updateCopyState();
      renderGutter();
      setStatus("Worker error", "error");
      pendingHighlight = true;
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
    pendingHighlight = true;
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
    lastResult = null;
    updateCopyState();
    lastDiagnostics = [];
    renderDiagnostics([]);
    setPre(stdout, "", "Running…");
    renderGutter();
    runCode = editorInput.value;
    runStarted = window.performance.now();
    timeoutId = window.setTimeout(onTimeout, WALL_LIMIT_MS);
    worker.postMessage({
      type: "run",
      code: runCode,
      maxSteps: MAX_STEPS
    });
  }

  function scheduleHighlight(immediate) {
    if (highlightTimer !== null) {
      window.clearTimeout(highlightTimer);
      highlightTimer = null;
    }
    if (immediate) {
      sendHighlight();
      return;
    }
    highlightTimer = window.setTimeout(function () {
      highlightTimer = null;
      sendHighlight();
    }, HIGHLIGHT_DELAY_MS);
  }

  function sendHighlight() {
    if (!workerReady || running) {
      pendingHighlight = true;
      return;
    }
    var id = ++highlightSequence;
    activeHighlightId = id;
    highlightCode = editorInput.value;
    worker.postMessage({
      type: "highlight",
      id: id,
      code: highlightCode
    });
  }

  function populateExamples() {
    var custom = document.createElement("option");
    custom.value = "custom";
    custom.textContent = "Custom program";
    examplesSelect.appendChild(custom);

    var groups = {};
    examples.forEach(function (example) {
      var groupName = example.group || "Programs";
      var group = groups[groupName];
      if (!group) {
        group = document.createElement("optgroup");
        group.label = groupName;
        groups[groupName] = group;
        examplesSelect.appendChild(group);
      }
      var option = document.createElement("option");
      option.value = example.id;
      option.textContent = example.title;
      group.appendChild(option);
    });
  }

  function findExample(id) {
    return examples.filter(function (candidate) {
      return candidate.id === id;
    })[0];
  }

  function loadExample(id) {
    var example = findExample(id);
    if (!example) {
      return;
    }
    activeExampleId = example.id;
    editorInput.value = example.code;
    examplesSelect.value = example.id;
    setCaption(example.summary);
    resetButton.disabled = false;
    currentTokens = [];
    renderEditor();
    clearResult("Run the example to see its output.");
    editorInput.scrollTop = 0;
    highlight.scrollTop = 0;
    gutter.scrollTop = 0;
    scheduleHighlight(true);
  }

  function markCustom() {
    examplesSelect.value = "custom";
    setCaption("Custom program");
  }

  function onEditorInput() {
    markCustom();
    lastResult = null;
    lastDiagnostics = [];
    renderDiagnostics([]);
    updateCopyState();
    renderEditor();
    setStatus("Edited · run to refresh", "ready");
    scheduleHighlight(false);
  }

  function shareUrl() {
    var base = window.location.href.split("#")[0];
    return base + "#code=" + encodeURIComponent(editorInput.value);
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

  function activeTabText() {
    if (!lastResult) {
      return "";
    }
    if (activeTab === "output") {
      var text = lastResult.stdout || "";
      lastDiagnostics.forEach(function (diagnostic) {
        text += (text ? "\n" : "") +
          diagnostic.line + ":" + diagnostic.col + " " + diagnostic.message;
      });
      return text;
    }
    if (activeTab === "trace") {
      return (lastResult.trace || []).map(function (entry) {
        var line = "L" + entry.line + ":" + entry.col + " " + entry.intent + " " + entry.text;
        if (entry.reason) {
          line += "\n  " + entry.reason;
        }
        return line;
      }).join("\n");
    }
    if (activeTab === "paraphrase") {
      return lastResult.paraphrase || "";
    }
    return lastResult.ir || "";
  }

  function copyActiveTab() {
    if (!lastResult) {
      return;
    }
    copyText(activeTabText()).then(function () {
      copyTabButton.textContent = "Copied";
      if (copyResetTimer !== null) {
        window.clearTimeout(copyResetTimer);
      }
      copyResetTimer = window.setTimeout(function () {
        copyTabButton.textContent = "Copy";
        copyResetTimer = null;
      }, 1200);
    }).catch(function () {
      copyTabButton.textContent = "Copy failed";
      window.setTimeout(function () {
        copyTabButton.textContent = "Copy";
      }, 1600);
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
    copyTabButton.addEventListener("click", copyActiveTab);

    resetButton.addEventListener("click", function () {
      if (activeExampleId) {
        loadExample(activeExampleId);
        editorInput.focus();
      }
    });

    clearButton.addEventListener("click", function () {
      editorInput.value = "";
      currentTokens = [];
      activeExampleId = null;
      resetButton.disabled = true;
      markCustom();
      renderEditor();
      clearResult("The editor is empty. Write English, or choose an example.");
      scheduleHighlight(true);
      editorInput.focus();
    });

    examplesSelect.addEventListener("change", function () {
      if (examplesSelect.value === "custom") {
        setCaption("Custom program");
        return;
      }
      loadExample(examplesSelect.value);
    });

    editorInput.addEventListener("input", onEditorInput);
    editorInput.addEventListener("scroll", syncScroll);

    editorInput.addEventListener("keydown", function (event) {
      if (event.key === "Tab" && !event.shiftKey) {
        event.preventDefault();
        var start = editorInput.selectionStart;
        var end = editorInput.selectionEnd;
        editorInput.setRangeText("  ", start, end, "end");
        onEditorInput();
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

    window.addEventListener("resize", function () {
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      resizeTimer = window.setTimeout(function () {
        resizeTimer = null;
        renderGutter();
        syncScroll();
      }, 80);
    });

    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(function () {
        renderGutter();
        syncScroll();
      });
    }
  }

  function initialize() {
    populateExamples();
    var shared = hashCode();
    var defaultExample = findExample("fibonacci") || examples[0] || null;
    activeExampleId = defaultExample ? defaultExample.id : null;

    if (shared !== null) {
      editorInput.value = shared;
      examplesSelect.value = "custom";
      setCaption("Shared program");
      clearResult("Shared program loaded. Press Run to execute it.");
    } else if (defaultExample) {
      editorInput.value = defaultExample.code;
      examplesSelect.value = defaultExample.id;
      setCaption(defaultExample.summary);
      clearResult("Run the example to see its output.");
    } else {
      setCaption("Custom program");
      clearResult("Run a program to see its output.");
    }

    resetButton.disabled = !activeExampleId;
    bindEvents();
    currentTokens = [];
    renderEditor();
    setTab("output");
    updateCopyState();
    spawnWorker();
    scheduleHighlight(true);
    editorInput.scrollTop = 0;
    highlight.scrollTop = 0;
    gutter.scrollTop = 0;
  }

  initialize();
})();
