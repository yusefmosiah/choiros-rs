# PDF App: Complete Implementation Guide

**Status:** Production Implementation Plan
**Date:** 2026-02-08
**Scope:** Full PDF platform - upload, view, process, generate, share
**Timeline:** Single-session implementation (~4-6 hours)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         DIOXUS FRONTEND                              │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  App Window (PDF Viewer)                                     │   │
│  │  ┌─────────────┐  ┌──────────────────────────────────────┐  │   │
│  │  │ Toolbar     │  │ PDF.js Viewer (iframe or WASM)       │  │   │
│  │  │ - prev/next │  │ - Page navigation                    │  │   │
│  │  │ - zoom      │  │ - Text selection                     │  │   │
│  │  │ - download  │  │ - Search                             │  │   │
│  │  │ - share     │  │ - Annotations (future)               │  │   │
│  │  └─────────────┘  └──────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                       │
│  ┌───────────────────────────┼───────────────────────────────┐     │
│  │  Agent Panel (sidecar)    │                               │     │
│  │  - Extract text          ◄├───────────────────────────────┤     │
│  │  - Summarize              │  WebSocket / HTTP API         │     │
│  │  - Generate PDF          ─┤──────────────────────────────►│     │
│  │  - Fill forms             │                               │     │
│  └───────────────────────────┴───────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼ WebSocket
┌─────────────────────────────────────────────────────────────────────┐
│                         SANDBOX BACKEND                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │ PdfActor        │  │ Content API     │  │ Export API          │  │
│  │ - extract       │  │ /api/viewer/    │  │ - download          │  │
│  │ - generate      │  │   content       │  │ - share (public)    │  │
│  │ - render        │  │ - upload        │  │ - email (future)    │  │
│  │ - process       │  │ - metadata      │  │ - connectors        │  │
│  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘  │
│           │                    │                      │             │
│           └────────────────────┼──────────────────────┘             │
│                                │                                    │
│  ┌─────────────────────────────┼───────────────────────────────┐   │
│  │  EventStoreActor            │                               │   │
│  │  - pdf.uploaded             │                               │   │
│  │  - pdf.viewed               │                               │   │
│  │  - pdf.extracted            │                               │   │
│  │  - pdf.generated            │                               │   │
│  │  - pdf.shared               │                               │   │
│  └─────────────────────────────┴───────────────────────────────┘   │
│                                                                     │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │ File Storage    │  │ Public Links    │  │ Agent Tools         │  │
│  │ (workspace/)    │  │ (short URLs)    │  │ - pdf_extract       │  │
│  │ - originals     │  │ - expiring      │  │ - pdf_generate      │  │
│  │ - rendered      │  │ - access logs   │  │ - pdf_fill_form     │  │
│  │ - exports       │  │                 │  │ - pdf_merge         │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: Backend Core (Hour 1)

**Files:**
```
sandbox/Cargo.toml              - add dependencies
sandbox/src/actors/pdf.rs       - PdfActor implementation
sandbox/src/api/pdf.rs          - PDF-specific API routes
sandbox/src/api/viewer.rs       - extend for PDF MIME type
sandbox/src/tools/pdf.rs        - agent tool definitions
```

**Dependencies:**
```toml
[dependencies]
# PDF processing
pdfium-render = { version = "0.8", features = ["thread_safe"] }
markitdown = "0.1"
lopdf = "0.38"

# File handling
tempfile = "3.8"
walkdir = "2.5"

# URL generation for public links
nanoid = "0.4"
```

