# Content Viewer Application Requirements Research
## Dioxus Web Desktop Environment

---

## Executive Summary

This document provides a comprehensive research guide for implementing content viewers in a Dioxus-based web desktop application. Each viewer type includes:

- **Functionality requirements** - Core features needed for a complete user experience
- **JS Interop needs** - How to bridge Rust/Dioxus with JavaScript libraries
- **Library options** - Recommended libraries and their trade-offs
- **Best practices** - Implementation considerations and patterns

---

## 1. Text Editor

### Functionality Requirements

#### Rich Text vs Plain Text Support
- **Rich Text**: Support for formatted text (bold, italic, underline, headers, lists, links)
  - HTML-based rich text: `<textarea>` with contenteditable or specialized editor
  - Markdown-based: Live preview with markdown rendering
- **Plain Text**: Simple text editing for code, logs, configuration files
  - Line-by-line text manipulation
  - No formatting preservation needed

#### Syntax Highlighting
- Support for 20+ programming languages (Rust, JavaScript, Python, etc.)
- Language auto-detection or manual selection
- Configurable color themes (dark/light mode)
- Line number synchronization with highlighting

#### Find/Replace Functionality
- Global search (Ctrl+F) across entire document
- Case-sensitive and regex search options
- Incremental search with highlighting of all matches
- Replace single occurrence or all occurrences
- Find next/previous shortcuts (F3, Shift+F3)

#### Line Numbers and Gutter
- Line number display on left margin
- Gutter area for breakpoint markers, fold indicators, line status
- Custom gutter width configuration
- Line number formatting (e.g., padding with zeros)

#### Auto-save and Dirty State
- **Dirty State**: Visual indicator when document has unsaved changes (asterisk, dot)
- Auto-save triggers:
  - On change with debouncing (default 30-60 seconds)
  - On focus loss
  - On keyboard shortcut (Ctrl+S)
- Conflict resolution for concurrent edits
- Auto-save status indicator

#### Undo/Redo
- Unlimited undo/redo stack
- Visual indicator when undo/redo available
- Keyboard shortcuts (Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z)
- Persistent history across editor sessions (optional)
- Batch operations for macro undo

### JS Interop Needs

#### CodeMirror 6 Integration (Recommended)
```rust
// Cargo.toml dependencies
// dioxus
// wasm-bindgen
// serde
```

```javascript
// JavaScript glue code (in public/js/editor.js)
import { EditorView, basicSetup } from "https://esm.sh/codemirror@6.0.1";
import { EditorState } from "https://esm.sh/@codemirror/state@6.0.1";
import { javascript } from "https://esm.sh/@codemirror/lang-javascript@6.0.1";

let editor = null;

window.createEditor = (elementId, initialContent, language) => {
  const parent = document.getElementById(elementId);
  
  editor = new EditorView({
    doc: initialContent,
    extensions: [
      basicSetup,
      javascript(),
      EditorView.theme({
        "&": { backgroundColor: "#1e1e1e", color: "#d4d4d4" },
        ".cm-content": { fontFamily: "monospace" }
      })
    ],
    parent: parent
  });

  return {
    getContent: () => editor.state.doc.toString(),
    setContent: (content) => {
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: content }
      });
    },
    getSelection: () => editor.state.selection.main.from,
    destroy: () => editor.destroy()
  };
};
```

```rust
// Rust bindings (use wasm-bindgen macro)
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window)]
    fn createEditor(elementId: &str, initialContent: &str, language: &str) -> JsValue;
}

// Component usage
#[component]
pub fn TextEditor(content: String, on_change: Callback<String>) -> Element {
    let editor_ref = use_coroutine_handle();
    let content_state = use_signal(|| content.clone());

    use_effect(move || {
        let editor = create_editor("editor-container", &content_state.read(), "javascript");
        editor_ref.set(Some(editor));
    });

    rsx! {
        div { id: "editor-container" }
    }
}
```

#### Monaco Editor Integration (Alternative)
- Larger bundle size (~2MB minified)
- More features (IntelliSense, multi-cursor, minimap)
- TypeScript definitions included
- Better for VS Code-like experience

```javascript
// Monaco loader
require.config({ paths: { 'vs': 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.44.0/min/vs' }});

require(['vs/editor/editor.main'], function() {
  monaco.editor.create(document.getElementById('container'), {
    value: initialContent,
    language: 'javascript',
    theme: 'vs-dark',
    automaticLayout: true
  });
});
```

### Library Options

| Library | Bundle Size | Features | Pros | Cons |
|--------|------------|---------|------|------|
| **CodeMirror 6** | ~150KB | Syntax highlighting, search, line numbers | Modular, extensible, well-documented | Smaller ecosystem than Monaco |
| **Monaco Editor** | ~2MB | Full VS Code editor experience | Rich feature set, industry standard | Large bundle, slower to load |
| **Ace Editor** | ~300KB | Syntax highlighting, themes | Lightweight compared to Monaco | Older API, less active development |
| **CodeMirror 5** | ~100KB | Legacy features | Mature, stable | Not actively maintained |

### Best Practices

1. **Lazy Loading**: Load editor JS bundle only when text viewer window opens
2. **Content Caching**: Store document in `IndexedDB` for persistence
3. **Worker Processing**: Offload syntax highlighting to Web Worker for large files
4. **Virtual Scrolling**: Implement for files >10,000 lines to maintain performance
5. **Memory Management**: Destroy editor instance when window closes

```rust
// Memory cleanup
use_effect_with_cleanup(
    move || editor_ref.clone(),
    move |editor_ref| {
        move || {
            if let Some(editor) = editor_ref.get() {
                // Call JS cleanup function
                destroy_editor(&editor);
            }
        }
    }
)
```

---

## 2. Image Viewer

### Functionality Requirements

#### Formats Supported
**Standard Raster Formats:**
- PNG (Portable Network Graphics) - Lossless, transparency
- JPEG/JPG (Joint Photographic Experts Group) - Lossy, photographs
- GIF (Graphics Interchange Format) - 8-bit indexed, animations
- WebP (Web Picture) - Modern format, better compression than PNG/JPEG
- AVIF (AV1 Image File Format) - State-of-the-art compression
- BMP (Bitmap) - Windows format, uncompressed (avoid for web)

**Vector Format:**
- SVG (Scalable Vector Graphics) - Vector, infinite scaling
- SVGZ (Compressed SVG) - Gzipped SVG

**Browser Support Summary:**
- Universal support: PNG, JPEG, GIF, SVG, BMP
- Modern browsers (2020+): WebP, AVIF
- Safari 14+: WebP support requires macOS Big Sur+

#### Zoom, Pan, Rotate, Flip
**Zoom Controls:**
- Zoom in/out buttons (+/-)
- Zoom levels: 25%, 50%, 75%, 100%, 125%, 150%, 200%, 400%
- Fit to width
- Fit to page
- Actual size (100%)
- Mouse wheel zoom with modifier (Ctrl+scroll)
- Touch pinch-to-zoom

**Pan Controls:**
- Drag to pan (mouse or touch)
- Arrow key navigation
- Scroll wheel panning (without zoom modifier)
- Double-click to recenter

**Rotate Controls:**
- Rotate 90° clockwise/counter-clockwise buttons
- Rotate 180° button
- Custom angle rotation (0-360° slider)
- Keyboard shortcuts (R, Shift+R)

**Flip Controls:**
- Flip horizontal (mirror)
- Flip vertical (upside down)

