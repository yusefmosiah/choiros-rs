//! Markdown Parsing and Rendering Module
//!
//! This module provides markdown parsing capabilities for ChoirOS chat messages.
//! Uses pulldown-cmark for parsing CommonMark-compliant markdown.
//!
//! ## Features
//! - Code blocks with language support
//! - Inline code
//! - Bold, italic, strikethrough
//! - Lists (ordered and unordered)
//! - Links
//! - Tables (GitHub-flavored)
//! - Blockquotes
//! - Headers
//! - HTML sanitization for security

use pulldown_cmark::{html, Options, Parser};
use regex::Regex;

/// Error type for markdown operations
#[derive(Debug, thiserror::Error)]
pub enum MarkdownError {
    #[error("Failed to parse markdown: {0}")]
    ParseError(String),

    #[error("HTML sanitization failed")]
    SanitizationError,
}

/// Result type for markdown operations
pub type Result<T> = std::result::Result<T, MarkdownError>;

/// Configuration for markdown rendering
#[derive(Debug, Clone)]
pub struct MarkdownConfig {
    /// Enable GitHub-flavored markdown (tables, strikethrough, task lists)
    pub gfm: bool,
    /// Enable HTML sanitization
    pub sanitize_html: bool,
    /// Enable syntax highlighting hints in code blocks
    pub code_syntax_highlighting: bool,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            gfm: true,
            sanitize_html: true,
            code_syntax_highlighting: true,
        }
    }
}

impl MarkdownConfig {
    /// Create a new config with security-focused defaults
    pub fn secure() -> Self {
        Self {
            gfm: true,
            sanitize_html: true,
            code_syntax_highlighting: true,
        }
    }

    /// Create a config for testing (less strict)
    pub fn test() -> Self {
        Self {
            gfm: true,
            sanitize_html: false, // Allow for testing raw HTML
            code_syntax_highlighting: true,
        }
    }
}

/// Parsed markdown result containing structured content
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMarkdown {
    /// Original markdown text
    pub raw: String,
    /// Rendered HTML
    pub html: String,
    /// List of code blocks found (language, content)
    pub code_blocks: Vec<(Option<String>, String)>,
    /// Whether the content contains potentially unsafe HTML
    pub has_unsafe_html: bool,
}

/// Parse markdown and render to HTML
pub fn render_markdown(input: &str, config: &MarkdownConfig) -> Result<ParsedMarkdown> {
    let options = build_options(config);
    let parser = Parser::new_ext(input, options);

    let mut html_output = String::with_capacity(input.len() * 2);
    html::push_html(&mut html_output, parser);

    // Extract code blocks for syntax highlighting
    let code_blocks = extract_code_blocks(input);

    // Check for unsafe HTML if sanitization is enabled
    let has_unsafe_html = if config.sanitize_html {
        contains_unsafe_html(&html_output)
    } else {
        false
    };

    // Sanitize if needed
    let final_html = if config.sanitize_html && has_unsafe_html {
        sanitize_html(&html_output).map_err(|_| MarkdownError::SanitizationError)?
    } else {
        html_output
    };

    Ok(ParsedMarkdown {
        raw: input.to_string(),
        html: final_html,
        code_blocks,
        has_unsafe_html,
    })
}

/// Render markdown to HTML string (simple interface)
pub fn render_to_html(input: &str) -> String {
    let config = MarkdownConfig::default();
    match render_markdown(input, &config) {
        Ok(parsed) => parsed.html,
        Err(_) => escape_html(input), // Fallback to escaped text on error
    }
}

/// Render markdown with a specific configuration
pub fn render_to_html_with_config(input: &str, config: &MarkdownConfig) -> Result<String> {
    render_markdown(input, config).map(|parsed| parsed.html)
}

/// Build parser options from config
fn build_options(config: &MarkdownConfig) -> Options {
    let mut options = Options::empty();

    // Enable standard options
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    if config.gfm {
        options.insert(Options::ENABLE_TABLES);
    }

    options
}