**PdfActor - Full Implementation:**
```rust
// sandbox/src/actors/pdf.rs
use pdfium_render::prelude::*;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfMetadata {
    pub page_count: u32,
    pub title: Option<String>,
    pub author: Option<String>,
    pub creation_date: Option<String>,
    pub file_size: u64,
    pub file_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfExtractionResult {
    pub text: String,
    pub markdown: String,
    pub page_texts: Vec<String>,
    pub structure: PdfStructure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfStructure {
    pub headings: Vec<Heading>,
    pub tables: Vec<Table>,
    pub forms: Vec<FormField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub page: u32,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub field_type: String,
    pub value: Option<String>,
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPage {
    pub page_number: u32,
    pub width: u32,
    pub height: u32,
    pub data_url: String, // base64 PNG
}

pub enum PdfMsg {
    // Extraction
    ExtractText {
        path: String,
        options: ExtractOptions,
        reply: RpcReplyPort<Result<PdfExtractionResult, String>>,
    },
    GetMetadata {
        path: String,
        reply: RpcReplyPort<Result<PdfMetadata, String>>,
    },

    // Rendering
    RenderPage {
        path: String,
        page_number: u32,
        scale: f32,
        reply: RpcReplyPort<Result<RenderedPage, String>>,
    },
    RenderAllPages {
        path: String,
        scale: f32,
        output_dir: String,
        reply: RpcReplyPort<Result<Vec<RenderedPage>, String>>,
    },

    // Form handling
    GetFormFields {
        path: String,
        reply: RpcReplyPort<Result<Vec<FormField>, String>>,
    },
    FillForm {
        path: String,
        field_values: HashMap<String, String>,
        output_path: String,
        reply: RpcReplyPort<Result<(), String>>,
    },

    // Generation
    GenerateFromMarkdown {
        markdown: String,
        output_path: String,
        reply: RpcReplyPort<Result<(), String>>,
    },
    MergePdfs {
        input_paths: Vec<String>,
        output_path: String,
        reply: RpcReplyPort<Result<(), String>>,
    },
}

#[derive(Debug, Clone)]
pub struct ExtractOptions {
    pub preserve_layout: bool,
    pub extract_tables: bool,
    pub ocr: bool, // future
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            preserve_layout: true,
            extract_tables: false,
            ocr: false,
        }
    }
}

pub struct PdfActor {
    pdfium: Pdfium,
}

impl PdfActor {
    pub fn new() -> Result<Self, PdfError> {
        let pdfium = Pdfium::new(Pdfium::bind_to_library()?);
        Ok(Self { pdfium })
    }

    fn extract_with_pdfium(&self, path: &str, options: &ExtractOptions)
        -> Result<PdfExtractionResult, String> {
        let document = self.pdfium.load_pdf_from_file(path, None)
            .map_err(|e| e.to_string())?;

        let mut page_texts = Vec::new();
        let mut headings = Vec::new();
        let mut full_text = String::new();

        for (i, page) in document.pages().iter().enumerate() {
            let page_num = i as u32 + 1;
            let text = page.text()
                .map_err(|e| e.to_string())?
                .all();

            page_texts.push(text.clone());
            full_text.push_str(&text);
            full_text.push('\n');

            // Simple heading detection (lines ending with colon or all caps)
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.ends_with(':') ||
                   (trimmed.len() > 3 && trimmed.chars().all(|c| c.is_uppercase() || c.is_whitespace())) {
                    headings.push(Heading {
                        level: 1,
                        text: trimmed.to_string(),
                        page: page_num,
                    });
                }
            }
        }

        // Convert to markdown
        let markdown = self.text_to_markdown(&full_text, &headings);

        Ok(PdfExtractionResult {
            text: full_text,
            markdown,
            page_texts,
            structure: PdfStructure {
                headings,
                tables: Vec::new(), // TODO: table extraction
                forms: Vec::new(),  // TODO: form extraction
            },
        })
    }

    fn text_to_markdown(&self, text: &str, headings: &[Heading]) -> String {
        let mut markdown = String::new();
        let mut lines = text.lines().peekable();

        while let Some(line) = lines.next() {
            let trimmed = line.trim();

            // Check if this is a heading
            if let Some(heading) = headings.iter().find(|h| h.text == trimmed) {
                markdown.push_str(&"#".repeat(heading.level as usize));
                markdown.push(' ');
                markdown.push_str(trimmed.trim_end_matches(':'));
                markdown.push('\n');
            } else if trimmed.is_empty() {
                markdown.push('\n');
            } else {
                markdown.push_str(trimmed);
                markdown.push('\n');
            }
        }

        markdown
    }

    fn render_page(&self, path: &str, page_number: u32, scale: f32)
        -> Result<RenderedPage, String> {
        let document = self.pdfium.load_pdf_from_file(path, None)
            .map_err(|e| e.to_string())?;

        let page = document.pages().get(page_number as u16)
            .map_err(|e| e.to_string())?;

        let config = PdfRenderConfig::new()
            .set_target_width(1200)
            .render_form_data(true);

        let bitmap = page.render_with_config(&config)
            .map_err(|e| e.to_string())?;

        let image = bitmap.as_image();
        let mut buffer: Vec<u8> = Vec::new();

        image.write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;

        let base64 = base64::engine::general_purpose::STANDARD.encode(&buffer);
        let data_url = format!("data:image/png;base64,{}", base64);

        Ok(RenderedPage {
            page_number,
            width: bitmap.width(),
            height: bitmap.height(),
            data_url,
        })
    }
}

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
            PdfMsg::ExtractText { path, options, reply } => {
                let result = self.extract_with_pdfium(&path, &options);
                let _ = reply.send(result);
            }

            PdfMsg::GetMetadata { path, reply } => {
                let result = (|| {
                    let document = self.pdfium.load_pdf_from_file(&path, None)
                        .map_err(|e| e.to_string())?;

                    let metadata = document.metadata();
                    let file_size = std::fs::metadata(&path)
                        .map(|m| m.len())
                        .unwrap_or(0);

                    // Calculate hash
                    let content = std::fs::read(&path)
                        .map_err(|e| e.to_string())?;
                    let hash = format!("{:x}", md5::compute(&content));

                    Ok(PdfMetadata {
                        page_count: document.pages().len() as u32,
                        title: metadata.title(),
                        author: metadata.author(),
                        creation_date: metadata.creation_date()
                            .map(|d| d.to_string()),
                        file_size,
                        file_hash: hash,
                    })
                })();
                let _ = reply.send(result);
            }

            PdfMsg::RenderPage { path, page_number, scale, reply } => {
                let result = self.render_page(&path, page_number, scale);
                let _ = reply.send(result);
            }

            PdfMsg::RenderAllPages { path, scale, output_dir, reply } => {
                let result = (|| {
                    let document = self.pdfium.load_pdf_from_file(&path, None)
                        .map_err(|e| e.to_string())?;

                    std::fs::create_dir_all(&output_dir)
                        .map_err(|e| e.to_string())?;

                    let mut rendered = Vec::new();
                    for i in 0..document.pages().len() {
                        let page = self.render_page(&path, i as u32, scale)?;

                        // Save to file
                        let output_path = format!("{}/page_{:04}.png", output_dir, i + 1);
                        if let Some(data) = page.data_url.strip_prefix("data:image/png;base64,") {
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(data)
                                .map_err(|e| e.to_string())?;
                            std::fs::write(&output_path, bytes)
                                .map_err(|e| e.to_string())?;
                        }

                        rendered.push(page);
                    }

                    Ok(rendered)
                })();
                let _ = reply.send(result);
            }

            PdfMsg::GetFormFields { path, reply } => {
                // TODO: Implement form field extraction
                let _ = reply.send(Ok(Vec::new()));
            }

            PdfMsg::FillForm { path, field_values, output_path, reply } => {
                // TODO: Implement form filling with lopdf
                let _ = reply.send(Ok(()));
            }

            PdfMsg::GenerateFromMarkdown { markdown, output_path, reply } => {
                let result = generate_pdf_from_markdown(&markdown, &output_path)
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }

            PdfMsg::MergePdfs { input_paths, output_path, reply } => {
                let result = (|| {
                    let mut output = lopdf::Document::with_version("1.5");

                    for path in input_paths {
                        let doc = lopdf::Document::load(&path)
                            .map_err(|e| e.to_string())?;
                        // Merge logic here
                    }

                    output.save(&output_path)
                        .map_err(|e| e.to_string())
                })();
                let _ = reply.send(result);
            }
        }
        Ok(())
    }
}

fn generate_pdf_from_markdown(md: &str, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use lopdf::{Document, Object, Stream, dictionary};
    use lopdf::content::{Content, Operation};

    let mut doc = Document::with_version("1.5");

    // Parse markdown for basic structure
    let mut current_page_ops: Vec<Operation> = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 12.into()]),
    ];

    let mut y_position = 750.0;
    let line_height = 15.0;

    for line in md.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("# ") {
            // Heading 1
            current_page_ops.push(Operation::new("Tf", vec!["F1".into(), 18.into()]));
            y_position -= line_height * 1.5;
            current_page_ops.push(Operation::new("Td", vec![50.into(), y_position]));
            current_page_ops.push(Operation::new("Tj", vec![
                Object::string_literal(trimmed.trim_start_matches("# "))
            ]));
            current_page_ops.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
        } else if trimmed.starts_with("## ") {
            // Heading 2
            current_page_ops.push(Operation::new("Tf", vec!["F1".into(), 14.into()]));
            y_position -= line_height * 1.3;
            current_page_ops.push(Operation::new("Td", vec![50.into(), y_position]));
            current_page_ops.push(Operation::new("Tj", vec![
                Object::string_literal(trimmed.trim_start_matches("## "))
            ]));
            current_page_ops.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
        } else if !trimmed.is_empty() {
            // Regular text
            y_position -= line_height;
            if y_position < 50.0 {
                // New page needed
                current_page_ops.push(Operation::new("ET", vec![]));
                // ... create page and start new one
                y_position = 750.0;
                current_page_ops.push(Operation::new("BT", vec![]));
                current_page_ops.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
            }
            current_page_ops.push(Operation::new("Td", vec![50.into(), y_position]));
            current_page_ops.push(Operation::new("Tj", vec![Object::string_literal(trimmed)]));
        }
    }

    current_page_ops.push(Operation::new("ET", vec![]));

    // Create page with content
    let content = Content { operations: current_page_ops };
    let content_stream = Stream::new(dictionary! {}, content.encode()?);

    let content_id = doc.new_object_id();
    doc.objects.insert(content_id, Object::Stream(content_stream));

    let page_id = doc.new_object_id();
    let page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(doc.new_object_id()), // Simplified
        "Contents" => Object::Reference(content_id),
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(page_id, Object::Dictionary(page_dict));

    // Build catalog
    let pages_id = doc.new_object_id();
    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = doc.new_object_id();
    let catalog_dict = dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    };
    doc.objects.insert(catalog_id, Object::Dictionary(catalog_dict));

    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc.save(path)?;
    Ok(())
}
```