#### Slideshow Mode
- **Auto-play**: Advance images every N seconds (configurable)
- **Manual navigation**: Previous/next buttons
- **Keyboard shortcuts**: Left/Right arrows, Space (play/pause)
- **Transition effects**: Fade, slide, wipe (optional)
- **Progress indicator**: Dots, thumbnails, or progress bar
- **Loop option**: Restart from beginning when last image reached

#### Image Metadata Display
- **EXIF data**: Camera model, ISO, aperture, shutter speed
- **Dimensions**: Width × height in pixels
- **File size**: Human-readable format (KB, MB)
- **File type**: MIME type and file extension
- **Color space**: sRGB, Adobe RGB, etc.
- **Bit depth**: 8-bit, 16-bit, etc.
- **Creation date**: Date/time stamp from EXIF
- **GPS location**: Latitude/longitude if available

#### Image Editing Basics
**Crop:**
- Aspect ratio presets (1:1, 4:3, 16:9, free)
- Drag corner handles to resize crop area
- Real-time preview of cropped region
- Apply/Cancel buttons

**Brightness:**
- Slider: -100 (dark) to +100 (bright)
- Real-time preview
- Reset button to original

**Contrast:**
- Slider: -100 to +100
- Real-time preview
- Reset button

**Additional filters (optional):**
- Saturation adjustment
- Hue rotation
- Blur/Sharpen
- Grayscale conversion
- Sepia tone

### JS Interop Needs

#### Custom Pan/Zoom/Rotate Implementation

```javascript
// public/js/imageViewer.js
class ImageViewer {
  constructor(containerId, imageUrl) {
    this.container = document.getElementById(containerId);
    this.imageUrl = imageUrl;
    this.state = {
      zoom: 1,
      pan: { x: 0, y: 0 },
      rotation: 0,
      flipH: false,
      flipV: false
    };
    this.init();
  }

  async init() {
    this.image = new Image();
    this.image.src = this.imageUrl;
    await new Promise(resolve => this.image.onload = resolve);
    
    this.canvas = document.createElement('canvas');
    this.ctx = this.canvas.getContext('2d');
    this.container.appendChild(this.canvas);
    this.render();
    this.setupEvents();
  }

  render() {
    const { width, height } = this.image;
    const { zoom, pan, rotation, flipH, flipV } = this.state;
    
    this.canvas.width = width * zoom;
    this.canvas.height = height * zoom;
    
    this.ctx.save();
    this.ctx.translate(this.canvas.width / 2, this.canvas.height / 2);
    this.ctx.rotate(rotation * Math.PI / 180);
    this.ctx.scale(flipH ? -1 : 1, flipV ? -1 : 1);
    this.ctx.scale(zoom, zoom);
    this.ctx.translate(-width / 2 + pan.x, -height / 2 + pan.y);
    this.ctx.drawImage(this.image, 0, 0);
    this.ctx.restore();
  }

  setupEvents() {
    let isDragging = false;
    let lastPos = { x: 0, y: 0 };

    this.canvas.addEventListener('mousedown', (e) => {
      isDragging = true;
      lastPos = { x: e.clientX, y: e.clientY };
    });

    window.addEventListener('mousemove', (e) => {
      if (!isDragging) return;
      const dx = (e.clientX - lastPos.x) / this.state.zoom;
      const dy = (e.clientY - lastPos.y) / this.state.zoom;
      this.state.pan.x += dx;
      this.state.pan.y += dy;
      lastPos = { x: e.clientX, y: e.clientY };
      this.render();
    });

    window.addEventListener('mouseup', () => {
      isDragging = false;
    });

    this.canvas.addEventListener('wheel', (e) => {
      e.preventDefault();
      const delta = e.deltaY > 0 ? 0.9 : 1.1;
      this.state.zoom *= delta;
      this.state.zoom = Math.max(0.1, Math.min(10, this.state.zoom));
      this.render();
    });
  }

  // API methods
  setZoom(zoom) {
    this.state.zoom = zoom;
    this.render();
  }

  rotate(degrees) {
    this.state.rotation = (this.state.rotation + degrees) % 360;
    this.render();
  }

  flipHorizontal() {
    this.state.flipH = !this.state.flipH;
    this.render();
  }

  getMetadata() {
    // Use exif-js or similar library
    return {
      width: this.image.width,
      height: this.image.height,
      // ... additional metadata
    };
  }
}

window.createImageViewer = (containerId, imageUrl) => {
  return new ImageViewer(containerId, imageUrl);
};
```

```rust
// Rust bindings
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    type ImageViewer;
    #[wasm_bindgen(constructor)]
    fn createImageViewer(containerId: &str, imageUrl: &str) -> ImageViewer;
    
    #[wasm_bindgen(method)]
    fn setZoom(this: &ImageViewer, zoom: f64);
    
    #[wasm_bindgen(method)]
    fn rotate(this: &ImageViewer, degrees: i32);
    
    #[wasm_bindgen(method)]
    fn flipHorizontal(this: &ImageViewer);
    
    #[wasm_bindgen(method)]
    fn getMetadata(this: &ImageViewer) -> JsValue;
}
```

#### Using Panzoom Library (Alternative)

```javascript
import Panzoom from 'panzoom';

const element = document.getElementById('image-container');
const panzoom = Panzoom(element, {
  maxScale: 10,
  minScale: 0.1,
  contain: 'outside'
});

// Enable controls
panzoom.zoomWithButtons();
panzoom.panWithButtons();
```

### Library Options

| Library | Bundle Size | Features | Pros | Cons |
|--------|------------|---------|------|------|
| **Custom Canvas** | ~5KB | Full control | Lightweight, customizable | More development effort |
| **Panzoom** | ~15KB | Pan/zoom with touch | Simple API, touch support | Limited rotation/flip |
| **OpenSeadragon** | ~100KB | Deep zoom, rotation | Powerful, image pyramid support | Heavy for simple use case |
| **Leaflet** | ~150KB | Map-like pan/zoom | Well-tested, plugins | Overkill for single image |
| **Cropper.js** | ~30KB | Crop functionality | Easy cropping integration | No pan/zoom built-in |

### Best Practices

1. **Lazy Loading**: Load high-resolution images on demand
2. **Image Pyramid**: Generate multiple resolutions for large images
3. **Memory Management**: Clear canvas when viewer closes
4. **Progressive Loading**: Use progressive JPEGs for large images
5. **Offscreen Canvas**: Render transformations in worker for performance

```rust
// Progressive image loading
use_effect(move || {
    async fn load_image_progressive(url: String) -> Result<ImageData, JsError> {
        // Check for progressive JPEG
        // Load in chunks
        // Render each pass
    }
});
```

---

## 3. PDF Viewer

### Functionality Requirements

#### Rendering Approach

**PDF.js (Recommended)**
- Mozilla's JavaScript PDF rendering library
- Renders PDF pages to HTML5 Canvas
- Pure JavaScript, no native dependencies
- Supports password-protected PDFs
- Layer support for text selection
- Annotation support (highlights, notes)

**Pros:**
- Cross-browser compatibility
- Active development and community
- Feature-rich (search, annotations, forms)
- No plugin required

**Cons:**
- Performance overhead on large PDFs
- Memory intensive for high-resolution pages
- Initial parse time

**Native Browser PDF** (Alternative)
- Use browser's built-in PDF viewer via `<iframe>` or `<embed>`
- Pros: Native performance, no JS overhead
- Cons: Inconsistent across browsers, limited customization

```html
<!-- Native PDF viewer -->
<iframe src="document.pdf" width="100%" height="600px"></iframe>

<!-- PDF.js viewer -->
<iframe src="/web/viewer.html?file=document.pdf" width="100%" height="600px"></iframe>
```

