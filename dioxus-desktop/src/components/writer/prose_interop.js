(function () {
  if (window.__writerProseInterop) {
    return;
  }

  function escapeHtml(text) {
    return text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/\"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function inlineMarkdownToHtml(input) {
    let out = escapeHtml(input);
    out = out.replace(/`([^`]+)`/g, "<code>$1</code>");
    out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
    out = out.replace(/_([^_]+)_/g, "<em>$1</em>");
    return out;
  }

  function markdownToHtml(markdown) {
    if (!markdown || !markdown.trim()) {
      return "<p><br></p>";
    }
    const blocks = markdown.replace(/\r/g, "").split(/\n\s*\n/);
    const htmlBlocks = blocks.map(function (block) {
      const trimmed = block.trim();
      if (!trimmed) {
        return "<p><br></p>";
      }
      if (/^###\s+/.test(trimmed)) {
        return "<h3>" + inlineMarkdownToHtml(trimmed.replace(/^###\s+/, "")) + "</h3>";
      }
      if (/^##\s+/.test(trimmed)) {
        return "<h2>" + inlineMarkdownToHtml(trimmed.replace(/^##\s+/, "")) + "</h2>";
      }
      if (/^#\s+/.test(trimmed)) {
        return "<h1>" + inlineMarkdownToHtml(trimmed.replace(/^#\s+/, "")) + "</h1>";
      }
      return (
        "<p>" +
        inlineMarkdownToHtml(block)
          .replace(/\n/g, "<br>") +
        "</p>"
      );
    });
    return htmlBlocks.join("");
  }

  function saveSelection(root) {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) {
      return null;
    }
    const range = sel.getRangeAt(0);
    if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) {
      return null;
    }
    const pre = range.cloneRange();
    pre.selectNodeContents(root);
    pre.setEnd(range.startContainer, range.startOffset);
    const start = pre.toString().length;
    return { start: start, end: start + range.toString().length };
  }

  function restoreSelection(root, saved) {
    if (!saved) {
      return;
    }
    const selection = window.getSelection();
    if (!selection) {
      return;
    }
    let charIndex = 0;
    const range = document.createRange();
    range.setStart(root, 0);
    range.collapse(true);

    const nodeStack = [root];
    let foundStart = false;
    let stop = false;

    while (!stop && nodeStack.length > 0) {
      const node = nodeStack.pop();
      if (!node) {
        continue;
      }
      if (node.nodeType === 3) {
        const next = charIndex + node.length;
        if (!foundStart && saved.start >= charIndex && saved.start <= next) {
          range.setStart(node, saved.start - charIndex);
          foundStart = true;
        }
        if (foundStart && saved.end >= charIndex && saved.end <= next) {
          range.setEnd(node, saved.end - charIndex);
          stop = true;
        }
        charIndex = next;
      } else {
        for (let i = node.childNodes.length - 1; i >= 0; i -= 1) {
          nodeStack.push(node.childNodes[i]);
        }
      }
    }

    selection.removeAllRanges();
    selection.addRange(range);
  }

  function inlineHtmlToMarkdown(node) {
    if (!node) {
      return "";
    }
    if (node.nodeType === 3) {
      return node.nodeValue || "";
    }
    if (node.nodeType !== 1) {
      return "";
    }

    const tag = node.tagName.toLowerCase();
    const parts = [];
    node.childNodes.forEach(function (child) {
      parts.push(inlineHtmlToMarkdown(child));
    });
    const inner = parts.join("");

    if (tag === "strong" || tag === "b") {
      return "**" + inner + "**";
    }
    if (tag === "em" || tag === "i") {
      return "_" + inner + "_";
    }
    if (tag === "code") {
      return "`" + inner + "`";
    }
    if (tag === "br") {
      return "\n";
    }
    return inner;
  }

  function htmlToMarkdown(root) {
    if (!root) {
      return "";
    }

    const blocks = [];
    root.childNodes.forEach(function (node) {
      if (node.nodeType === 3) {
        const text = (node.nodeValue || "").trim();
        if (text) {
          blocks.push(text);
        }
        return;
      }
      if (node.nodeType !== 1) {
        return;
      }

      const tag = node.tagName.toLowerCase();
      const text = inlineHtmlToMarkdown(node).trim();
      if (!text) {
        return;
      }
      if (tag === "h1") {
        blocks.push("# " + text);
      } else if (tag === "h2") {
        blocks.push("## " + text);
      } else if (tag === "h3") {
        blocks.push("### " + text);
      } else {
        blocks.push(text);
      }
    });

    if (blocks.length === 0) {
      const fallback = (root.innerText || "").replace(/\r/g, "").trim();
      return fallback;
    }
    return blocks.join("\n\n").trimEnd();
  }

  function setMarkdown(id, markdown) {
    const el = document.getElementById(id);
    if (!el) {
      return;
    }
    const saved = saveSelection(el);
    el.innerHTML = markdownToHtml(markdown || "");
    restoreSelection(el, saved);
  }

  function getMarkdown(id) {
    const el = document.getElementById(id);
    if (!el) {
      return "";
    }
    return htmlToMarkdown(el);
  }

  function applyShortcuts(id) {
    const markdown = getMarkdown(id);
    if (!markdown) {
      return;
    }
    const hasShortcutTokens =
      /\*\*[^*]+\*\*/.test(markdown) ||
      /_[^_]+_/.test(markdown) ||
      /`[^`]+`/.test(markdown) ||
      /^#\s+/m.test(markdown);
    if (!hasShortcutTokens) {
      return;
    }
    setMarkdown(id, markdown);
  }

  function computeBubbleTops(id, count) {
    const total = Number(count) || 0;
    if (total <= 0) {
      return [];
    }
    const el = document.getElementById(id);
    if (!el) {
      return [];
    }
    const rootRect = el.getBoundingClientRect();
    if (!rootRect || rootRect.height <= 0) {
      return [];
    }

    const blocks = Array.from(el.querySelectorAll("h1,h2,h3,p,li,blockquote,pre,div"));
    if (blocks.length === 0) {
      const fallback = [];
      for (let i = 0; i < total; i += 1) {
        fallback.push(Math.round(((i + 1) / (total + 1)) * rootRect.height));
      }
      return fallback;
    }

    const tops = [];
    for (let i = 0; i < total; i += 1) {
      const block = blocks[Math.min(i, blocks.length - 1)];
      const rect = block.getBoundingClientRect();
      const top = rect.top - rootRect.top + Math.min(rect.height / 2, 32);
      tops.push(Math.max(6, Math.round(top)));
    }
    return tops;
  }

  window.__writerProseInterop = {
    setMarkdown: setMarkdown,
    getMarkdown: getMarkdown,
    applyShortcuts: applyShortcuts,
    computeBubbleTops: computeBubbleTops,
  };
})();