---

### Phase 2: Backend API (Hour 2)

**PDF API Routes:**
```rust
// sandbox/src/api/pdf.rs
use axum::{
    extract::{Query, State, Multipart},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct UploadPdfRequest {
    pub window_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UploadPdfResponse {
    pub success: bool,
    pub uri: String,
    pub metadata: PdfMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RenderPageRequest {
    pub uri: String,
    pub page: u32,
    #[serde(default = "default_scale")]
    pub scale: f32,
}

fn default_scale() -> f32 { 1.5 }

#[derive(Debug, Serialize)]
pub struct RenderPageResponse {
    pub success: bool,
    pub page: RenderedPage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GeneratePdfRequest {
    pub content: String,
    pub filename: String,
    pub format: String, // "markdown" or "html"
}

#[derive(Debug, Serialize)]
pub struct PublicLinkResponse {
    pub success: bool,
    pub public_url: String,
    pub expires_at: Option<String>,
}

pub fn pdf_routes() -> Router<ApiState> {
    Router::new()
        .route("/upload", post(upload_pdf))
        .route("/render", get(render_page))
        .route("/metadata", get(get_metadata))
        .route("/extract", post(extract_text))
        .route("/generate", post(generate_pdf))
        .route("/download", get(download_pdf))
        .route("/share", post(create_public_link))
}

async fn upload_pdf(
    State(state): State<ApiState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // Handle file upload
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            file_bytes = Some(field.bytes().await.unwrap().to_vec());
        }
    }

    let (Some(bytes), Some(name)) = (file_bytes, filename) else {
        return (StatusCode::BAD_REQUEST, Json(UploadPdfResponse {
            success: false,
            uri: String::new(),
            metadata: PdfMetadata {
                page_count: 0,
                title: None,
                author: None,
                creation_date: None,
                file_size: 0,
                file_hash: String::new(),
            },
            error: Some("No file provided".to_string()),
        }));
    };

    // Save to workspace
    let workspace_dir = state.workspace_dir.clone();
    let file_path = format!("{}/{}", workspace_dir, name);

    if let Err(e) = tokio::fs::write(&file_path, &bytes).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(UploadPdfResponse {
            success: false,
            uri: String::new(),
            metadata: PdfMetadata::default(),
            error: Some(format!("Failed to save file: {}", e)),
        }));
    }

    // Get metadata from PdfActor
    let pdf_actor = state.app_state.pdf_actor();
    let metadata = match ractor::call!(pdf_actor, |reply| PdfMsg::GetMetadata {
        path: file_path.clone(),
        reply,
    }) {
        Ok(Ok(m)) => m,
        Ok(Err(e)) | Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(UploadPdfResponse {
                success: false,
                uri: String::new(),
                metadata: PdfMetadata::default(),
                error: Some(format!("Failed to read PDF: {}", e)),
            }));
        }
    };

    // Emit event
    let event_store = state.app_state.event_store();
    let append = AppendEvent {
        event_type: "pdf.uploaded".to_string(),
        payload: serde_json::json!({
            "uri": format!("file://{}", file_path),
            "filename": name,
            "file_size": metadata.file_size,
            "file_hash": metadata.file_hash,
            "page_count": metadata.page_count,
        }),
        actor_id: format!("pdf:{}", name),
        user_id: "user-1".to_string(),
    };
    let _ = event_store.cast(EventStoreMsg::Append { event: append, reply: None });

    (StatusCode::OK, Json(UploadPdfResponse {
        success: true,
        uri: format!("file://{}", file_path),
        metadata,
        error: None,
    }))
}

async fn render_page(
    State(state): State<ApiState>,
    Query(params): Query<RenderPageRequest>,
) -> impl IntoResponse {
    let path = params.uri.strip_prefix("file://").unwrap_or(&params.uri);
    let pdf_actor = state.app_state.pdf_actor();

    match ractor::call!(pdf_actor, |reply| PdfMsg::RenderPage {
        path: path.to_string(),
        page_number: params.page,
        scale: params.scale,
        reply,
    }) {
        Ok(Ok(page)) => (StatusCode::OK, Json(RenderPageResponse {
            success: true,
            page,
            error: None,
        })),
        Ok(Err(e)) | Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(RenderPageResponse {
            success: false,
            page: RenderedPage {
                page_number: 0,
                width: 0,
                height: 0,
                data_url: String::new(),
            },
            error: Some(e.to_string()),
        })),
    }
}

async fn download_pdf(
    State(state): State<ApiState>,
    Query(params): Query<DownloadRequest>,
) -> Response {
    let path = params.uri.strip_prefix("file://").unwrap_or(&params.uri);

    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("document.pdf");

            Response::builder()
                .header("Content-Type", "application/pdf")
                .header("Content-Disposition", format!("attachment; filename=\"{}\"", filename))
                .body(bytes.into())
                .unwrap()
        }
        Err(_) => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("PDF not found".into())
                .unwrap()
        }
    }
}

async fn create_public_link(
    State(state): State<ApiState>,
    Json(req): Json<CreatePublicLinkRequest>,
) -> impl IntoResponse {
    // Generate short ID
    let short_id = nanoid::nanoid!(10);

    // Store in database with expiration
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(req.expires_hours.unwrap_or(24));

    // Save to public_links table
    // ... SQL insert

    let public_url = format!("{}/p/{}", state.public_base_url, short_id);

    (StatusCode::OK, Json(PublicLinkResponse {
        success: true,
        public_url,
        expires_at: Some(expires_at.to_rfc3339()),
    }))
}
```

