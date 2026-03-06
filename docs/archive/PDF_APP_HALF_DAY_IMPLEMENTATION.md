# PDF App: Half-Day Implementation Guide

**Status:** Implementation Plan
**Date:** 2026-02-08
**Scope:** PDF extraction, generation, and agentic workflows
**Timeline:** ~4 hours (single-session implementation)

---

## Why This Is "Relatively Automatic"

ChoirOS has already solved the hard problems:

| Problem | Already Built | PDF Leverages |
|---------|--------------|---------------|
| Async worker spawning | `TerminalActor`, `ChatAgent` | `PdfActor` follows same pattern |
| Event persistence | `EventStoreActor` with SQLite | PDF operations emit events |
| Binary content handling | `viewer.rs` image base64 encoding | PDFs use same transport |
| Frontend viewers | `TextViewer`, `ImageViewer` components | `PdfViewer` extends pattern |
| MIME type routing | `infer_mime()` in `viewer.rs` | Add `"pdf"` branch |
| WASM integration | `dioxus-web`, `js-sys` bindings | PDF.js or pdfium-WASM |

**The insight:** We're not building PDF infrastructure. We're **wiring PDF libraries into existing infrastructure.**

---

## The 4-Hour Breakdown

### Hour 1: Backend Extraction (markitdown-rs)

**Goal:** PDFs can be uploaded and converted to Markdown via API.

**Files to modify:**
```
sandbox/Cargo.toml          - add markitdown = "0.1"
sandbox/src/api/viewer.rs   - extend infer_mime() for PDF
sandbox/src/actors/pdf.rs   - new actor (100 lines)
```

**Code:**
```rust
// sandbox/src/actors/pdf.rs
use markitdown::MarkItDown;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};

pub enum PdfMsg {
    ExtractToMarkdown {
        path: String,
        reply: RpcReplyPort<String>,
    },
}

pub struct PdfActor;

#[async_trait::async_trait]
impl Actor for PdfActor {
    type Msg = PdfMsg;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            PdfMsg::ExtractToMarkdown { path, reply } => {
                let mut md = MarkItDown::new();
                let result = md.convert(&path, None)
                    .map(|r| r.text_content)
                    .unwrap_or_else(|e| format!("Error: {}", e));
                let _ = reply.send(result);
            }
        }
        Ok(())
    }
}
```

**Integration in viewer.rs:**
```rust
// sandbox/src/api/viewer.rs:infer_mime()
"pdf" => "application/pdf".to_string(),

// In load_initial_snapshot(), add:
"application/pdf" => {
    // Spawn pdf extraction, store markdown in EventStore
    let pdf_actor = state.app_state.pdf_actor();
    let markdown = ractor::call!(pdf_actor, |reply| PdfMsg::ExtractToMarkdown {
        path: path.clone(),
        reply,
    })?;
    Ok(Some(ViewerSnapshot {
        mime: "text/markdown".to_string(), // Converted
        content: markdown,
        revision: make_revision(0),
        readonly: true,
    }))
}
```

**Test:**
```bash
curl "http://localhost:8080/api/viewer/content?uri=file://test.pdf"
# Returns: { "success": true, "mime": "text/markdown", "content": "# Extracted..." }
```

---

### Hour 2: PDF Generation (lopdf)

**Goal:** Agents can generate PDFs from Markdown.

**Add to Cargo.toml:**
```toml
lopdf = "0.38"
```

**Extend PdfActor:**
```rust
pub enum PdfMsg {
    // ... existing
    GenerateFromMarkdown {
        markdown: String,
        output_path: String,
        reply: RpcReplyPort<Result<(), String>>,
    },
}

// In handle():
PdfMsg::GenerateFromMarkdown { markdown, output_path, reply } => {
    let result = generate_pdf_from_md(&markdown, &output_path)
        .map_err(|e| e.to_string());
    let _ = reply.send(result);
}
```

**Simple implementation:**
```rust
fn generate_pdf_from_md(md: &str, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use lopdf::{Document, Object, Stream, dictionary};
    use lopdf::content::{Content, Operation};

    let mut doc = Document::with_version("1.5");

    // Create page
    let page_id = doc.new_object_id();
    let content_id = doc.new_object_id();

    // Simple text layout (improve later)
    let operations = md.lines().enumerate().map(|(i, line)| {
        vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![50.into(), (750 - i * 15) as f64]),
            Operation::new("Tj", vec![Object::string_literal(line)]),
            Operation::new("ET", vec![]),
        ]
    }).flatten().collect();

    let content = Content { operations };
    let content_stream = Stream::new(dictionary! {}, content.encode()?);
    doc.objects.insert(content_id, Object::Stream(content_stream));

    // Page dictionary
    let page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => doc.catalog()?.get(b"Pages")?.clone(),
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(page_id, Object::Dictionary(page_dict));

    doc.save(path)?;
    Ok(())
}
```

**API endpoint:**
```rust
// sandbox/src/api/pdf.rs (new file, 50 lines)
pub async fn generate_pdf(
    State(state): State<ApiState>,
    Json(req): Json<GeneratePdfRequest>,
) -> impl IntoResponse {
    let pdf_actor = state.app_state.pdf_actor();
    let output_path = format!("{}/{}.pdf", state.workspace_dir, req.filename);

    match ractor::call!(pdf_actor, |reply| PdfMsg::GenerateFromMarkdown {
        markdown: req.markdown,
        output_path: output_path.clone(),
        reply,
    }) {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({
            "success": true,
            "path": output_path
        }))),
        Ok(Err(e)) => (StatusCode::500, Json(json!({
            "success": false,
            "error": e
        }))),
        Err(_) => (StatusCode::500, Json(json!({
            "success": false,
            "error": "Actor failed"
        }))),
    }
}
```