#### Page Navigation
- **Page numbers**: Display current page and total pages (e.g., "3 / 15")
- **Next/Previous buttons**: Advance one page forward/backward
- **Go to page**: Input field to jump to specific page
- **Keyboard shortcuts**: Page Up/Down, Arrow keys
- **Thumbnail view**: Grid of page thumbnails for quick navigation
- **Bookmarks**: Save and jump to marked pages (optional)

#### Zoom Levels
- **Preset zooms**: 25%, 50%, 75%, 100%, 125%, 150%, 200%, 400%
- **Fit to width**: Scale to fit page width in viewport
- **Fit to page**: Scale to fit entire page in viewport
- **Actual size**: Display at 100% scale
- **Zoom in/out**: Buttons and keyboard shortcuts (+/-)
- **Mouse wheel**: Zoom with Ctrl+wheel

#### Text Selection and Copy
- **Text selection**: Click and drag to select text
- **Selection highlight**: Blue background for selected text
- **Copy to clipboard**: Ctrl+C, Right-click → Copy, Copy button
- **Select all**: Ctrl+A, Select All button
- **Text layer**: Separate text layer for selection (not just image)
- **Font embedding**: Support for embedded fonts in PDF

#### Annotations Support
- **Highlight**: Yellow highlighter tool for text
- **Underline**: Underline selected text
- **Strike-through**: Cross out selected text
- **Sticky notes**: Add pop-up notes at specific positions
- **Draw**: Freehand drawing tool
- **Shapes**: Add rectangles, circles, arrows
- **Save annotations**: Export annotations with document (optional)
- **Import annotations**: Load saved annotations (optional)

#### Download/Print Options
- **Download PDF**: Button to download original PDF file
- **Print PDF**: Button to print document (opens print dialog)
- **Save as images**: Export pages as PNG/JPEG (optional)
- **Email PDF**: Open email client with PDF attachment (optional)

### JS Interop Needs

#### PDF.js Integration

```javascript
// public/js/pdfViewer.js
import * as pdfjsLib from 'https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.min.mjs';

pdfjsLib.GlobalWorkerOptions.workerSrc = 
  'https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.worker.min.mjs';

class PDFViewer {
  constructor(containerId, pdfUrl) {
    this.container = document.getElementById(containerId);
    this.pdfUrl = pdfUrl;
    this.pdfDoc = null;
    this.currentPage = 1;
    this.scale = 1.0;
    this.pages = [];
    this.rendering = false;
    this.init();
  }

  async init() {
    const loadingTask = pdfjsLib.getDocument(this.pdfUrl);
    this.pdfDoc = await loadingTask.promise;
    this.renderPage(this.currentPage);
    this.setupNavigation();
  }

  async renderPage(pageNum) {
    if (this.rendering) return;
    this.rendering = true;

    const page = await this.pdfDoc.getPage(pageNum);
    const viewport = page.getViewport({ scale: this.scale });
    
    // Clear previous canvas
    this.container.innerHTML = '';
    
    const canvas = document.createElement('canvas');
    const context = canvas.getContext('2d');
    canvas.height = viewport.height;
    canvas.width = viewport.width;
    this.container.appendChild(canvas);
    
    const renderContext = {
      canvasContext: context,
      viewport: viewport
    };
    
    await page.render(renderContext).promise;
    this.rendering = false;
  }

  setupNavigation() {
    // Add navigation buttons
    const prevBtn = document.createElement('button');
    prevBtn.textContent = 'Previous';
    prevBtn.onclick = () => {
      if (this.currentPage > 1) {
        this.currentPage--;
        this.renderPage(this.currentPage);
      }
    };
    
    const nextBtn = document.createElement('button');
    nextBtn.textContent = 'Next';
    nextBtn.onclick = () => {
      if (this.currentPage < this.pdfDoc.numPages) {
        this.currentPage++;
        this.renderPage(this.currentPage);
      }
    };
    
    this.container.prepend(prevBtn, nextBtn);
    
    // Keyboard navigation
    window.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowLeft' && this.currentPage > 1) {
        this.currentPage--;
        this.renderPage(this.currentPage);
      } else if (e.key === 'ArrowRight' && this.currentPage < this.pdfDoc.numPages) {
        this.currentPage++;
        this.renderPage(this.currentPage);
      }
    });
  }

  // API methods
  nextPage() {
    if (this.currentPage < this.pdfDoc.numPages) {
      this.currentPage++;
      this.renderPage(this.currentPage);
    }
  }

  previousPage() {
    if (this.currentPage > 1) {
      this.currentPage--;
      this.renderPage(this.currentPage);
    }
  }

  goToPage(pageNum) {
    if (pageNum >= 1 && pageNum <= this.pdfDoc.numPages) {
      this.currentPage = pageNum;
      this.renderPage(this.currentPage);
    }
  }

  setZoom(scale) {
    this.scale = scale;
    this.renderPage(this.currentPage);
  }

  download() {
    window.open(this.pdfUrl, '_blank');
  }
}

window.createPDFViewer = (containerId, pdfUrl) => {
  return new PDFViewer(containerId, pdfUrl);
};
```

```rust
// Rust bindings
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    type PDFViewer;
    #[wasm_bindgen(constructor)]
    fn createPDFViewer(containerId: &str, pdfUrl: &str) -> PDFViewer;
    
    #[wasm_bindgen(method)]
    fn nextPage(this: &PDFViewer);
    
    #[wasm_bindgen(method)]
    fn previousPage(this: &PDFViewer);
    
    #[wasm_bindgen(method)]
    fn goToPage(this: &PDFViewer, pageNum: u32);
    
    #[wasm_bindgen(method)]
    fn setZoom(this: &PDFViewer, scale: f64);
    
    #[wasm_bindgen(method)]
    fn download(this: &PDFViewer);
}

// Dioxus component
#[component]
pub fn PDFViewer(url: String) -> Element {
    let viewer_ref = use_coroutine_handle();

    use_effect(move || {
        let viewer = create_pdf_viewer("pdf-container", &url);
        viewer_ref.set(Some(viewer));
    });

    rsx! {
        div {
            id: "pdf-container",
            style: "width: 100%; height: 600px; overflow: auto;",
        }
    }
}
```

### Library Options

| Library | Bundle Size | Features | Pros | Cons |
|--------|------------|---------|------|------|
| **PDF.js** | ~500KB | Full rendering, annotations | Cross-browser, feature-rich | Performance overhead |
| **PDFObject** | ~150KB | Simpler API | Lightweight | Less features, deprecated |
| **Native Browser** | 0KB | Native performance | No customization overhead | Inconsistent, no API access |
| **pdf-lib** | ~300KB | PDF generation/editing | Good for creating PDFs | Read-only in some contexts |

### Best Practices

1. **Lazy Loading**: Load PDF.js worker only when PDF viewer opens
2. **Page Caching**: Render adjacent pages ahead for smoother navigation
3. **Web Workers**: Parse PDF in worker to prevent UI blocking
4. **Memory Management**: Unload pages from memory when not visible
5. **Progress Indicator**: Show loading progress for large PDFs

```rust
// Web worker for PDF parsing
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn parse_pdf(url: String) -> Result<PDFInfo, JsError> {
    // Send to worker for parsing
    // Return basic info (page count, metadata)
}
```

---

## 4. Audio Player

### Functionality Requirements

#### Formats Supported
**Universal Support (all modern browsers):**
- MP3 (MPEG-1 Audio Layer 3) - Most common, lossy compression
- WAV (Waveform Audio File Format) - Uncompressed, high quality
- OGG (Ogg Vorbis) - Open-source, lossy compression
- AAC (Advanced Audio Coding) - Better quality than MP3 at similar bitrate