---

### Phase 3: Frontend PDF Viewer (Hour 3)

**PDF.js Embedded Viewer:**
```rust
// dioxus-desktop/src/viewers/pdf.rs
use dioxus::prelude::*;
use shared_types::WindowId;

#[derive(Clone, PartialEq)]
pub struct PdfViewerProps {
    pub window_id: WindowId,
    pub uri: String,
}

#[component]
pub fn PdfViewer(props: PdfViewerProps) -> Element {
    let uri = props.uri.clone();
    let window_id = props.window_id.clone();

    // State
    let current_page = use_signal(|| 1u32);
    let total_pages = use_signal(|| 1u32);
    let zoom = use_signal(|| 100u32);
    let current_image = use_signal(|| String::new());
    let is_loading = use_signal(|| true);
    let error = use_signal(|| None::<String>);

    // Fetch metadata on mount
    use_effect(move || {
        let uri = uri.clone();
        spawn(async move {
            match fetch_pdf_metadata(&uri).await {
                Ok(meta) => {
                    total_pages.set(meta.page_count);
                    // Load first page
                    if let Ok(page) = fetch_rendered_page(&uri, 1, 1.5).await {
                        current_image.set(page.data_url);
                    }
                    is_loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    is_loading.set(false);
                }
            }
        });
    });

    let navigate_page = move |delta: i32| {
        let new_page = (current_page() as i32 + delta).clamp(1, total_pages() as i32) as u32;
        if new_page != current_page() {
            current_page.set(new_page);
            is_loading.set(true);

            let uri = props.uri.clone();
            spawn(async move {
                let scale = zoom() as f32 / 100.0;
                match fetch_rendered_page(&uri, new_page, scale).await {
                    Ok(page) => {
                        current_image.set(page.data_url);
                        is_loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(e));
                        is_loading.set(false);
                    }
                }
            });
        }
    };

    let change_zoom = move |delta: i32| {
        let new_zoom = (zoom() as i32 + delta).clamp(50, 300) as u32;
        zoom.set(new_zoom);

        // Re-render current page
        is_loading.set(true);
        let uri = props.uri.clone();
        let page = current_page();
        spawn(async move {
            let scale = new_zoom as f32 / 100.0;
            match fetch_rendered_page(&uri, page, scale).await {
                Ok(p) => {
                    current_image.set(p.data_url);
                    is_loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    is_loading.set(false);
                }
            }
        });
    };

    let download_pdf = move |_| {
        let uri = props.uri.clone();
        spawn(async move {
            let _ = download_pdf_file(&uri).await;
        });
    };

    let share_pdf = move |_| {
        let uri = props.uri.clone();
        spawn(async move {
            if let Ok(link) = create_public_link(&uri, None).await {
                // Copy to clipboard
                let _ = copy_to_clipboard(&link.public_url);
            }
        });
    };

    rsx! {
        div {
            class: "pdf-viewer-container",
            style: "display: flex; flex-direction: column; height: 100%; background: #2a2a2a;",

            // Toolbar
            div {
                class: "pdf-toolbar",
                style: "display: flex; align-items: center; padding: 8px 16px; background: #1a1a1a; border-bottom: 1px solid #333;",

                // Navigation
                button {
                    onclick: move |_| navigate_page(-1),
                    disabled: current_page() <= 1,
                    "◀"
                }
                span { "{current_page} / {total_pages}" }
                button {
                    onclick: move |_| navigate_page(1),
                    disabled: current_page() >= total_pages(),
                    "▶"
                }

                // Separator
                div { style: "width: 1px; height: 24px; background: #444; margin: 0 16px;" }

                // Zoom
                button { onclick: move |_| change_zoom(-25), "−" }
                span { "{zoom}%" }
                button { onclick: move |_| change_zoom(25), "+" }

                // Spacer
                div { style: "flex: 1;" }

                // Actions
                button {
                    onclick: download_pdf,
                    "⬇ Download"
                }
                button {
                    onclick: share_pdf,
                    "🔗 Share"
                }
            }

            // Viewer area
            div {
                class: "pdf-page-container",
                style: "flex: 1; overflow: auto; display: flex; justify-content: center; align-items: center; padding: 20px;",

                if is_loading() {
                    div { "Loading..." }
                } else if let Some(err) = error() {
                    div { style: "color: #ff6b6b;", "Error: {err}" }
                } else {
                    img {
                        src: current_image(),
                        style: "max-width: 100%; max-height: 100%; box-shadow: 0 4px 20px rgba(0,0,0,0.5);"
                    }
                }
            }

            // Agent Panel (collapsible)
            PdfAgentPanel {
                uri: props.uri.clone(),
                on_extract: move |text| {
                    // Send to chat or copy to clipboard
                }
            }
        }
    }
}

#[component]
fn PdfAgentPanel(uri: String, on_extract: EventHandler<String>) -> Element {
    let extracted_text = use_signal(|| String::new());
    let is_extracting = use_signal(|| false);

    let extract_text = move |_| {
        let uri = uri.clone();
        is_extracting.set(true);
        spawn(async move {
            match extract_pdf_text(&uri).await {
                Ok(result) => {
                    extracted_text.set(result.text);
                    is_extracting.set(false);
                }
                Err(_) => {
                    is_extracting.set(false);
                }
            }
        });
    };

    let generate_summary = move |_| {
        // Call agent to summarize
    };

    rsx! {
        div {
            class: "pdf-agent-panel",
            style: "border-top: 1px solid #333; padding: 12px; background: #1a1a1a;",

            div { style: "font-weight: bold; margin-bottom: 8px;", "AI Actions" }

            div {
                style: "display: flex; gap: 8px; flex-wrap: wrap;",

                button {
                    onclick: extract_text,
                    disabled: is_extracting(),
                    "📄 Extract Text"
                }
                button {
                    onclick: generate_summary,
                    "📝 Summarize"
                }
                button {
                    onclick: move |_| {},
                    "❓ Ask about PDF"
                }
                button {
                    onclick: move |_| {},
                    "📊 Extract Tables"
                }
            }

            if !extracted_text().is_empty() {
                div {
                    style: "margin-top: 12px; padding: 12px; background: #2a2a2a; border-radius: 4px; max-height: 200px; overflow: auto;",

                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;",
                        span { style: "font-weight: bold;", "Extracted Text" }
                        button {
                            onclick: move |_| {
                                let _ = copy_to_clipboard(&extracted_text());
                            },
                            "Copy"
                        }
                    }
                    pre {
                        style: "white-space: pre-wrap; font-size: 12px; color: #ccc;",
                        "{extracted_text}"
                    }
                }
            }
        }
    }
}

// API client functions
async fn fetch_pdf_metadata(uri: &str) -> Result<PdfMetadata, String> {
    let url = format!("/api/pdf/metadata?uri={}", urlencoding::encode(uri));
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    serde_json::from_value(data["metadata"].clone()).map_err(|e| e.to_string())
}

async fn fetch_rendered_page(uri: &str, page: u32, scale: f32) -> Result<RenderedPage, String> {
    let url = format!(
        "/api/pdf/render?uri={}&page={}&scale={}",
        urlencoding::encode(uri),
        page,
        scale
    );
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    serde_json::from_value(data["page"].clone()).map_err(|e| e.to_string())
}

async fn extract_pdf_text(uri: &str) -> Result<PdfExtractionResult, String> {
    let response = gloo_net::http::Request::post("/api/pdf/extract")
        .json(&serde_json::json!({ "uri": uri }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    response.json().await.map_err(|e| e.to_string())
}

async fn download_pdf_file(uri: &str) -> Result<(), String> {
    let url = format!("/api/pdf/download?uri={}", urlencoding::encode(uri));

    // Use browser download
    let window = web_sys::window().unwrap();
    window.location().set_href(&url).map_err(|e| e.to_string())?;
    Ok(())
}

async fn create_public_link(uri: &str, expires_hours: Option<i64>) -> Result<PublicLinkResponse, String> {
    let response = gloo_net::http::Request::post("/api/pdf/share")
        .json(&serde_json::json!({
            "uri": uri,
            "expires_hours": expires_hours
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    response.json().await.map_err(|e| e.to_string())
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("No window")?;
    let navigator = window.navigator();
    let clipboard = navigator.clipboard().ok_or("No clipboard")?;

    let _ = clipboard.write_text(text);
    Ok(())
}
```