---

### Hour 3: Frontend Viewer (Dioxus)

**Goal:** View PDFs in the browser.

**Two options (choose one):**

#### Option A: PDF.js Integration (Fastest)
```rust
// dioxus-desktop/src/viewers/pdf.rs
use dioxus::prelude::*;

#[component]
pub fn PdfViewer(uri: String) -> Element {
    let pdf_content = use_resource(move || {
        let uri = uri.clone();
        async move {
            fetch_pdf_as_data_url(&uri).await
        }
    });

    rsx! {
        iframe {
            src: "https://mozilla.github.io/pdf.js/web/viewer.html?file={pdf_content}"
            width: "100%",
            height: "100%",
            style: "border: none;"
        }
    }
}
```

#### Option B: Server-Side Render (Better for agents)
Already done in Hour 1! The `viewer.rs` endpoint returns Markdown. Just render that with the existing `TextViewer` component.

**Decision:** Use Option B for now. PDF.js adds complexity; server-side extraction fits agent workflows better.

---

### Hour 4: Agent Integration (Tool Definition)

**Goal:** Agents can read and generate PDFs as tools.

**Add to sandbox tool system:**
```rust
// sandbox/src/tools/pdf.rs
use serde_json::json;

pub struct PdfTools;

impl PdfTools {
    pub fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "pdf_extract_text".to_string(),
                description: "Extract text from a PDF file".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to PDF file" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "pdf_generate".to_string(),
                description: "Generate a PDF from Markdown content".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "Markdown content" },
                        "filename": { "type": "string", "description": "Output filename" }
                    },
                    "required": ["content", "filename"]
                }),
            },
        ]
    }

    pub async fn execute(
        tool: &str,
        args: serde_json::Value,
        pdf_actor: &ActorRef<PdfMsg>,
    ) -> Result<String, String> {
        match tool {
            "pdf_extract_text" => {
                let path = args["path"].as_str().ok_or("Missing path")?;
                let result = ractor::call!(pdf_actor, |reply| PdfMsg::ExtractToMarkdown {
                    path: path.to_string(),
                    reply,
                }).map_err(|e| e.to_string())?;
                Ok(result)
            }
            "pdf_generate" => {
                let content = args["content"].as_str().ok_or("Missing content")?;
                let filename = args["filename"].as_str().ok_or("Missing filename")?;
                let output_path = format!("workspace/{}", filename);
                ractor::call!(pdf_actor, |reply| PdfMsg::GenerateFromMarkdown {
                    markdown: content.to_string(),
                    output_path,
                    reply,
                }).map_err(|e| e.to_string())??;
                Ok(format!("Generated: {}", filename))
            }
            _ => Err("Unknown tool".to_string()),
        }
    }
}
```

**Register in ChatAgent:**
```rust
// sandbox/src/actors/chat_agent.rs
let tools = vec![
    // ... existing tools
    PdfTools::definitions(),
];
```

**Test conversation:**
```
User: Extract the text from report.pdf
Agent: [calls pdf_extract_text] The document contains Q3 financial results...

User: Create a PDF summary of that
Agent: [calls pdf_generate with markdown content] Generated: summary.pdf
```

---

## What You Get in 4 Hours

| Feature | Implementation | Integration |
|---------|---------------|-------------|
| PDF → Markdown | markitdown-rs | Via viewer API |
| Markdown → PDF | lopdf | Via PdfActor |
| Agent tools | 2 tool definitions | ChatAgent |
| Frontend viewer | Existing TextViewer | Markdown rendering |
| Event persistence | Automatic | EventStoreActor |

**What's NOT included (future work):**
- PDF.js WASM client-side rendering (adds 2-4 hours)
- Form field extraction/filling (adds pdfium-render, 2-3 hours)
- Complex table preservation (adds layout analysis, 4-6 hours)
- OCR for scanned PDFs (adds tesseract dependency, 2-3 hours)

These are **incremental enhancements**, not core functionality.

---

## Why This Fits ChoirOS

1. **Actor model:** `PdfActor` is just another worker. Follows `TerminalActor` pattern.
2. **Event sourcing:** PDF operations emit events automatically via `EventStoreActor`.
3. **Viewer abstraction:** PDFs become Markdown, reuse existing viewer components.
4. **Tool system:** Agents get PDF capabilities through existing tool infrastructure.
5. **WASM-ready:** Architecture supports future client-side PDFium without rewrites.

---

## Testing Checklist

```bash
# 1. Extraction works
curl "http://localhost:8080/api/viewer/content?uri=file://test.pdf"
# Expect: { "success": true, "mime": "text/markdown", "content": "..." }

# 2. Generation works
curl -X POST http://localhost:8080/api/pdf/generate \
  -H "Content-Type: application/json" \
  -d '{"markdown": "# Hello\n\nWorld", "filename": "test.pdf"}'
# Expect: { "success": true, "path": "workspace/test.pdf" }

# 3. Agent can use tools
# In chat: "Extract text from document.pdf"
# Expect: Agent returns extracted content
```

---

## Post-Implementation: The Full Vision

This half-day implementation unlocks the complete PDF architecture described in the strategic report:

```
Phase 1 (Done): PDF ↔ Markdown (bi-directional)
Phase 2 (Next):  Structured extraction (tables, forms)
Phase 3 (Next):  WASM client rendering
Phase 4 (Next):  Form filling, annotations, redlines
```

Each phase builds on the `PdfActor` foundation. The hard part (infrastructure) is ChoirOS. The easy part (PDF libraries) is wiring.

---

*The automatic computer doesn't need custom PDF infrastructure. It needs actors that speak PDF.*
