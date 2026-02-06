(function () {
  const viewers = new Map();
  let nextId = 1;

  window.createTextViewer = function createTextViewer(container, options = {}) {
    if (!container) return 0;

    const textarea = document.createElement("textarea");
    textarea.className = "viewer-textarea";
    textarea.spellcheck = false;
    textarea.value = options.initialContent || "";
    textarea.readOnly = !!options.readOnly;
    textarea.style.width = "100%";
    textarea.style.height = "100%";
    textarea.style.boxSizing = "border-box";
    textarea.style.border = "none";
    textarea.style.outline = "none";
    textarea.style.padding = "12px";
    textarea.style.background = "transparent";
    textarea.style.color = "var(--text-primary, #e5e7eb)";
    textarea.style.fontFamily = "ui-monospace, SFMono-Regular, Menlo, monospace";
    textarea.style.fontSize = "13px";
    textarea.style.lineHeight = "1.5";
    textarea.style.resize = "none";

    container.replaceChildren(textarea);

    const id = nextId++;
    viewers.set(id, {
      container,
      textarea,
      onChange: null,
      changeHandler: null,
    });
    return id;
  };

  window.setTextViewerContent = function setTextViewerContent(handle, text) {
    const entry = viewers.get(handle);
    if (!entry) return;
    entry.textarea.value = text ?? "";
  };

  window.getTextViewerContent = function getTextViewerContent(handle) {
    const entry = viewers.get(handle);
    if (!entry) return "";
    return entry.textarea.value;
  };

  window.onTextViewerChange = function onTextViewerChange(handle, cb) {
    const entry = viewers.get(handle);
    if (!entry) return;

    if (entry.changeHandler) {
      entry.textarea.removeEventListener("input", entry.changeHandler);
    }

    entry.onChange = cb;
    entry.changeHandler = () => {
      if (entry.onChange) entry.onChange(entry.textarea.value);
    };
    entry.textarea.addEventListener("input", entry.changeHandler);
  };

  window.disposeTextViewer = function disposeTextViewer(handle) {
    const entry = viewers.get(handle);
    if (!entry) return;
    if (entry.changeHandler) {
      entry.textarea.removeEventListener("input", entry.changeHandler);
    }
    entry.container.replaceChildren();
    viewers.delete(handle);
  };
})();