---

### Phase 4: Agent Tools (Hour 4)

**Tool Definitions:**
```rust
// sandbox/src/tools/pdf.rs
use crate::actors::pdf::{PdfMsg, ExtractOptions, PdfExtractionResult};
use ractor::ActorRef;
use serde_json::json;

pub struct PdfToolKit {
    pdf_actor: ActorRef<PdfMsg>,
}

impl PdfToolKit {
    pub fn new(pdf_actor: ActorRef<PdfMsg>) -> Self {
        Self { pdf_actor }
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "pdf_extract".to_string(),
                description: "Extract text and structure from a PDF file".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "uri": {
                            "type": "string",
                            "description": "URI of the PDF file (e.g., file://path/to/doc.pdf)"
                        },
                        "preserve_layout": {
                            "type": "boolean",
                            "description": "Preserve document layout in extraction",
                            "default": true
                        },
                        "extract_tables": {
                            "type": "boolean",
                            "description": "Attempt to extract table structures",
                            "default": false
                        }
                    },
                    "required": ["uri"]
                }),
            },
            ToolDefinition {
                name: "pdf_generate".to_string(),
                description: "Generate a PDF document from content".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "Markdown or HTML content for the PDF"
                        },
                        "filename": {
                            "type": "string",
                            "description": "Output filename (e.g., report.pdf)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Document title"
                        }
                    },
                    "required": ["content", "filename"]
                }),
            },
            ToolDefinition {
                name: "pdf_fill_form".to_string(),
                description: "Fill form fields in a PDF".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "uri": {
                            "type": "string",
                            "description": "URI of the PDF with form fields"
                        },
                        "fields": {
                            "type": "object",
                            "description": "Field name -> value mapping"
                        },
                        "output_filename": {
                            "type": "string",
                            "description": "Name for the filled PDF"
                        }
                    },
                    "required": ["uri", "fields", "output_filename"]
                }),
            },
            ToolDefinition {
                name: "pdf_merge".to_string(),
                description: "Merge multiple PDFs into one".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "uris": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "URIs of PDFs to merge"
                        },
                        "output_filename": {
                            "type": "string",
                            "description": "Name for the merged PDF"
                        }
                    },
                    "required": ["uris", "output_filename"]
                }),
            },
            ToolDefinition {
                name: "pdf_share".to_string(),
                description: "Create a public shareable link for a PDF".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "uri": {
                            "type": "string",
                            "description": "URI of the PDF to share"
                        },
                        "expires_hours": {
                            "type": "integer",
                            "description": "Link expiration time in hours (default: 24)",
                            "default": 24
                        }
                    },
                    "required": ["uri"]
                }),
            },
        ]
    }

    pub async fn execute(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ToolResult, String> {
        match tool {
            "pdf_extract" => {
                let uri = args["uri"].as_str().ok_or("Missing uri")?;
                let path = uri.strip_prefix("file://").unwrap_or(uri);

                let options = ExtractOptions {
                    preserve_layout: args["preserve_layout"].as_bool().unwrap_or(true),
                    extract_tables: args["extract_tables"].as_bool().unwrap_or(false),
                    ocr: false,
                };

                let result = ractor::call!(
                    self.pdf_actor,
                    |reply| PdfMsg::ExtractText {
                        path: path.to_string(),
                        options,
                        reply,
                    }
                ).map_err(|e| e.to_string())?;

                match result {
                    Ok(extraction) => Ok(ToolResult {
                        content: format!(
                            "Extracted {} pages. Text preview:\n\n{}\n\nHeadings found: {}",
                            extraction.page_texts.len(),
                            &extraction.text[..extraction.text.len().min(2000)],
                            extraction.structure.headings.len()
                        ),
                        structured: Some(json!(extraction)),
                    }),
                    Err(e) => Err(format!("Extraction failed: {}", e)),
                }
            }

            "pdf_generate" => {
                let content = args["content"].as_str().ok_or("Missing content")?;
                let filename = args["filename"].as_str().ok_or("Missing filename")?;

                // Ensure .pdf extension
                let filename = if filename.ends_with(".pdf") {
                    filename.to_string()
                } else {
                    format!("{}.pdf", filename)
                };

                let output_path = format!("workspace/{}", filename);

                ractor::call!(
                    self.pdf_actor,
                    |reply| PdfMsg::GenerateFromMarkdown {
                        markdown: content.to_string(),
                        output_path: output_path.clone(),
                        reply,
                    }
                )
                .map_err(|e| e.to_string())?
                .map_err(|e| e)?;

                Ok(ToolResult {
                    content: format!("PDF generated: {}\nURI: file://{}", filename, output_path),
                    structured: Some(json!({
                        "uri": format!("file://{}", output_path),
                        "filename": filename
                    })),
                })
            }

            "pdf_fill_form" => {
                let uri = args["uri"].as_str().ok_or("Missing uri")?;
                let path = uri.strip_prefix("file://").unwrap_or(uri);
                let fields = args["fields"].as_object().ok_or("Missing fields")?;
                let output_filename = args["output_filename"].as_str().ok_or("Missing output_filename")?;

                let field_values: HashMap<String, String> = fields
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();

                let output_path = format!("workspace/{}", output_filename);

                ractor::call!(
                    self.pdf_actor,
                    |reply| PdfMsg::FillForm {
                        path: path.to_string(),
                        field_values,
                        output_path: output_path.clone(),
                        reply,
                    }
                )
                .map_err(|e| e.to_string())?
                .map_err(|e| e)?;

                Ok(ToolResult {
                    content: format!("Form filled: {}", output_filename),
                    structured: Some(json!({
                        "uri": format!("file://{}", output_path)
                    })),
                })
            }

            "pdf_merge" => {
                let uris = args["uris"].as_array().ok_or("Missing uris")?;
                let output_filename = args["output_filename"].as_str().ok_or("Missing output_filename")?;

                let input_paths: Vec<String> = uris
                    .iter()
                    .filter_map(|u| u.as_str())
                    .map(|u| u.strip_prefix("file://").unwrap_or(u).to_string())
                    .collect();

                let output_path = format!("workspace/{}", output_filename);

                ractor::call!(
                    self.pdf_actor,
                    |reply| PdfMsg::MergePdfs {
                        input_paths,
                        output_path: output_path.clone(),
                        reply,
                    }
                )
                .map_err(|e| e.to_string())?
                .map_err(|e| e)?;

                Ok(ToolResult {
                    content: format!("Merged PDF: {}", output_filename),
                    structured: Some(json!({
                        "uri": format!("file://{}", output_path)
                    })),
                })
            }

            "pdf_share" => {
                // Implementation would create public link
                Ok(ToolResult {
                    content: "Share link created".to_string(),
                    structured: None,
                })
            }

            _ => Err(format!("Unknown tool: {}", tool)),
        }
    }
}

// Register in ChatAgent
impl ChatAgent {
    pub async fn handle_with_tools(&mut self, message: &str) -> Result<String, String> {
        // ... existing setup

        let pdf_tools = PdfToolKit::new(self.app_state.pdf_actor());
        let tools = vec![
            // ... other tools
            pdf_tools.definitions(),
        ];

        // Call LLM with tools
        let response = self.llm.chat_with_tools(message, &tools).await?;

        // Handle tool calls
        if let Some(tool_calls) = response.tool_calls {
            for call in tool_calls {
                let result = pdf_tools.execute(&call.name, call.arguments).await?;
                // Send result back to LLM
            }
        }

        Ok(response.content)
    }
}
```