**Modern Browser Support (2020+):**
- FLAC (Free Lossless Audio Codec) - Lossless compression
- Opus - Modern codec, better than OGG
- M4A - MPEG-4 audio container

**Browser Support Summary:**
- Chrome/Firefox/Edge: MP3, WAV, OGG, AAC, FLAC, Opus
- Safari: MP3, WAV, AAC, ALAC (Apple Lossless), FLAC (14+)
- Mobile: Similar support to desktop browsers

#### Play/Pause, Stop, Next/Prev
**Basic Controls:**
- Play button: Start or resume playback
- Pause button: Pause playback (maintain position)
- Stop button: Stop playback and reset to beginning
- Next/Prev: Navigate in playlist

**Keyboard Shortcuts:**
- Space: Play/Pause
- S: Stop
- Right Arrow: Next track
- Left Arrow: Previous track
- Ctrl+Right Arrow: Fast forward
- Ctrl+Left Arrow: Rewind

#### Volume Control
- **Volume slider**: 0 to 100% (0-1.0 range)
- **Mute button**: Toggle mute/unmute
- **Volume presets**: Low (25%), Medium (50%), High (100%)
- **Keyboard shortcuts**: M (mute), Up/Down arrows (volume)
- **Visual indicator**: Volume level bar or speaker icon

#### Progress Bar and Seeking
- **Progress bar**: Visual representation of playback position
- **Current time**: Display current playback time (MM:SS)
- **Duration**: Display total duration (MM:SS)
- **Seeking**: Click/drag on progress bar to jump to position
- **Buffered**: Visual indicator of buffered content
- **Live updates**: Update every second or 100ms

#### Playlist Support
- **Playlist view**: List of tracks with metadata (title, artist, duration)
- **Track selection**: Click to play specific track
- **Queue management**: Add/remove tracks
- **Shuffle**: Randomize playback order
- **Repeat**: Loop playlist, loop single track, no repeat
- **Save/Load**: Persist playlist to IndexedDB

#### Audio Visualization
- **Waveform**: Display audio waveform visualization
- **Frequency spectrum**: Show frequency bars (bass, mid, treble)
- **Real-time**: Update visualization as audio plays
- **Canvas-based**: Use HTML5 Canvas for rendering

### JS Interop Needs

#### HTML5 Audio API

```rust
// Dioxus component using native audio element
#[component]
pub fn AudioPlayer(url: String) -> Element {
    rsx! {
        audio {
            src: "{url}",
            controls: true,
            autoplay: false,
        }
    }
}
```

#### Advanced Audio Player with Visualization

```javascript
// public/js/audioPlayer.js
class AudioPlayer {
  constructor(containerId, audioUrl) {
    this.container = document.getElementById(containerId);
    this.audio = new Audio(audioUrl);
    this.setupAudio();
    this.setupVisualization();
    this.setupControls();
  }

  setupAudio() {
    this.audioContext = new (window.AudioContext || window.webkitAudioContext)();
    this.source = this.audioContext.createMediaElementSource(this.audio);
    this.analyser = this.audioContext.createAnalyser();
    this.analyser.fftSize = 256;
    
    this.source.connect(this.analyser);
    this.analyser.connect(this.audioContext.destination);
  }

  setupVisualization() {
    this.canvas = document.createElement('canvas');
    this.canvas.width = 300;
    this.canvas.height = 50;
    this.ctx = this.canvas.getContext('2d');
    this.container.appendChild(this.canvas);
    
    this.bufferLength = this.analyser.frequencyBinCount;
    this.dataArray = new Uint8Array(this.bufferLength);
    
    const draw = () => {
      requestAnimationFrame(draw);
      this.analyser.getByteFrequencyData(this.dataArray);
      
      this.ctx.fillStyle = 'rgb(200, 200, 200)';
      this.ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
      
      const barWidth = (this.canvas.width / this.bufferLength) * 2.5;
      let barHeight;
      let x = 0;
      
      for (let i = 0; i < this.bufferLength; i++) {
        barHeight = this.dataArray[i];
        this.ctx.fillStyle = `rgb(${barHeight + 100}, 50, 50)`;
        this.ctx.fillRect(x, this.canvas.height - barHeight, barWidth, barHeight);
        x += barWidth + 1;
      }
    };
    
    draw();
  }

  setupControls() {
    this.audio.addEventListener('timeupdate', () => {
      // Update progress bar
      this.updateProgress();
    });

    this.audio.addEventListener('ended', () => {
      // Play next track or stop
      this.onTrackEnded();
    });
  }

  updateProgress() {
    const progress = (this.audio.currentTime / this.audio.duration) * 100;
    // Update UI progress element
  }

  // API methods
  play() {
    this.audioContext.resume();
    this.audio.play();
  }

  pause() {
    this.audio.pause();
  }

  stop() {
    this.audio.pause();
    this.audio.currentTime = 0;
  }

  setVolume(volume) {
    this.audio.volume = volume; // 0.0 to 1.0
  }

  seekTo(time) {
    this.audio.currentTime = time;
  }
}

window.createAudioPlayer = (containerId, audioUrl) => {
  return new AudioPlayer(containerId, audioUrl);
};
```

```rust
// Rust bindings
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    type AudioPlayer;
    #[wasm_bindgen(constructor)]
    fn createAudioPlayer(containerId: &str, audioUrl: &str) -> AudioPlayer;
    
    #[wasm_bindgen(method)]
    fn play(this: &AudioPlayer);
    
    #[wasm_bindgen(method)]
    fn pause(this: &AudioPlayer);
    
    #[wasm_bindgen(method)]
    fn stop(this: &AudioPlayer);
    
    #[wasm_bindgen(method)]
    fn setVolume(this: &AudioPlayer, volume: f64);
    
    #[wasm_bindgen(method)]
    fn seekTo(this: &AudioPlayer, time: f64);
}
```

#### Web Speech API for Audio Description (Optional)

```javascript
// Add voice description for accessibility
const utterance = new SpeechSynthesisUtterance(
  "Now playing: Song Title by Artist"
);
window.speechSynthesis.speak(utterance);
```

### Library Options

| Library | Bundle Size | Features | Pros | Cons |
|--------|------------|---------|------|------|
| **HTML5 Audio** | 0KB | Basic controls | Native, no dependencies | Limited customization |
| **Howler.js** | ~200KB | Audio engine | Game-focused, Web Audio API wrapper | Overkill for simple player |
| **Pizzicato** | ~50KB | Simple audio | Lightweight, promise-based | Less features |
| **Custom Web Audio** | ~5KB | Full control | Customizable | More development effort |

### Best Practices

1. **Lazy Loading**: Load audio files on demand, not all at once
2. **Preloading**: Prebuffer next track in playlist
3. **Memory Management**: Stop audio and close AudioContext when done
4. **Error Handling**: Graceful fallback when format not supported
5. **Accessibility**: Provide keyboard controls and ARIA labels

```rust
// Check format support
#[wasm_bindgen]
pub fn can_play_format(mime_type: &str) -> bool {
    let audio = HtmlAudioElement::new();
    audio.can_play_type(mime_type) != ""
}
```

---

## 5. Video Player

### Functionality Requirements

#### Formats Supported
**Universal Support (all modern browsers):**
- MP4 (MPEG-4 Part 14) with H.264 video codec
- WebM with VP8/VP9 video codec
- OGG with Theora video codec

**Modern Browser Support (2020+):**
- MP4 with H.265/HEVC codec (limited support)
- WebM with AV1 codec (Chrome/Firefox)
- HLS (HTTP Live Streaming) m3u8 playlists