/// Extract code blocks from markdown
fn extract_code_blocks(input: &str) -> Vec<(Option<String>, String)> {
    let mut blocks = Vec::new();
    // Use DOTALL mode (?s) to make . match newlines, and multiline-friendly pattern
    let code_fence_regex = Regex::new(r#"(?s)```(\w+)?\r?\n(.*?)```"#).unwrap();

    for cap in code_fence_regex.captures_iter(input) {
        let language = cap.get(1).map(|m| m.as_str().to_string());
        let content = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        // Trim trailing whitespace/newlines from content
        let content = content.trim_end().to_string();
        blocks.push((language, content));
    }

    blocks
}

/// Check if HTML contains potentially unsafe content
fn contains_unsafe_html(html: &str) -> bool {
    let unsafe_patterns = [
        "<script",
        "</script>",
        "javascript:",
        "onload=",
        "onerror=",
        "onclick=",
        "<iframe",
        "</iframe>",
        "<object",
        "</object>",
        "<embed",
        "</embed>",
    ];

    let lower = html.to_lowercase();
    unsafe_patterns
        .iter()
        .any(|pattern| lower.contains(pattern))
}

/// Sanitize HTML by removing unsafe tags and attributes
fn sanitize_html(html: &str) -> Result<String> {
    // Simple sanitization - in production, use ammonia crate
    // (?s) enables DOTALL mode so . matches newlines
    let script_regex = Regex::new(r#"(?s)<script[^>]*>.*?</script>"#).unwrap();
    let iframe_regex = Regex::new(r#"(?s)<iframe[^>]*>.*?</iframe>"#).unwrap();
    let object_regex = Regex::new(r#"(?s)<object[^>]*>.*?</object>"#).unwrap();
    // <embed> can be self-closing or not have closing tag
    let embed_regex = Regex::new(r#"<embed[^>]*>"#).unwrap();
    let event_handler_regex = Regex::new(r#"\s*on\w+=["'][^"']*["']"#).unwrap();
    let js_protocol_regex = Regex::new(r#"href=["']javascript:[^"']*["']"#).unwrap();

    let mut sanitized = html.to_string();
    sanitized = script_regex.replace_all(&sanitized, "").to_string();
    sanitized = iframe_regex.replace_all(&sanitized, "").to_string();
    sanitized = object_regex.replace_all(&sanitized, "").to_string();
    sanitized = embed_regex.replace_all(&sanitized, "").to_string();
    sanitized = event_handler_regex.replace_all(&sanitized, "").to_string();
    sanitized = js_protocol_regex
        .replace_all(&sanitized, "href=\"#\"")
        .to_string();

    Ok(sanitized)
}

/// Escape HTML special characters
fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Check if text contains markdown syntax
pub fn contains_markdown(text: &str) -> bool {
    // Use regex to detect actual markdown patterns with word boundaries
    let patterns = [
        r"```",                             // Code block
        r"`[^`]+`",                         // Inline code (content between backticks)
        r"\*\*[^\*]+\*\*",                  // Bold with **
        r"__[^_]+__",                       // Bold with __
        r"(?<![\*\w])\*[^\*]+\*(?![\*\w])", // Italic with * (not part of bold)
        r"(?<!\w)_[^_]+_(?!\w)",            // Italic with _ (word boundaries)
        r"^#{1,6}\s+",                      // Headers at start of line
        r"^\s*[-\*]\s+",                    // Unordered list
        r"^\s*\d+\.\s+",                    // Ordered list
        r"^\s*>\s+",                        // Blockquote
        r"\[([^\]]+)\]\(([^)]+)\)",         // Links [text](url)
        r"\|[^|]+\|",                       // Tables
        r"~~[^~]+~~",                       // Strikethrough
    ];

    patterns.iter().any(|pattern| {
        Regex::new(pattern)
            .map(|re| re.is_match(text))
            .unwrap_or(false)
    })
}

/// Extract plain text from markdown (remove formatting)
pub fn extract_plain_text(markdown: &str) -> String {
    let options = build_options(&MarkdownConfig::default());
    let parser = Parser::new_ext(markdown, options);

    let mut text = String::new();
    for event in parser {
        use pulldown_cmark::Event;
        match event {
            Event::Text(content) | Event::Code(content) => {
                text.push_str(&content);
            }
            Event::Html(html) => {
                text.push_str(&html);
            }
            Event::SoftBreak | Event::HardBreak => {
                text.push('\n');
            }
            _ => {}
        }
    }

    text
}

/// Get word count from markdown
pub fn word_count(markdown: &str) -> usize {
    let plain = extract_plain_text(markdown);
    plain.split_whitespace().count()
}

/// Get character count from markdown (excluding formatting)
pub fn char_count(markdown: &str) -> usize {
    extract_plain_text(markdown).chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_basic_paragraph() {
        let input = "Hello, world!";
        let html = render_to_html(input);
        assert!(html.contains("Hello, world!"));
    }

    #[test]
    fn test_contains_markdown() {
        assert!(contains_markdown("This has **bold** text"));
        assert!(contains_markdown("`code` here"));
        assert!(!contains_markdown("Plain text only"));
    }

    #[test]
    fn test_extract_plain_text() {
        let markdown = "Hello **world**!";
        let plain = extract_plain_text(markdown);
        assert_eq!(plain, "Hello world!");
    }

    #[test]
    fn test_word_count() {
        let markdown = "Hello **world**! This is `code`.";
        assert_eq!(word_count(markdown), 5);
    }
}