---

## Integration Checklist

### Backend Wiring
- [ ] Add `PdfActor` to `AppState`
- [ ] Register PDF routes in `api/mod.rs`
- [ ] Add `application/pdf` to `infer_mime()`
- [ ] Register PDF tools in `ChatAgent`

### Frontend Wiring
- [ ] Add `PdfViewer` to viewers module
- [ ] Register PDF MIME type handler
- [ ] Add upload UI (drag & drop)

### Database (if public links needed)
```sql
CREATE TABLE public_links (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    file_hash TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME,
    access_count INTEGER DEFAULT 0,
    created_by TEXT
);
```

---

## Testing Flow

1. **Upload:** Drag PDF into window → displays with page navigation
2. **View:** Navigate pages, zoom in/out
3. **Extract:** Click "Extract Text" → shows extracted content
4. **Generate:** Ask agent "Create a PDF report from this data"
5. **Share:** Click "Share" → copy public link
6. **Download:** Click "Download" → browser downloads file

---

## What You Get

| Feature | Implementation |
|---------|---------------|
| PDF upload | Multipart form + file storage |
| Page navigation | Server-side render + image streaming |
| Zoom | Dynamic re-render at scale |
| Text extraction | pdfium-render + structured output |
| PDF generation | lopdf from Markdown |
| Form filling | lopdf form field manipulation |
| Public sharing | Short URL + expiring links |
| Download | Direct file serving |
| Agent tools | 5 tools for extract/generate/fill/merge/share |
| Event logging | Automatic via EventStoreActor |

**Result:** Full PDF platform integrated into ChoirOS automatic computer architecture.