**Browser Support Summary:**
- Chrome/Firefox/Edge: MP4 (H.264), WebM (VP8/VP9/AV1)
- Safari: MP4 (H.264), HLS (m3u8), WebM (VP8, no AV1)
- Mobile: Similar support, may require HLS for streaming

**Audio Codecs:**
- AAC (Advanced Audio Coding) - Universal
- MP3 - Universal
- Opus - Chrome/Firefox/Edge
- Vorbis (in OGG) - Chrome/Firefox/Edge

#### Controls
**Basic Controls:**
- Play/Pause button: Start or pause video
- Stop button: Stop playback and reset to beginning
- Volume slider: Adjust audio volume
- Mute button: Toggle mute/unmute
- Fullscreen button: Toggle fullscreen mode
- Progress bar: Seek within video

**Keyboard Shortcuts:**
- Space: Play/Pause
- M: Mute
- F: Fullscreen
- Left/Right Arrow: Rewind/Fast forward
- Up/Down Arrow: Volume

#### Subtitles Support
- **Format support**: WebVTT (Web Video Text Tracks)
- **Built-in subtitles**: Display embedded subtitles
- **External subtitles**: Load .vtt or .srt files
- **Language selection**: Choose from multiple subtitle tracks
- **Subtitle appearance**: Font, size, color, background
- **Enable/disable**: Toggle subtitles on/off

```html
<video controls>
  <source src="video.mp4" type="video/mp4">
  <track src="subtitles-en.vtt" kind="subtitles" srclang="en" label="English">
  <track src="subtitles-es.vtt" kind="subtitles" srclang="es" label="Español">
</video>
```

#### Playback Speed
- **Speed options**: 0.25x, 0.5x, 0.75x, 1x (normal), 1.25x, 1.5x, 2x
- **Default**: 1x
- **Preserve pitch**: Maintain audio pitch at different speeds
- **Keyboard shortcut**: >/< to increase/decrease speed

#### Picture-in-Picture (PiP)
- **Enter PiP**: Button to open floating video window
- **PiP window**: Small, draggable window with video
- **Close PiP**: Button or keyboard to exit PiP
- **Controls in PiP**: Play/pause, volume, close
- **Resize**: Drag edges to resize PiP window
- **Multi-window**: Support multiple PiP windows (browser-dependent)

### JS Interop Needs

#### HTML5 Video API

```rust
// Dioxus component using native video element
#[component]
pub fn VideoPlayer(url: String) -> Element {
    rsx! {
        video {
            src: "{url}",
            controls: true,
            autoplay: false,
            width: "100%",
        }
    }
}
```

#### Advanced Video Player

```javascript
// public/js/videoPlayer.js
class VideoPlayer {
  constructor(containerId, videoUrl) {
    this.container = document.getElementById(containerId);
    this.video = document.createElement('video');
    this.video.src = videoUrl;
    this.video.controls = false; // Custom controls
    this.container.appendChild(this.video);
    this.setupControls();
    this.setupKeyboardShortcuts();
  }

  setupControls() {
    // Create custom control bar
    const controls = document.createElement('div');
    controls.className = 'video-controls';
    
    // Play/Pause button
    const playPauseBtn = document.createElement('button');
    playPauseBtn.textContent = '▶';
    playPauseBtn.onclick = () => this.togglePlay();
    controls.appendChild(playPauseBtn);
    
    // Progress bar
    const progressContainer = document.createElement('div');
    progressContainer.className = 'progress-container';
    
    const progressBar = document.createElement('div');
    progressBar.className = 'progress-bar';
    
    const progressFill = document.createElement('div');
    progressFill.className = 'progress-fill';
    progressBar.appendChild(progressFill);
    
    progressContainer.appendChild(progressBar);
    controls.appendChild(progressContainer);
    
    // Time display
    const timeDisplay = document.createElement('span');
    timeDisplay.className = 'time-display';
    timeDisplay.textContent = '0:00 / 0:00';
    controls.appendChild(timeDisplay);
    
    // Volume slider
    const volumeContainer = document.createElement('div');
    volumeContainer.className = 'volume-container';
    
    const volumeSlider = document.createElement('input');
    volumeSlider.type = 'range';
    volumeSlider.min = '0';
    volumeSlider.max = '1';
    volumeSlider.step = '0.1';
    volumeSlider.value = '1';
    volumeSlider.oninput = (e) => {
      this.video.volume = parseFloat(e.target.value);
    };
    
    volumeContainer.appendChild(volumeSlider);
    controls.appendChild(volumeContainer);
    
    // Fullscreen button
    const fullscreenBtn = document.createElement('button');
    fullscreenBtn.textContent = '⛶';
    fullscreenBtn.onclick = () => this.toggleFullscreen();
    controls.appendChild(fullscreenBtn);
    
    // PiP button
    if (document.pictureInPictureEnabled) {
      const pipBtn = document.createElement('button');
      pipBtn.textContent = '🔲';
      pipBtn.onclick = () => this.togglePiP();
      controls.appendChild(pipBtn);
    }
    
    // Speed control
    const speedSelect = document.createElement('select');
    ['0.25', '0.5', '0.75', '1', '1.25', '1.5', '2'].forEach(speed => {
      const option = document.createElement('option');
      option.value = speed;
      option.textContent = speed + 'x';
      if (speed === '1') option.selected = true;
      speedSelect.appendChild(option);
    });
    speedSelect.onchange = (e) => {
      this.video.playbackRate = parseFloat(e.target.value);
    };
    controls.appendChild(speedSelect);
    
    // Subtitle track selector
    const textTracks = this.video.textTracks;
    if (textTracks.length > 0) {
      const subtitleSelect = document.createElement('select');
      const offOption = document.createElement('option');
      offOption.value = '-1';
      offOption.textContent = 'Off';
      subtitleSelect.appendChild(offOption);
      
      for (let i = 0; i < textTracks.length; i++) {
        const track = textTracks[i];
        const option = document.createElement('option');
        option.value = i.toString();
        option.textContent = track.label || `Track ${i + 1}`;
        subtitleSelect.appendChild(option);
      }
      
      subtitleSelect.onchange = (e) => {
        const trackIndex = parseInt(e.target.value);
        for (let i = 0; i < textTracks.length; i++) {
          textTracks[i].mode = i === trackIndex ? 'showing' : 'hidden';
        }
      };
      controls.appendChild(subtitleSelect);
    }
    
    this.container.appendChild(controls);
    
    // Update progress bar
    this.video.addEventListener('timeupdate', () => {
      const progress = (this.video.currentTime / this.video.duration) * 100;
      progressFill.style.width = progress + '%';
      timeDisplay.textContent = this.formatTime(this.video.currentTime) + 
        ' / ' + this.formatTime(this.video.duration);
    });
    
    // Handle progress bar clicks
    progressBar.addEventListener('click', (e) => {
      const rect = progressBar.getBoundingClientRect();
      const percent = (e.clientX - rect.left) / rect.width;
      this.video.currentTime = percent * this.video.duration;
    });
  }

  togglePlay() {
    if (this.video.paused) {
      this.video.play();
    } else {
      this.video.pause();
    }
  }

  toggleFullscreen() {
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      this.container.requestFullscreen();
    }
  }

  async togglePiP() {
    if (document.pictureInPictureElement) {
      await document.exitPictureInPicture();
    } else {
      await this.video.requestPictureInPicture();
    }
  }

  setupKeyboardShortcuts() {
    window.addEventListener('keydown', (e) => {
      switch (e.code) {
        case 'Space':
          e.preventDefault();
          this.togglePlay();
          break;
        case 'KeyM':
          this.video.muted = !this.video.muted;
          break;
        case 'KeyF':
          this.toggleFullscreen();
          break;
        case 'ArrowLeft':
          this.video.currentTime -= 5;
          break;
        case 'ArrowRight':
          this.video.currentTime += 5;
          break;
        case 'ArrowUp':
          this.video.volume = Math.min(1, this.video.volume + 0.1);
          break;
        case 'ArrowDown':
          this.video.volume = Math.max(0, this.video.volume - 0.1);
          break;
      }
    });
  }

  formatTime(seconds) {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  }
}

window.createVideoPlayer = (containerId, videoUrl) => {
  return new VideoPlayer(containerId, videoUrl);
};
```

