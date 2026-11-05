(() => {
  const $ = (id) => document.getElementById(id);
  const main = $("main");
  const events = [];
  const held = new Map();
  const settingRows = new Map();
  const number = new Intl.NumberFormat();
  const shapes = {
    I: [[0, 1], [1, 1], [2, 1], [3, 1]],
    O: [[1, 0], [2, 0], [1, 1], [2, 1]],
    T: [[1, 0], [0, 1], [1, 1], [2, 1]],
    S: [[1, 0], [2, 0], [0, 1], [1, 1]],
    Z: [[0, 0], [1, 0], [1, 1], [2, 1]],
    J: [[0, 0], [0, 1], [1, 1], [2, 1]],
    L: [[2, 0], [0, 1], [1, 1], [2, 1]],
  };
  const palettes = {
    truecolor: { I: "#70b7bc", O: "#e3bd63", T: "#b69bbd", S: "#96b787", Z: "#d88173", J: "#819cb7", L: "#df9c64", Garbage: "#78827a" },
    "256": { I: "#5fafaf", O: "#d7af5f", T: "#af87af", S: "#87af87", Z: "#d78787", J: "#87afd7", L: "#d7af87", Garbage: "#878787" },
    "16": { I: "#58a6ab", O: "#d6b663", T: "#af86b4", S: "#8aa46e", Z: "#cb7467", J: "#6e8eaa", L: "#cc985b", Garbage: "#758176" },
  };
  let view = null;
  let playing = false;
  let busy = false;
  let requested = false;
  let animation = null;
  let stopped = false;
  let previousScreen = null;
  let previousSelection = null;
  let keyboardSelection = false;
  let nextSignature = null;
  let holdSignature = null;
  let hintSignature = null;
  let lessons = [];
  let lessonFilter = "all";
  let lessonQuery = "";
  let selectedLessonIndex = 0;
  let catalogSignature = null;
  let lessonDetailSignature = null;
  let trainingLessonId = null;
  const lessonButtons = new Map();

  function color(kind) {
    return (palettes[view?.palette] || palettes.truecolor)[kind] || "#78827a";
  }

  function setText(id, value) {
    const node = $(id);
    const text = String(value);
    if (node.textContent !== text) node.textContent = text;
  }

  function formatTime(seconds) {
    const total = Math.max(0, Math.floor(seconds));
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
  }

  function makeElement(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function makeButton(label, action, index, delta, className = "small-button") {
    const button = makeElement("button", className, label);
    button.type = "button";
    button.dataset.action = action;
    if (index !== undefined) button.dataset.index = index;
    if (delta !== undefined) button.dataset.delta = delta;
    return button;
  }

  function preview(node, kind) {
    node.replaceChildren();
    if (!shapes[kind]) return;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 88 50");
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", `${kind} piece`);
    const cells = shapes[kind];
    const minX = Math.min(...cells.map(([x]) => x));
    const maxX = Math.max(...cells.map(([x]) => x));
    const minY = Math.min(...cells.map(([, y]) => y));
    const maxY = Math.max(...cells.map(([, y]) => y));
    const offsetX = (88 - (maxX - minX + 1) * 19) / 2;
    const offsetY = (50 - (maxY - minY + 1) * 19) / 2;
    for (const [x, y] of cells) {
      const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      rect.setAttribute("x", offsetX + (x - minX) * 19);
      rect.setAttribute("y", offsetY + (y - minY) * 19);
      rect.setAttribute("width", 18);
      rect.setAttribute("height", 18);
      rect.setAttribute("rx", 2);
      rect.setAttribute("fill", color(kind));
      svg.append(rect);
      const highlight = document.createElementNS("http://www.w3.org/2000/svg", "rect");
      highlight.setAttribute("x", offsetX + (x - minX) * 19 + 2);
      highlight.setAttribute("y", offsetY + (y - minY) * 19 + 1);
      highlight.setAttribute("width", 14);
      highlight.setAttribute("height", 2);
      highlight.setAttribute("rx", 1);
      highlight.setAttribute("fill", "#ffffff");
      highlight.setAttribute("opacity", ".2");
      svg.append(highlight);
    }
    node.append(svg);
  }

  function drawBoard(canvas, game, hidePieces = false, targets = []) {
    const bounds = canvas.getBoundingClientRect();
    const border = parseFloat(getComputedStyle(canvas).borderLeftWidth) || 0;
    const width = Math.max(1, bounds.width - border * 2);
    const ratio = window.devicePixelRatio || 1;
    const targetWidth = Math.round(width * ratio);
    const targetHeight = Math.round(width * 2 * ratio);
    if (canvas.width !== targetWidth || canvas.height !== targetHeight) {
      canvas.width = targetWidth;
      canvas.height = targetHeight;
    }
    const ctx = canvas.getContext("2d");
    const scale = canvas.width / 300;
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.fillStyle = "#202727";
    ctx.fillRect(0, 0, 300, 600);
    ctx.strokeStyle = "#2c3531";
    ctx.lineWidth = .65;
    ctx.beginPath();
    for (let x = 1; x < 10; x += 1) {
      ctx.moveTo(x * 30, 0);
      ctx.lineTo(x * 30, 600);
    }
    for (let y = 1; y < 20; y += 1) {
      ctx.moveTo(0, y * 30);
      ctx.lineTo(300, y * 30);
    }
    ctx.stroke();
    if (hidePieces) return;

    function block(x, y, kind, ghost = false, clearing = false) {
      if (x < 0 || x > 9 || y < 0 || y > 19) return;
      const left = x * 30 + 1;
      const top = y * 30 + 1;
      ctx.fillStyle = clearing ? "#f2eee0" : color(kind);
      ctx.globalAlpha = ghost ? .12 : 1;
      ctx.beginPath();
      ctx.roundRect(left, top, 28, 28, 2);
      ctx.fill();
      ctx.globalAlpha = 1;
      if (ghost) {
        ctx.strokeStyle = color(kind);
        ctx.lineWidth = 1.2;
        ctx.stroke();
      } else {
        ctx.fillStyle = "#ffffff";
        ctx.globalAlpha = .2;
        ctx.fillRect(left + 2, top + 1, 24, 2);
        ctx.globalAlpha = 1;
      }
    }

    for (let y = 0; y < game.board.length; y += 1) {
      for (let x = 0; x < game.board[y].length; x += 1) {
        const kind = game.board[y][x];
        if (kind) block(x, y, kind, false, game.clearing?.includes(y));
      }
    }
    if (targets.length > 0) {
      ctx.strokeStyle = "#e3bd63";
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 3]);
      for (const cell of targets) {
        if (cell.x >= 0 && cell.x < 10 && cell.y >= 0 && cell.y < 20) ctx.strokeRect(cell.x * 30 + 4, cell.y * 30 + 4, 22, 22);
      }
      ctx.setLineDash([]);
    }
    for (const cell of game.ghost || []) block(cell.x, cell.y, cell.kind, true);
    for (const cell of game.active || []) block(cell.x, cell.y, cell.kind);
  }

  function drawDemo() {
    const rows = Array.from({ length: 20 }, () => Array(10).fill(null));
    const pattern = [
      [null, null, null, null, null, null, null, null, "L", null],
      [null, null, null, null, null, null, "O", "O", "L", null],
      ["J", null, null, null, null, null, "O", "O", "L", "L"],
      ["J", "J", "J", "S", "S", null, "T", "T", "T", "Z"],
      ["I", "I", "I", "I", "S", "S", "T", "Z", "Z", "Z"],
    ];
    pattern.forEach((row, i) => { rows[15 + i] = row; });
    drawBoard($("demo-board"), {
      board: rows,
      active: shapes.T.map(([x, y]) => ({ x: x + 3, y: y + 4, kind: "T" })),
      ghost: shapes.T.map(([x, y]) => ({ x: x + 3, y: y + 13, kind: "T" })),
      clearing: [],
    });
  }

  function renderLessonDiagram(lesson) {
    const node = $("lesson-diagram");
    node.replaceChildren();
    const diagram = lesson.diagram;
    $("lesson-diagram-panel").hidden = !diagram?.length;
    if (!diagram?.length) return;
    const firstFilled = diagram.findIndex((row) => /[^.]/.test(row));
    const rows = diagram.slice(Math.max(0, firstFilled - 1));
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", `0 0 300 ${Math.max(1, rows.length) * 30}`);
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", `Example structure for ${lesson.title}`);
    for (const [y, row] of rows.entries()) {
      for (let x = 0; x < 10; x += 1) {
        const cell = row[x] || ".";
        const rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        rect.setAttribute("x", x * 30 + 1);
        rect.setAttribute("y", y * 30 + 1);
        rect.setAttribute("width", 28);
        rect.setAttribute("height", 28);
        rect.setAttribute("rx", 2);
        rect.setAttribute("fill", cell === "." ? "#29322e" : color(cell === "#" ? "Garbage" : cell));
        svg.append(rect);
      }
    }
    node.append(svg);
  }

  function renderLessonDetail(lesson, index) {
    const signature = `${lesson.id}:${view.palette}:${catalogSignature}`;
    if (signature === lessonDetailSignature) return;
    lessonDetailSignature = signature;
    setText("lesson-category", lesson.category === "opener" ? "OPENER" : "SPIN LAB");
    setText("lesson-difficulty", lesson.difficulty);
    setText("lesson-title", lesson.title);
    setText("lesson-summary", lesson.summary.match(/^.*?[.!?](?:\s|$)/)?.[0].trim() || lesson.summary);
    $("lesson-more").open = false;
    setText("lesson-when", lesson.when);
    $("lesson-steps").replaceChildren(...lesson.steps.map((step) => makeElement("li", null, step)));
    $("lesson-tips").replaceChildren(...lesson.tips.map((tip) => makeElement("li", null, tip)));
    $("lesson-tips-section").hidden = lesson.tips.length === 0;
    setText("lesson-source", lesson.source);
    renderLessonDiagram(lesson);
    const freePractice = lesson.practice === "opener-sandbox";
    const guidedOpener = lesson.practice === "opener-build";
    const button = $("lesson-practice");
    button.dataset.index = index;
    button.hidden = !lesson.practice;
    button.firstChild.textContent = freePractice ? "Free practice " : guidedOpener ? "Build first bag " : "Start challenge ";
    setText("lesson-practice-note", freePractice ? "Ungraded practice." : guidedOpener ? "First bag only." : lesson.practice ? "" : "Reference only.");
    $("lesson-practice-note").hidden = Boolean(lesson.practice) && !freePractice && !guidedOpener;
  }

  function renderLearn() {
    if (view.lessons?.length) lessons = view.lessons;
    const signature = JSON.stringify(lessons);
    if (signature !== catalogSignature) {
      catalogSignature = signature;
      lessonDetailSignature = null;
      lessonButtons.clear();
      $("lesson-list").replaceChildren();
      for (const [index, lesson] of lessons.entries()) {
        const button = makeElement("button", "lesson-card");
        button.type = "button";
        button.dataset.lessonIndex = index;
        const meta = makeElement("span", "lesson-card-meta");
        meta.append(makeElement("span", null, lesson.category === "opener" ? "OPENER" : "SPIN LAB"), makeElement("span", null, lesson.difficulty));
        button.append(meta, makeElement("strong", null, lesson.title));
        $("lesson-list").append(button);
        lessonButtons.set(index, button);
      }
    }
    const matches = lessons.map((lesson, index) => ({ lesson, index })).filter(({ lesson }) => {
      const searchable = `${lesson.title} ${lesson.summary} ${lesson.difficulty} ${lesson.when} ${lesson.steps.join(" ")}`.toLowerCase();
      return (lessonFilter === "all" || lesson.category === lessonFilter) && searchable.includes(lessonQuery);
    });
    if (!matches.some(({ index }) => index === selectedLessonIndex)) selectedLessonIndex = matches[0]?.index ?? -1;
    const visibleIndices = new Set(matches.map(({ index }) => index));
    for (const [index, button] of lessonButtons) {
      button.hidden = !visibleIndices.has(index);
      button.setAttribute("aria-pressed", String(index === selectedLessonIndex));
    }
    document.querySelectorAll("[data-lesson-filter]").forEach((button) => {
      button.setAttribute("aria-pressed", String(button.dataset.lessonFilter === lessonFilter));
    });
    setText("lesson-count", `${matches.length} ${matches.length === 1 ? "lesson" : "lessons"}`);
    $("lesson-empty").hidden = matches.length > 0;
    $("lesson-detail").hidden = selectedLessonIndex < 0;
    if (selectedLessonIndex >= 0) renderLessonDetail(lessons[selectedLessonIndex], selectedLessonIndex);
  }

  function renderTraining() {
    const training = view.training;
    $("training-panel").hidden = !training;
    $("training-library-button").hidden = !training;
    $("attempt-stat").hidden = !training;
    $("best-stat").hidden = Boolean(training);
    $("level-stat").hidden = Boolean(training);
    if (!training) return;
    const { lesson, sandbox, status } = training;
    const guidedOpener = lesson.practice === "opener-build";
    const successful = status === "success" && !sandbox;
    const ended = status === "retry" || status === "success";
    const paused = view.screen === "Paused";
    const gameOver = view.screen === "Game over";
    setText("game-mode", sandbox ? "FREE PRACTICE" : guidedOpener ? "FIRST BAG" : "SPIN CHALLENGE");
    setText("game-heading", lesson.title);
    $("training-progress").hidden = !guidedOpener || training.total_steps === 0;
    setText("training-progress-count", `${training.step} / ${training.total_steps}`);
    $("training-progress-meter").max = Math.max(1, training.total_steps);
    $("training-progress-meter").value = training.step;
    document.querySelector(".training-hint").hidden = ended || gameOver;
    setText("training-hint", training.hint);
    setText("attempts", training.attempts);
    $("training-target-note").hidden = training.target.length === 0;
    $("training-sandbox-note").hidden = !sandbox;
    if (trainingLessonId !== lesson.id) {
      $("training-steps").replaceChildren(...lesson.steps.map((step) => makeElement("li", null, step)));
      document.querySelector(".training-steps").open = false;
      trainingLessonId = lesson.id;
    }
    $("game-toast").hidden = true;
    document.querySelector(".pause-button").hidden = paused || ended || gameOver;
    $("overlay-tertiary").dataset.action = "learn";
    setText("overlay-tertiary", "Lessons");
    $("overlay-secondary").dataset.action = "retry-lesson";
    setText("overlay-secondary", "Reset");
    if (paused) {
      $("overlay-description").hidden = true;
    } else if (ended || gameOver) {
      $("game-overlay").hidden = false;
      $("result-stats").hidden = true;
      setText("overlay-title", sandbox ? "Game over" : successful ? guidedOpener ? "First bag complete" : "Complete" : "Try again");
      setText("overlay-description", training.feedback);
      $("overlay-description").hidden = !training.feedback || training.feedback.replace(/[.!?]/g, "").trim().toLowerCase() === $("overlay-title").textContent.toLowerCase();
      const primary = $("overlay-primary");
      primary.dataset.action = "retry-lesson";
      primary.firstChild.textContent = "Retry ";
      $("overlay-secondary").hidden = false;
      $("overlay-secondary").dataset.action = "learn";
      setText("overlay-secondary", "Lessons");
      $("overlay-tertiary").dataset.action = "title";
      setText("overlay-tertiary", "Menu");
    }
  }

  function renderTitle() {
    document.querySelectorAll(".mode-option").forEach((button) => {
      button.setAttribute("aria-pressed", String(Number(button.dataset.index) === view.mode));
    });
    $("difficulty").hidden = view.mode !== 1;
    setText("bot-level", view.bot_level);
    setText("title-best", number.format(view.best));
    $("show-records").hidden = view.high_scores.length === 0;
    const enoughSpace = window.innerWidth >= (view.mode === 1 ? 540 : 300) && window.innerHeight >= 360;
    document.querySelector(".start-button").disabled = !enoughSpace;
    $("start-hint").hidden = enoughSpace;
    if (!enoughSpace) setText("start-hint", "Enlarge the window to play.");
    drawDemo();
  }

  function renderGame() {
    const game = view.player;
    const versus = Boolean(view.opponent);
    const training = view.training;
    $("arena").classList.toggle("training", Boolean(training));
    $("overlay-secondary").dataset.action = "restart";
    setText("overlay-secondary", "Restart");
    $("overlay-tertiary").dataset.action = "title";
    setText("overlay-tertiary", "Menu");
    const paused = view.screen === "Paused";
    const finished = ["Game over", "You win", "You lose"].includes(view.screen);
    $("arena").classList.toggle("versus", versus);
    $("opponent-column").hidden = !versus;
    $("attack-stat").hidden = !versus;
    $("garbage-meter").hidden = !versus || game.garbage === 0;
    $("garbage-meter").setAttribute("aria-label", `${game.garbage} incoming garbage lines`);
    $("garbage-meter").firstElementChild.style.height = `${Math.min(100, game.garbage * 5)}%`;
    document.querySelector(".pause-button").hidden = paused || finished;
    $("game-mode").hidden = !training;
    setText("game-heading", versus ? "Versus AI" : "Marathon");
    setText("score", number.format(game.score));
    setText("best", number.format(Math.max(game.score, view.best)));
    setText("level", game.level);
    setText("lines", game.lines);
    setText("time", formatTime(game.elapsed));
    setText("finesse", `${Math.round(game.finesse)}%`);
    $("finesse").title = `${game.faults} finesse faults`;
    const pauseControl = view.controls.find((control) => control.label === "Pause");
    setText("pause-key", pauseControl?.keys || "Esc");
    setText("attack", game.attack);
    setText("piece-count", `${game.pieces} PIECES`);
    const hold = `${game.hold}:${game.can_hold}:${view.palette}`;
    if (hold !== holdSignature) {
      preview($("hold-preview"), game.hold);
      $("hold-preview").classList.toggle("unavailable", !game.can_hold);
      holdSignature = hold;
    }
    const next = `${game.next.join(",")}:${view.palette}`;
    if (next !== nextSignature) {
      const previews = $("next-previews");
      previews.replaceChildren();
      for (const kind of game.next) {
        const item = makeElement("div", "piece-preview");
        preview(item, kind);
        previews.append(item);
      }
      nextSignature = next;
    }
    $("game-toast").hidden = !game.toast || paused || finished;
    setText("game-toast", game.toast || "");
    drawBoard($("player-board"), game, paused, training?.target || []);
    if (versus) {
      drawBoard($("opponent-board"), view.opponent, paused);
      setText("opponent-level", `LEVEL ${view.bot_level}`);
      setText("opponent-lines", `${view.opponent.lines} LINES`);
      setText("opponent-attack", `${view.opponent.attack} SENT`);
    }
    $("game-overlay").hidden = !paused && !finished;
    if (paused || finished) {
      setText("overlay-title", paused ? "Paused" : view.screen);
      $("overlay-description").hidden = true;
      $("result-stats").hidden = paused;
      setText("result-score", number.format(game.score));
      const primary = $("overlay-primary");
      primary.dataset.action = paused ? "resume" : "restart";
      primary.firstChild.textContent = paused ? "Resume " : "Play again ";
      $("overlay-secondary").hidden = !paused;
    }
    const holdControl = view.controls.find((control) => control.label === "Hold");
    setText("hold-key", holdControl?.keys || "C");
    renderTraining();
  }

  function createSetting(item) {
    if (item.kind === "header") {
      const header = makeElement("h2", "settings-section", item.label);
      $("settings-items").append(header);
      return { node: header };
    }
    if (!["field", "key"].includes(item.kind)) return null;
    const node = makeElement("div", "setting-row");
    const copy = makeElement("div", "setting-copy");
    const label = makeElement("h3", null, item.label);
    const description = makeElement("p", null, item.description);
    label.id = `setting-label-${item.index}`;
    copy.append(label, description);
    const controls = makeElement("div", `setting-controls${item.kind === "key" ? " key-controls" : ""}`);
    let value;
    let toggle;
    if (item.kind === "key") {
      value = makeElement("div", "setting-keys");
      controls.append(value);
      const change = makeButton("Change", "rebind", item.index);
      const add = makeButton("+", "add-key", item.index);
      const reset = makeButton("Reset", "reset-key", item.index, undefined, "small-button quiet");
      change.setAttribute("aria-label", `Change ${item.label} keys`);
      add.setAttribute("aria-label", `Add a key for ${item.label}`);
      reset.setAttribute("aria-label", `Restore ${item.label} default keys`);
      controls.append(change, add, reset);
    } else if (item.toggle) {
      toggle = makeButton("", "setting", item.index, 1, "toggle");
      toggle.setAttribute("role", "switch");
      toggle.setAttribute("aria-labelledby", label.id);
      toggle.append(makeElement("span"));
      controls.append(toggle);
    } else {
      const stepper = makeElement("div", "stepper");
      const decrease = makeButton("−", "setting", item.index, -1, "");
      const increase = makeButton("+", "setting", item.index, 1, "");
      decrease.setAttribute("aria-label", `Decrease ${item.label}`);
      increase.setAttribute("aria-label", `Increase ${item.label}`);
      value = makeElement("span", "setting-value");
      stepper.append(decrease, value, increase);
      controls.append(stepper);
    }
    node.append(copy, controls);
    $("settings-items").append(node);
    return { node, label, description, value, toggle, kind: item.kind };
  }

  function renderSettings() {
    const settings = view.settings;
    if (!settings) return;
    let selected;
    for (const item of settings.items) {
      if (item.selected) selected = item;
      let row = settingRows.get(item.index);
      if (!row) {
        row = createSetting(item);
        if (row) settingRows.set(item.index, row);
      }
      if (!row || !row.label) continue;
      row.node.classList.toggle("selected", item.selected);
      row.label.textContent = item.label;
      row.description.textContent = item.description || "";
      row.description.hidden = !item.description;
      if (row.toggle) row.toggle.setAttribute("aria-checked", String(item.value.toLowerCase() === "on" || item.value.toLowerCase() === "true"));
      if (row.value && row.kind !== "key") row.value.textContent = item.value;
      if (row.value && row.kind === "key" && row.value.dataset.value !== item.value) {
        row.value.replaceChildren();
        for (const key of item.value.split(" / ")) row.value.append(makeElement("kbd", null, key));
        row.value.dataset.value = item.value;
      }
    }
    $("capture-banner").hidden = !settings.capturing;
    setText("capture-banner", `Press a key for ${selected?.label || "this control"}. Esc and Enter are valid keys.`);
    $("settings-status").hidden = !settings.status;
    setText("settings-status", settings.status || "");
    if (keyboardSelection && selected && previousSelection !== selected.index) {
      settingRows.get(selected.index)?.node.scrollIntoView({ block: "nearest" });
    }
    previousSelection = selected?.index;
  }

  function hint(keys, label) {
    const item = makeElement("span");
    for (const key of keys) item.append(makeElement("kbd", null, key));
    item.append(document.createTextNode(` ${label}`));
    return item;
  }

  function renderHints() {
    const signature = `${view.screen}:${view.controls.map((entry) => entry.keys).join(";")}`;
    if (signature === hintSignature) return;
    hintSignature = signature;
    const hints = $("key-hints");
    hints.replaceChildren();
    if (view.screen === "Title") {
      hints.append(hint(["↑", "↓"], "choose"), hint(["Enter"], "play"), hint([view.controls.find((entry) => entry.label === "Settings")?.keys || "S"], "settings"));
    } else if (view.screen === "Learn") {
      hints.append(hint(["Tab"], "browse"), hint(["Enter"], "select"), hint(["Esc"], "menu"));
    } else if (view.screen === "Settings") {
      hints.append(hint(["↑", "↓"], "navigate"), hint(["←", "→"], "adjust"), hint(["Esc"], "done"));
    } else {
      const shortcuts = [["Move left", "Move right", "move"], ["Rotate ↻", null, "rotate"], ["Hard drop", null, "drop"], ["Hold", null, "hold"]];
      const fallback = { "Move left": "←", "Move right": "→", "Rotate ↻": "↑ / X", "Hard drop": "Space", Hold: "C" };
      for (const [first, second, label] of shortcuts) {
        const control = (name) => view.controls.find((entry) => entry.label === name)?.keys || fallback[name];
        const keys = [control(first)];
        if (second) keys.push(control(second));
        hints.append(hint(keys, label));
      }
    }
  }

  function render() {
    if (!view) return;
    const title = view.screen === "Title";
    const settings = view.screen === "Settings";
    const learn = view.screen === "Learn";
    $("learn-screen").hidden = !learn;
    $("title-screen").hidden = !title;
    $("play-screen").hidden = title || settings || learn;
    $("settings-screen").hidden = !settings;
    $("settings-button").hidden = settings;
    $("settings-button").title = `Settings (${view.controls.find((entry) => entry.label === "Settings")?.keys || "S"})`;
    if (title) renderTitle();
    else if (learn) renderLearn();
    else if (settings) renderSettings();
    else renderGame();
    const screenChanged = previousScreen !== view.screen;
    if (screenChanged || settings || !title) renderHints();
    if (screenChanged) {
      main.setAttribute("aria-label", `bostris — ${view.screen}`);
      if (!settings) main.focus({ preventScroll: true });
    }
    previousScreen = view.screen;
  }

  function dimensions() {
    return { width: Math.max(1, Math.min(8192, window.innerWidth)), height: Math.max(1, Math.min(8192, window.innerHeight)) };
  }

  function schedule() {
    if (animation !== null || stopped || document.hidden) return;
    if (!playing && !requested && events.length === 0) return;
    animation = requestAnimationFrame(() => {
      animation = null;
      pump();
    });
  }

  async function pump() {
    if (busy || stopped) return;
    busy = true;
    requested = false;
    const batch = events.splice(0);
    try {
      const invoke = window.__TAURI__?.core?.invoke;
      if (!invoke) throw new Error("Open bostris in the desktop app to play.");
      const frame = await invoke("frame", { ...dimensions(), events: batch });
      $("failure").hidden = true;
      playing = frame.playing;
      stopped = frame.quit;
      setText("status", frame.error || "");
      $("status").hidden = !frame.error;
      if (frame.view) {
        view = frame.view;
        render();
      }
    } catch (error) {
      stopped = true;
      playing = false;
      held.clear();
      events.length = 0;
      setText("failure-message", String(error?.message || error));
      $("failure").hidden = false;
      $("retry").focus();
    } finally {
      busy = false;
      if (requested || events.length > 0) pump();
      else schedule();
    }
  }

  function request() {
    requested = true;
    if (animation !== null) {
      cancelAnimationFrame(animation);
      animation = null;
    }
    pump();
  }

  function keyName(key) {
    const names = { ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down", " ": "Space", Escape: "Esc" };
    if (Object.hasOwn(names, key)) return names[key];
    if (["Enter", "Tab", "Backspace", "Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(key)) return key;
    if (/^F([1-9]|1[0-2])$/.test(key) || [...key].length === 1) return key;
    return null;
  }

  window.addEventListener("keydown", (event) => {
    if (event.metaKey || event.altKey || stopped || $("records").open) return;
    const capturing = Boolean(view?.settings?.capturing);
    if (view?.screen === "Learn") {
      if (event.key === "Escape") {
        event.preventDefault();
        events.push({ action: "title" });
        request();
      }
      return;
    }
    if (!capturing && (event.target.closest?.("input, select, textarea") || event.target.isContentEditable)) return;
    const nativeControl = event.target.closest?.("button, input, select, textarea, a, summary");
    if (!capturing && (event.key === "Tab" || (nativeControl && [" ", "Enter", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(event.key)))) return;
    const key = keyName(event.key);
    if (!key) return;
    event.preventDefault();
    const identity = event.code || event.key;
    const original = held.get(identity) || key;
    held.set(identity, original);
    keyboardSelection = true;
    events.push({ key: original, kind: event.repeat ? "repeat" : "press", control: event.ctrlKey });
    request();
  });

  window.addEventListener("keyup", (event) => {
    const identity = event.code || event.key;
    const key = held.get(identity);
    held.delete(identity);
    if (!key || stopped) return;
    event.preventDefault();
    events.push({ key, kind: "release", control: event.ctrlKey });
    request();
  });

  document.addEventListener("click", (event) => {
    const filterButton = event.target.closest?.("[data-lesson-filter]");
    if (filterButton && view?.screen === "Learn") {
      lessonFilter = filterButton.dataset.lessonFilter;
      renderLearn();
      return;
    }
    const lessonButton = event.target.closest?.("[data-lesson-index]");
    if (lessonButton && view?.screen === "Learn") {
      selectedLessonIndex = Number(lessonButton.dataset.lessonIndex);
      renderLearn();
      $("lesson-detail").scrollTop = 0;
      return;
    }
    const button = event.target.closest?.("[data-action]");
    if (!button || stopped) return;
    event.preventDefault();
    const action = button.dataset.action;
    const input = { action };
    if (button.dataset.index !== undefined) input.index = Number(button.dataset.index);
    if (button.dataset.delta !== undefined) input.delta = Number(button.dataset.delta);
    keyboardSelection = false;
    events.push(input);
    if (!["setting", "rebind", "add-key", "reset-key", "reset-settings"].includes(action)) main.focus({ preventScroll: true });
    request();
  });

  function blur() {
    if (stopped) return;
    held.clear();
    events.push({ blur: true });
    request();
  }

  window.addEventListener("blur", blur);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) blur();
    else request();
  });
  window.addEventListener("resize", () => {
    if (view) render();
    request();
  });
  $("lesson-search").addEventListener("input", (event) => {
    lessonQuery = event.target.value.trim().toLowerCase();
    renderLearn();
  });
  $("show-records").addEventListener("click", () => {
    if (!view) return;
    const body = $("records-body");
    body.replaceChildren();
    for (const [index, entry] of view.high_scores.entries()) {
      const row = document.createElement("tr");
      row.append(makeElement("td", null, String(index + 1).padStart(2, "0")), makeElement("td", null, number.format(entry.score)), makeElement("td", null, entry.lines), makeElement("td", null, entry.level));
      body.append(row);
    }
    $("records-empty").hidden = view.high_scores.length > 0;
    $("records-table").hidden = view.high_scores.length === 0;
    $("records").showModal();
  });
  $("close-records").addEventListener("click", () => $("records").close());
  $("retry").addEventListener("click", () => {
    stopped = false;
    main.focus({ preventScroll: true });
    events.push({ blur: true });
    request();
  });
  request();
})();