```rust
// Rust bindings
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    type VideoPlayer;
    #[wasm_bindgen(constructor)]
    fn createVideoPlayer(containerId: &str, videoUrl: &str) -> VideoPlayer;
    
    #[wasm_bindgen(method)]
    fn togglePlay(this: &VideoPlayer);
    
    #[wasm_bindgen(method)]
    fn toggleFullscreen(this: &VideoPlayer);
    
    #[wasm_bindgen(method)]
    fn togglePiP(this: &VideoPlayer) -> JsValue; // Returns Promise
}
```

### Library Options

| Library | Bundle Size | Features | Pros | Cons |
|--------|------------|---------|------|------|
| **HTML5 Video** | 0KB | Basic controls | Native, no dependencies | Limited customization |
| **Video.js** | ~200KB | Plugins, HLS, DASH | Feature-rich, extensible | Heavier than native |
| **Plyr** | ~50KB | Lightweight, modern | Simple API | Less plugin ecosystem |
| **DPlayer** | ~150KB | Customizable | Good documentation | Less active maintenance |

### Best Practices

1. **Adaptive Streaming**: Use HLS/DASH for large videos
2. **Lazy Loading**: Load video only when player is visible
3. **Prebuffering**: Buffer ahead of playback position
4. **Memory Management**: Release video resources when player closes
5. **Accessibility**: Provide ARIA labels, keyboard controls

```rust
// Check format support
#[wasm_bindgen]
pub fn can_play_video(mime_type: &str) -> bool {
    let video = HtmlVideoElement::new();
    video.can_play_type(mime_type) != ""
}
```

---

## 6. Embeds (YouTube/General)

### Functionality Requirements

#### YouTube API Integration
**Basic Embed:**
- **IFrame embed**: Standard YouTube iframe embed code
- **Autoplay**: Start video automatically (with user interaction)
- **Controls**: Show/hide player controls
- **Mute**: Start muted for autoplay

**Advanced API (YouTube IFrame API):**
- **Player control**: Play, pause, stop programmatically
- **Seek**: Jump to specific time
- **Volume**: Adjust volume
- **Quality**: Change playback quality
- **Playback rate**: Speed up/slow down
- **Playlist**: Load and play playlists
- **Events**: Listen to player state changes

**Player State Events:**
- `unstarted` (-1): Player hasn't started
- `ended` (0): Video finished
- `playing` (1): Video is playing
- `paused` (2): Video is paused
- `buffering` (3): Video is buffering
- `cued` (5): Video is cued and ready

#### General Iframe Security
**Sandbox Attributes:**
- **`allow-same-origin`**: Allow same-origin access (needed for scripts)
- **`allow-scripts`**: Allow JavaScript execution in iframe
- **`allow-popups`**: Allow opening new windows
- **`allow-forms`**: Allow form submission
- **`allow-modals`**: Allow alert(), confirm(), prompt()
- **`allow-top-navigation`**: Allow iframe to navigate parent page
- **Default sandbox**: No sandbox (full access)

**Security Best Practices:**
- Always use `sandbox` with minimal permissions
- Use `allow-same-origin` + `allow-scripts` only if needed
- Avoid `allow-top-navigation` for untrusted content
- Use `allow-popups-to-escape-sandbox` for ads (if necessary)
- Consider `credentialless` for third-party iframes

#### Responsive Embed Sizing
- **Aspect ratio**: Maintain 16:9 or 4:3 ratio
- **Container-based**: Use responsive container
- **Percentage widths**: Width: 100%, height: auto
- **Padding trick**: Use padding-bottom to maintain aspect ratio
- **CSS aspect-ratio**: Modern property (limited support)

```css
.responsive-embed {
  position: relative;
  padding-bottom: 56.25%; /* 16:9 aspect ratio */
  height: 0;
  overflow: hidden;
}

.responsive-embed iframe {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
}
```

#### Communication Between Iframe and Desktop
**postMessage API:**
- Parent → iframe: Send commands to iframe
- iframe → parent: Send events to parent
- **Message format**: JSON object with type and data
- **Origin validation**: Validate message origin
- **Error handling**: Graceful fallback on invalid messages

**Window.addEventListener('message'):**
```javascript
// In parent (Dioxus app)
window.addEventListener('message', (event) => {
  if (event.origin !== 'https://trusted-domain.com') {
    return; // Reject untrusted messages
  }
  
  const { type, data } = event.data;
  switch (type) {
    case 'video-ended':
      handleVideoEnded(data);
      break;
    case 'player-state-changed':
      updatePlayerState(data);
      break;
    case 'error':
      handleError(data);
      break;
  }
});

// Send message to iframe
iframe.contentWindow.postMessage({
  type: 'play-video',
  data: { videoId: 'abc123' }
}, 'https://youtube.com');
```

### JS Interop Needs

#### YouTube IFrame API

```javascript
// public/js/youtubePlayer.js
let player = null;

window.onYouTubeIframeAPIReady = () => {
  player = new YT.Player('youtube-player', {
    height: '100%',
    width: '100%',
    videoId: '', // Set via API
    playerVars: {
      'playsinline': 1,
      'autoplay': 0,
      'controls': 1,
      'rel': 0, // Don't show related videos
      'modestbranding': 1
    },
    events: {
      'onReady': onPlayerReady,
      'onStateChange': onPlayerStateChange,
      'onError': onPlayerError
    }
  });
};

function onPlayerReady(event) {
  // Player is ready to receive API calls
}

function onPlayerStateChange(event) {
  // Send state change to parent
  const state = event.data;
  window.parent.postMessage({
    type: 'youtube-state-change',
    data: { state }
  }, '*');
}

function onPlayerError(event) {
  const errorCode = event.data;
  window.parent.postMessage({
    type: 'youtube-error',
    data: { errorCode }
  }, '*');
}

// API methods
window.loadYouTubeVideo = (videoId, startTime = 0) => {
  if (player) {
    player.loadVideoById(videoId, startTime);
  }
};

window.playYouTubeVideo = () => {
  if (player) {
    player.playVideo();
  }
};

window.pauseYouTubeVideo = () => {
  if (player) {
    player.pauseVideo();
  }
};

window.seekYouTubeTo = (seconds) => {
  if (player) {
    player.seekTo(seconds, true);
  }
};

window.setYouTubeVolume = (volume) => {
  if (player) {
    player.setVolume(volume); // 0-100
  }
};

window.getYouTubePlayerState = () => {
  if (player) {
    return player.getPlayerState();
  }
  return -1;
};
```

```rust
// Rust bindings
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    fn loadYouTubeVideo(videoId: &str, startTime: f64);
    fn playYouTubeVideo();
    fn pauseYouTubeVideo();
    fn seekYouTubeTo(seconds: f64);
    fn setYouTubeVolume(volume: i32);
    fn getYouTubePlayerState() -> i32;
}

// Dioxus component
#[component]
pub fn YouTubeEmbed(video_id: String) -> Element {
    use_effect(move || {
        // Load YouTube IFrame API script
        let script = HtmlElement::new("script");
        script.set_attribute("src", "https://www.youtube.com/iframe_api");
        document().body().append_child(&script);
    });

    rsx! {
        div {
            id: "youtube-player",
            style: "width: 100%; height: 100%;",
        }
    }
}
```

#### General Iframe Embed

```javascript
// public/js/iframeEmbed.js
class IframeEmbed {
  constructor(containerId, url) {
    this.container = document.getElementById(containerId);
    this.iframe = document.createElement('iframe');
    this.iframe.src = url;
    this.iframe.style.width = '100%';
    this.iframe.style.height = '100%';
    this.iframe.style.border = 'none';
    
    // Security: sandbox with minimal permissions
    this.iframe.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-forms');
    
    // Setup message listener
    this.setupMessageListener();
    
    this.container.appendChild(this.iframe);
  }

  setupMessageListener() {
    window.addEventListener('message', (event) => {
      if (event.source !== this.iframe.contentWindow) {
        return; // Not from our iframe
      }
      
      // Validate origin if needed
      // const allowedOrigins = ['https://trusted-site.com'];
      // if (!allowedOrigins.includes(event.origin)) {
      //   return;
      // }
      
      const { type, data } = event.data;
      this.handleMessage(type, data);
    });
  }

  handleMessage(type, data) {
    // Handle iframe messages
    // Send to Rust via postMessage to parent
  }

  sendMessage(type, data) {
    this.iframe.contentWindow.postMessage({
      type,
      data
    }, '*'); // Or specific origin
  }

  getUrl() {
    return this.iframe.src;
  }

  setUrl(url) {
    this.iframe.src = url;
  }

  reload() {
    this.iframe.src = this.iframe.src;
  }
}

window.createIframeEmbed = (containerId, url) => {
  return new IframeEmbed(containerId, url);
};
```

```rust
// Rust bindings
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    type IframeEmbed;
    #[wasm_bindgen(constructor)]
    fn createIframeEmbed(containerId: &str, url: &str) -> IframeEmbed;
    
    #[wasm_bindgen(method)]
    fn getUrl(this: &IframeEmbed) -> String;
    
    #[wasm_bindgen(method)]
    fn setUrl(this: &IframeEmbed, url: &str);
    
    #[wasm_bindgen(method)]
    fn reload(this: &IframeEmbed);
    
    #[wasm_bindgen(method)]
    fn sendMessage(this: &IframeEmbed, type: &str, data: JsValue);
}

// Listen for iframe messages
use_effect(move || {
    let closure = Closure::new(move |event: JsEvent| {
        // Handle iframe messages
    });
    
    window().add_event_listener("message", closure.clone());
    
    || cleanup move || {
        window().remove_event_listener("message", closure);
    }
});
```

### Library Options

| Library | Bundle Size | Features | Pros | Cons |
|--------|------------|---------|------|------|
| **YouTube IFrame API** | ~100KB | Full YouTube control | Official API, well-documented | YouTube-specific |
| **Native Iframe** | 0KB | Basic embed | No dependencies | Limited control |
| **iframe-resizer** | ~10KB | Responsive sizing | Auto-resize | Simple feature |
| **Porthole** | ~30KB | Iframe communication | Message passing | Less maintained |

### Best Practices

1. **Sandboxing**: Always sandbox iframes with minimal permissions
2. **Origin Validation**: Validate message origins in postMessage
3. **Lazy Loading**: Load iframe scripts only when embed opens
4. **Memory Management**: Clean up iframes when embedding closes
5. **Error Handling**: Graceful fallback on iframe load errors

```rust
// Check if sandbox is supported
#[wasm_bindgen]
pub fn supports_sandbox() -> bool {
    let iframe = HtmlIFrameElement::new();
    iframe.set_attribute("sandbox", "allow-scripts");
    true // Always supported in modern browsers
}

// Safe message origin validation
#[wasm_bindgen]
pub fn is_valid_origin(message: JsEvent, allowed_origins: Vec<String>) -> bool {
    // Extract origin from message and validate
}
```

---

## General Dioxus JS Interop Patterns

### wasm-bindgen Setup

```toml
# Cargo.toml
[dependencies]
dioxus = { version = "0.7", features = ["web"] }
wasm-bindgen = "0.2"
serde = { version = "1.0", features = ["derive"] }
serde-wasm-bindgen = "0.6"
js-sys = "0.3"
web-sys = { version = "0.3", features = ["Worker", "Window"] }
```

### Binding JavaScript Libraries

```rust
use wasm_bindgen::prelude::*;
use web_sys::{Window, Document, HtmlElement, HtmlIFrameElement};
use serde::{Deserialize, Serialize};

// Export Rust function to JavaScript
#[wasm_bindgen]
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

// Import JavaScript function into Rust
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window)]
    fn alert(message: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(message: &str);
}

// Async JavaScript function
#[wasm_bindgen]
pub async fn fetch_data(url: String) -> Result<JsValue, JsError> {
    let window = window();
    let response = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;
    
    let json = JsFuture::from(response.json())
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;
    
    Ok(json)
}

// Serialize Rust struct to JSON
#[derive(Serialize, Deserialize)]
pub struct FileInfo {
    name: String,
    size: u64,
    mime_type: String,
}

#[wasm_bindgen]
pub fn get_file_info() -> JsValue {
    let info = FileInfo {
        name: "document.pdf".to_string(),
        size: 1024000,
        mime_type: "application/pdf".to_string(),
    };
    serde_wasm_bindgen::to_value(&info).unwrap()
}

// Custom error type
#[wasm_bindgen]
pub struct JsError {
    message: String,
}

impl JsError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

#[wasm_bindgen]
impl JsError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}
```

### JavaScript Glue Code Organization

```javascript
// public/js/bundle.js - Main entry point
import { createEditor } from './editor.js';
import { createImageViewer } from './imageViewer.js';
import { createPDFViewer } from './pdfViewer.js';
import { createAudioPlayer } from './audioPlayer.js';
import { createVideoPlayer } from './videoPlayer.js';
import { createYouTubePlayer } from './youtubePlayer.js';
import { createIframeEmbed } from './iframeEmbed.js';

// Export to window for Rust access
window.createEditor = createEditor;
window.createImageViewer = createImageViewer;
window.createPDFViewer = createPDFViewer;
window.createAudioPlayer = createAudioPlayer;
window.createVideoPlayer = createVideoPlayer;
window.loadYouTubeVideo = loadYouTubeVideo;
window.playYouTubeVideo = playYouTubeVideo;
window.pauseYouTubeVideo = pauseYouTubeVideo;
window.createIframeEmbed = createIframeEmbed;

// Initialize YouTube API
window.onYouTubeIframeAPIReady = () => {
  window.ytPlayer = new YT.Player('yt-player', {
    height: '100%',
    width: '100%',
    videoId: '',
    events: {
      'onReady': (event) => console.log('YouTube player ready'),
      'onStateChange': (event) => {
        window.parent.postMessage({
          type: 'youtube-state-change',
          data: { state: event.data }
        }, '*');
      }
    }
  });
};

// Load YouTube API script dynamically
const tag = document.createElement('script');
tag.src = 'https://www.youtube.com/iframe_api';
const firstScriptTag = document.getElementsByTagName('script')[0];
firstScriptTag.parentNode.insertBefore(tag, firstScriptTag);
```

### Dioxus Component Integration

```rust
// common/viewer_components.rs
use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

// Generic viewer component trait
pub trait ViewerComponent {
    fn new(url: String) -> Self;
    fn get_ref(&self) -> Option<JsValue>;
}

// Text Editor component
#[component]
pub fn TextEditorComponent(url: String, content: String, on_change: Callback<String>) -> Element {
    let editor_ref = use_coroutine_handle();
    let content_state = use_signal(|| content);

    use_effect(move || {
        let editor = create_editor("editor-container", &content_state.read(), "javascript");
        editor_ref.set(Some(editor));
    });

    rsx! {
        div {
            class: "text-editor",
            div { id: "editor-container" }
        }
    }
}

// Image Viewer component
#[component]
pub fn ImageViewerComponent(url: String) -> Element {
    let viewer_ref = use_coroutine_handle();

    use_effect(move || {
        let viewer = create_image_viewer("image-container", &url);
        viewer_ref.set(Some(viewer));
    });

    rsx! {
        div {
            class: "image-viewer",
            div { id: "image-container" }
        }
    }
}

// Viewer selector component
#[component]
pub fn ViewerSelector(url: String, file_type: String) -> Element {
    rsx! {
        div {
            class: "viewer-container",
            match file_type.as_str() {
                "text/plain" | "text/html" | "application/json" => TextEditorComponent { url: url.clone(), content: String::new(), on_change: Callback::new(|_| ()) },
                "image/png" | "image/jpeg" | "image/gif" | "image/webp" => ImageViewerComponent { url: url.clone() },
                "application/pdf" => PDFViewerComponent { url: url.clone() },
                "audio/mpeg" | "audio/wav" | "audio/ogg" => AudioPlayerComponent { url: url.clone() },
                "video/mp4" | "video/webm" => VideoPlayerComponent { url: url.clone() },
                _ => GenericViewerComponent { url: url.clone() },
            }
        }
    }
}
```

### Performance Optimization

```rust
// Lazy loading for viewer JS bundles
#[component]
pub fn LazyViewer(url: String) -> Element {
    let is_visible = use_signal(|| false);
    let js_loaded = use_signal(|| false);

    use_effect(move || {
        if is_visible.get() && !js_loaded.get() {
            spawn(async move || {
                // Load viewer JS bundle
                load_viewer_js().await.unwrap();
                js_loaded.set(true);
            });
        }
    });

    rsx! {
        div {
            class: "lazy-viewer",
            onmouseenter: move |_| {
                is_visible.set(true);
            },
            if js_loaded.get() {
                ViewerComponent { url: url.clone() }
            } else {
                div { "Loading..." }
            }
        }
    }
}

// Web Worker for heavy processing
#[wasm_bindgen(module = "/public/workers/worker.js")]
extern "C" {
    #[wasm_bindgen]
    fn process_image(image_data: &[u8]) -> JsValue;
}

#[component]
pub fn WorkerViewer(url: String) -> Element {
    let result_state = use_signal(|| None::<String>);

    use_effect(move || {
        spawn(async move || {
            let image_data = fetch_image_data(&url).await.unwrap();
            let result = process_image(&image_data);
            result_state.set(Some(result.as_string().unwrap()));
        });
    });

    rsx! {
        div {
            match result_state.get().as_ref() {
                Some(result) => rsx! { div { "{result}" } },
                None => rsx! { div { "Processing..." } },
            }
        }
    }
}
```

---

## Security Considerations

### Content Security Policy (CSP)

```html
<!-- Recommended CSP headers -->
<meta http-equiv="Content-Security-Policy" content="
  default-src 'self';
  script-src 'self' 'unsafe-inline' https://esm.sh https://cdnjs.cloudflare.com;
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob: https:;
  media-src 'self' blob:;
  frame-src 'self' https://www.youtube.com;
  connect-src 'self' https:;
  font-src 'self' data:;
">
```

### Iframe Security

```rust
// Safe iframe creation
#[wasm_bindgen]
pub fn create_safe_iframe(url: String, allow_scripts: bool) -> HtmlIFrameElement {
    let iframe = HtmlIFrameElement::new();
    iframe.set_attribute("src", &url);
    iframe.set_attribute("width", "100%");
    iframe.set_attribute("height", "100%");
    
    // Minimal sandbox
    let mut sandbox = String::from("allow-same-origin");
    if allow_scripts {
        sandbox.push_str(" allow-scripts");
    }
    sandbox.push_str(" allow-forms");
    
    iframe.set_attribute("sandbox", &sandbox);
    
    // Credentialless for third-party iframes
    if !is_same_origin(&url) {
        iframe.set_attribute("credentialless", "");
    }
    
    iframe
}
```

### Input Validation

```rust
// Validate file types
#[wasm_bindgen]
pub fn is_allowed_file_type(mime_type: &str) -> bool {
    let allowed = vec![
        "text/plain",
        "text/html",
        "application/json",
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/svg+xml",
        "application/pdf",
        "audio/mpeg",
        "audio/wav",
        "audio/ogg",
        "video/mp4",
        "video/webm",
    ];
    allowed.contains(&mime_type.to_string())
}

// Sanitize URLs
#[wasm_bindgen]
pub fn sanitize_url(url: &str) -> Result<String, String> {
    if url.starts_with("javascript:") || url.starts_with("data:") {
        Err("Invalid URL scheme".to_string())
    } else {
        Ok(url.to_string())
    }
}
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_type_validation() {
        assert!(is_allowed_file_type("image/png"));
        assert!(is_allowed_file_type("application/pdf"));
        assert!(!is_allowed_file_type("application/exe"));
    }

    #[test]
    fn test_url_sanitization() {
        assert_eq!(sanitize_url("https://example.com").unwrap(), "https://example.com");
        assert!(sanitize_url("javascript:alert('x')").is_err());
    }
}
```

### Integration Tests

```javascript
// tests/integration/viewers.test.js
describe('Viewer Integration', () => {
  it('should create editor', () => {
    const editor = window.createEditor('test-container', 'hello', 'javascript');
    expect(editor).toBeDefined();
    editor.destroy();
  });

  it('should create image viewer', () => {
    const viewer = window.createImageViewer('test-container', '/test.jpg');
    expect(viewer).toBeDefined();
  });

  it('should handle invalid URLs', async () => {
    const result = await fetch_data('invalid://url');
    expect(result.error).toBeDefined();
  });
});
```

---

## Conclusion

This research document provides a comprehensive foundation for implementing content viewers in a Dioxus-based web desktop application. Key takeaways:

1. **Use wasm-bindgen** for seamless Rust-JavaScript interop
2. **Prefer native HTML5 elements** for audio/video when possible
3. **Lazy load JavaScript libraries** to reduce initial bundle size
4. **Implement proper sandboxing** for iframes and embeds
5. **Use Web Workers** for heavy processing (PDF parsing, image processing)
6. **Follow security best practices**: CSP, origin validation, input sanitization
7. **Implement responsive design** with CSS aspect-ratio or padding tricks
8. **Provide keyboard shortcuts** and accessibility features
9. **Handle errors gracefully** with fallbacks and user feedback
10. **Test thoroughly** with unit and integration tests

Next steps:
1. Implement viewer components incrementally
2. Add error handling and loading states
3. Implement undo/redo for editors
4. Add playlist support for audio/video
5. Implement annotation support for PDFs
6. Add picture-in-picture for video
7. Create viewer factory for automatic selection based on file type

---

**Document Version**: 1.0  
**Last Updated**: 2025-02-05  
**Research Sources**:
- MDN Web Docs (Web APIs, Media Guides)
- Dioxus Documentation
- wasm-bindgen Documentation
- CodeMirror 6 Documentation
- PDF.js Documentation
- YouTube IFrame API Documentation
- Web Speech API Documentation
- HTML5 Audio/Video Specifications
