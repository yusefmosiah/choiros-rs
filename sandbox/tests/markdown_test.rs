//! Markdown Rendering Tests for ChoirOS Chat
//!
//! Comprehensive test suite for markdown parsing and rendering.
//! Tests cover code blocks, inline code, formatting, lists, links,
//! tables, blockquotes, headers, security, and edge cases.
//!
//! Run with: cargo test -p sandbox --test markdown_test -- --nocapture

use sandbox::markdown::{
    char_count, contains_markdown, extract_plain_text, render_markdown, render_to_html, word_count,
    MarkdownConfig,
};

// ====================================================================================
// Test Constants
// ====================================================================================

const CODE_BLOCK_RUST: &str = r#"```rust
fn main() {
    println!("Hello, world!");
}
```"#;

const CODE_BLOCK_JS: &str = r#"```javascript
console.log("Hello, world!");
```"#;

const INLINE_CODE: &str = "Use `println!()` macro to print output.";

const BOLD_TEXT: &str = "This is **bold** text.";
const ITALIC_TEXT: &str = "This is *italic* text.";
const BOLD_ITALIC: &str = "This is ***bold and italic*** text.";
const STRIKETHROUGH: &str = "This is ~~strikethrough~~ text.";

const UNORDERED_LIST: &str = r#"- First item
- Second item
- Third item"#;

const ORDERED_LIST: &str = r#"1. First item
2. Second item
3. Third item"#;

const NESTED_LIST: &str = r#"- Item 1
  - Subitem 1.1
  - Subitem 1.2
- Item 2
  - Subitem 2.1"#;

const LINK: &str = "Check out [ChoirOS](https://choiros.dev) for more info.";

const TABLE: &str = r#"| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |"#;

const BLOCKQUOTE: &str = "> This is a blockquote.\n> It can span multiple lines.";

const HEADER_H1: &str = "# Header 1";
const HEADER_H2: &str = "## Header 2";
const HEADER_H3: &str = "### Header 3";

const MIXED_CONTENT: &str = r#"# Main Header

This is a paragraph with **bold** and *italic* text.

## Code Example

```rust
fn hello() {
    println!("Hello!");
}
```

## Features

- Feature 1
- Feature 2
  - Subfeature 2.1

Visit [our site](https://choiros.dev) for more.

> Important quote here.

| Name | Value |
|------|-------|
| Test | 123   |"#;

const MALICIOUS_SCRIPT: &str = r#"<script>alert('XSS')</script>

Normal **bold** text.

<script>
document.location = 'https://evil.com';
</script>"#;

const MALICIOUS_JS_LINK: &str = r#"[Click me](javascript:alert('XSS'))

Normal text."#;

const MALICIOUS_EVENT_HANDLER: &str = r#"<img src="x" onerror="alert('XSS')">

Normal text."#;

// ====================================================================================
// Code Blocks Tests
// ====================================================================================

#[test]
fn test_markdown_code_blocks() {
    let result = render_markdown(CODE_BLOCK_RUST, &MarkdownConfig::test()).unwrap();

    // Should contain code block HTML
    assert!(result.html.contains("<pre"), "Should contain <pre> tag");
    assert!(result.html.contains("<code"), "Should contain <code> tag");
    assert!(
        result.html.contains("fn main()"),
        "Should contain the code content"
    );

    // Should extract code block
    assert_eq!(result.code_blocks.len(), 1, "Should find one code block");
    assert_eq!(
        result.code_blocks[0].0,
        Some("rust".to_string()),
        "Should detect Rust language"
    );
    assert!(
        result.code_blocks[0].1.contains("println!"),
        "Should contain code content"
    );
}

#[test]
fn test_markdown_code_with_language() {
    let languages = vec![
        ("rust", "fn main() {}"),
        ("javascript", "console.log('test');"),
        ("python", "print('test')"),
        ("go", "fmt.Println(\"test\")"),
        ("c", "printf(\"test\");"),
    ];

    for (lang, code) in languages {
        let input = format!("```{lang}\n{code}\n```");
        let result = render_markdown(&input, &MarkdownConfig::test()).unwrap();

        assert_eq!(
            result.code_blocks[0].0,
            Some(lang.to_string()),
            "Should detect {lang} language"
        );
        assert!(
            result.code_blocks[0].1.contains(code),
            "Should preserve {lang} code"
        );
    }
}

#[test]
fn test_markdown_code_block_without_language() {
    let input = "```
plain text code
```";

    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert_eq!(result.code_blocks.len(), 1);
    assert_eq!(result.code_blocks[0].0, None, "Should have no language");
    assert!(result.code_blocks[0].1.contains("plain text code"));
}

#[test]
fn test_markdown_multiple_code_blocks() {
    let input = r#"First block:
```rust
fn a() {}
```

Second block:
```javascript
function b() {}
```"#;

    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert_eq!(result.code_blocks.len(), 2, "Should find two code blocks");
    assert_eq!(result.code_blocks[0].0, Some("rust".to_string()));
    assert_eq!(result.code_blocks[1].0, Some("javascript".to_string()));
}

// ====================================================================================
// Inline Code Tests
// ====================================================================================

#[test]
fn test_markdown_inline_code() {
    let result = render_markdown(INLINE_CODE, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<code>println!()</code>"),
        "Should render inline code with <code> tag: {}",
        result.html
    );
    assert!(
        result.html.contains("Use"),
        "Should preserve surrounding text"
    );
}

#[test]
fn test_markdown_inline_code_in_sentence() {
    let input = "The `Vec<T>` type is generic over `T`.";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    // pulldown-cmark escapes < and > in inline code
    assert!(
        result.html.contains("Vec<T>") || result.html.contains("Vec&lt;T&gt;"),
        "Should contain Vec<T> code: {}",
        result.html
    );
    assert!(
        result.html.contains("<code>"),
        "Should contain code tags: {}",
        result.html
    );
    assert!(result.html.contains("type is generic"));
}

#[test]
fn test_markdown_inline_code_special_chars() {
    let input = "Use `&lt;` and `&gt;` for HTML entities.";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(result.html.contains("<code>"));
    assert!(result.html.contains("</code>"));
}

// ====================================================================================
// Bold and Italic Tests
// ====================================================================================

#[test]
fn test_markdown_bold() {
    let result = render_markdown(BOLD_TEXT, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<strong>bold</strong>") || result.html.contains("<b>bold</b>"),
        "Should render bold text: {}",
        result.html
    );
}

#[test]
fn test_markdown_italic() {
    let result = render_markdown(ITALIC_TEXT, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<em>italic</em>") || result.html.contains("<i>italic</i>"),
        "Should render italic text: {}",
        result.html
    );
}

#[test]
fn test_markdown_bold_italic() {
    let result = render_markdown(BOLD_ITALIC, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<strong>") && result.html.contains("<em>"),
        "Should render bold and italic text: {}",
        result.html
    );
}

#[test]
fn test_markdown_strikethrough() {
    let result = render_markdown(STRIKETHROUGH, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<del>strikethrough</del>")
            || result.html.contains("<s>strikethrough</s>"),
        "Should render strikethrough text: {}",
        result.html
    );
}

#[test]
fn test_markdown_bold_with_underscores() {
    let input = "This is __bold__ text.";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<strong>bold</strong>") || result.html.contains("<b>bold</b>"),
        "Should render bold with underscores: {}",
        result.html
    );
}

#[test]
fn test_markdown_italic_with_underscores() {
    let input = "This is _italic_ text.";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<em>italic</em>") || result.html.contains("<i>italic</i>"),
        "Should render italic with underscores: {}",
        result.html
    );
}

// ====================================================================================
// List Tests
// ====================================================================================

#[test]
fn test_markdown_unordered_list() {
    let result = render_markdown(UNORDERED_LIST, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<ul>") && result.html.contains("</ul>"),
        "Should contain <ul> tags: {}",
        result.html
    );
    assert!(
        result.html.contains("<li>"),
        "Should contain <li> tags: {}",
        result.html
    );
    assert!(result.html.contains("First item"));
    assert!(result.html.contains("Second item"));
    assert!(result.html.contains("Third item"));
}

#[test]
fn test_markdown_ordered_list() {
    let result = render_markdown(ORDERED_LIST, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<ol>") && result.html.contains("</ol>"),
        "Should contain <ol> tags: {}",
        result.html
    );
    assert!(
        result.html.contains("<li>"),
        "Should contain <li> tags: {}",
        result.html
    );
    assert!(result.html.contains("First item"));
    assert!(result.html.contains("Second item"));
    assert!(result.html.contains("Third item"));
}

#[test]
fn test_markdown_nested_lists() {
    let result = render_markdown(NESTED_LIST, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<ul>"),
        "Should contain unordered list tags"
    );

    // Check for nested structure (nested lists appear as <ul> inside <li>)
    let ul_count = result.html.matches("<ul").count();
    assert!(
        ul_count > 1,
        "Should have nested <ul> elements, found {}: {}",
        ul_count,
        result.html
    );

    assert!(result.html.contains("Item 1"));
    assert!(result.html.contains("Subitem 1.1"));
    assert!(result.html.contains("Subitem 1.2"));
    assert!(result.html.contains("Item 2"));
    assert!(result.html.contains("Subitem 2.1"));
}

#[test]
fn test_markdown_mixed_lists() {
    let input = r#"1. Ordered item
   - Unordered subitem
   - Another subitem
2. Another ordered item"#;

    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(result.html.contains("<ol>"), "Should contain ordered list");
    assert!(
        result.html.contains("<ul>"),
        "Should contain unordered list"
    );
    assert!(result.html.contains("Ordered item"));
    assert!(result.html.contains("Unordered subitem"));
}

// ====================================================================================
// Link Tests
// ====================================================================================

#[test]
fn test_markdown_links() {
    let result = render_markdown(LINK, &MarkdownConfig::test()).unwrap();

    assert!(
        result
            .html
            .contains("<a href=\"https://choiros.dev\">ChoirOS</a>"),
        "Should render link correctly: {}",
        result.html
    );
}

#[test]
fn test_markdown_multiple_links() {
    let input = "Visit [Rust](https://rust-lang.org) and [Dioxus](https://dioxuslabs.com).";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(result
        .html
        .contains("<a href=\"https://rust-lang.org\">Rust</a>"));
    assert!(result
        .html
        .contains("<a href=\"https://dioxuslabs.com\">Dioxus</a>"));
}

#[test]
fn test_markdown_link_with_title() {
    let input = r#"[ChoirOS](https://choiros.dev "ChoirOS Homepage")"#;
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(result.html.contains("href=\"https://choiros.dev\""));
    assert!(result.html.contains("ChoirOS"));
}

// ====================================================================================
// Table Tests
// ====================================================================================

#[test]
fn test_markdown_tables() {
    let result = render_markdown(TABLE, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<table>"),
        "Should contain <table> tag: {}",
        result.html
    );
    assert!(result.html.contains("<tr>"), "Should contain table rows");
    assert!(
        result.html.contains("Header 1"),
        "Should contain header content"
    );
    assert!(result.html.contains("Header 2"));
    assert!(
        result.html.contains("Cell 1"),
        "Should contain cell content"
    );
}

#[test]
fn test_markdown_table_alignment() {
    let input = r#"| Left | Center | Right |
|:-----|:------:|------:|
| L    | C      | R     |"#;

    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(result.html.contains("<table>"));
    assert!(result.html.contains("L"));
    assert!(result.html.contains("C"));
    assert!(result.html.contains("R"));
}

// ====================================================================================
// Blockquote Tests
// ====================================================================================

#[test]
fn test_markdown_blockquotes() {
    let result = render_markdown(BLOCKQUOTE, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<blockquote>"),
        "Should contain <blockquote> tag: {}",
        result.html
    );
    assert!(
        result.html.contains("is a blockquote"),
        "Should contain quoted text: {}",
        result.html
    );
}

#[test]
fn test_markdown_nested_blockquotes() {
    let input = "> Outer quote\n>> Nested quote";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    // Nested blockquotes appear as <blockquote> inside <blockquote>
    let blockquote_count = result.html.matches("<blockquote").count();
    assert!(
        blockquote_count > 1,
        "Should have nested blockquotes: {}",
        result.html
    );
}

// ====================================================================================
// Header Tests
// ====================================================================================

#[test]
fn test_markdown_headers() {
    let h1_result = render_markdown(HEADER_H1, &MarkdownConfig::test()).unwrap();
    let h2_result = render_markdown(HEADER_H2, &MarkdownConfig::test()).unwrap();
    let h3_result = render_markdown(HEADER_H3, &MarkdownConfig::test()).unwrap();

    assert!(
        h1_result.html.contains("<h1>Header 1</h1>") || h1_result.html.contains("<h1>"),
        "Should render H1: {}",
        h1_result.html
    );
    assert!(
        h2_result.html.contains("<h2>Header 2</h2>") || h2_result.html.contains("<h2>"),
        "Should render H2: {}",
        h2_result.html
    );
    assert!(
        h3_result.html.contains("<h3>Header 3</h3>") || h3_result.html.contains("<h3>"),
        "Should render H3: {}",
        h3_result.html
    );
}

#[test]
fn test_markdown_all_header_levels() {
    for level in 1..=6 {
        let input = format!("{} Header {}", "#".repeat(level), level);
        let result = render_markdown(&input, &MarkdownConfig::test()).unwrap();

        let expected_tag = format!("<h{level}>");
        assert!(
            result.html.contains(&expected_tag),
            "Should render H{}: {}",
            level,
            result.html
        );
    }
}

// ====================================================================================
// Mixed Content Tests
// ====================================================================================

#[test]
fn test_markdown_mixed_content() {
    let result = render_markdown(MIXED_CONTENT, &MarkdownConfig::test()).unwrap();

    // Check all elements are present
    assert!(
        result.html.contains("<h1>") || result.html.contains("<h2>"),
        "Should have headers"
    );
    assert!(result.html.contains("<p>"), "Should have paragraphs");
    assert!(
        result.html.contains("<strong>bold</strong>") || result.html.contains("<strong>"),
        "Should have bold"
    );
    assert!(
        result.html.contains("<em>italic</em>") || result.html.contains("<em>"),
        "Should have italic"
    );
    assert!(result.html.contains("<pre>"), "Should have code block");
    assert!(result.html.contains("<ul>"), "Should have list");
    assert!(result.html.contains("<a href="), "Should have links");
    assert!(
        result.html.contains("<blockquote>"),
        "Should have blockquote"
    );
    assert!(result.html.contains("<table>"), "Should have table");

    // Check code block extraction
    assert_eq!(result.code_blocks.len(), 1);
    assert_eq!(result.code_blocks[0].0, Some("rust".to_string()));
}

// ====================================================================================
// Security Tests (XSS Prevention)
// ====================================================================================

#[test]
fn test_markdown_html_escaping_script_tags() {
    let result = render_markdown(MALICIOUS_SCRIPT, &MarkdownConfig::default()).unwrap();

    // Should detect unsafe HTML
    assert!(result.has_unsafe_html, "Should detect unsafe HTML");

    // Should sanitize script tags
    assert!(
        !result.html.contains("<script>"),
        "Should remove <script> tags: {}",
        result.html
    );
    assert!(
        !result.html.contains("alert('XSS')"),
        "Should remove script content: {}",
        result.html
    );

    // Should preserve safe content
    assert!(
        result.html.contains("Normal") || result.html.contains("<strong>"),
        "Should preserve safe markdown: {}",
        result.html
    );
}

#[test]
fn test_markdown_javascript_link_sanitization() {
    let result = render_markdown(MALICIOUS_JS_LINK, &MarkdownConfig::default()).unwrap();

    // Should sanitize javascript: protocol
    assert!(
        !result.html.contains("javascript:"),
        "Should remove javascript: protocol: {}",
        result.html
    );

    // Should preserve the link text but safe href
    assert!(
        result.html.contains("Click me"),
        "Should preserve link text"
    );
}

#[test]
fn test_markdown_event_handler_sanitization() {
    let result = render_markdown(MALICIOUS_EVENT_HANDLER, &MarkdownConfig::default()).unwrap();

    // Should remove event handlers
    assert!(
        !result.html.contains("onerror="),
        "Should remove event handlers: {}",
        result.html
    );
}

#[test]
fn test_markdown_iframe_sanitization() {
    let input = "<iframe src='https://evil.com'></iframe>\n\nNormal text.";
    let result = render_markdown(input, &MarkdownConfig::default()).unwrap();

    assert!(
        !result.html.contains("<iframe"),
        "Should remove iframes: {}",
        result.html
    );
    assert!(result.has_unsafe_html, "Should detect iframe as unsafe");
}

#[test]
fn test_markdown_object_embed_sanitization() {
    let input = r#"<object data="evil.swf"></object>
<embed src="evil.swf">

Normal text."#;

    let result = render_markdown(input, &MarkdownConfig::default()).unwrap();

    assert!(
        !result.html.contains("<object"),
        "Should remove object tags: {}",
        result.html
    );
    assert!(
        !result.html.contains("<embed"),
        "Should remove embed tags: {}",
        result.html
    );
}

#[test]
fn test_markdown_safe_html_allowed() {
    let input = "This has <b>bold HTML</b> and <i>italic HTML</i>.";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    // In non-sanitized mode, safe HTML should be preserved
    assert!(
        result.html.contains("<b>bold HTML</b>"),
        "Should preserve safe HTML: {}",
        result.html
    );
    assert!(result.html.contains("<i>italic HTML</i>"));
}

// ====================================================================================
// Edge Case Tests
// ====================================================================================

#[test]
fn test_markdown_empty() {
    let result = render_markdown("", &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.is_empty() || result.html.trim().is_empty(),
        "Empty markdown should produce empty or minimal HTML: {}",
        result.html
    );
    assert!(result.code_blocks.is_empty());
}

#[test]
fn test_markdown_plain_text() {
    let input = "Just plain text without any markdown syntax.";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("Just plain text"),
        "Should preserve plain text: {}",
        result.html
    );
    assert!(result.code_blocks.is_empty(), "Should have no code blocks");
    assert!(!result.has_unsafe_html, "Plain text should be safe");
}

#[test]
fn test_markdown_whitespace_only() {
    let inputs = vec!["   ", "\n\n", "\t\t", " \n \t "];

    for input in inputs {
        let result = render_markdown(input, &MarkdownConfig::test()).unwrap();
        // Whitespace handling varies, but should not panic
        assert!(!result.has_unsafe_html);
    }
}

#[test]
fn test_markdown_special_characters() {
    let input = "Special chars: <>&\"'\nSymbols: @#$%^*()_+-=[]{}|;':\",./<>?";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    // Special chars should be escaped or preserved appropriately
    assert!(!result.html.contains("<script"));
}

#[test]
fn test_markdown_unicode() {
    let input = "Unicode: 你好世界 🎉 émojis: 🦀🔥💯\n数学: ∫∂∆∑∏";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("你好世界"),
        "Should preserve CJK: {}",
        result.html
    );
    assert!(
        result.html.contains("🎉"),
        "Should preserve emojis: {}",
        result.html
    );
}

#[test]
fn test_markdown_very_long_input() {
    let long_text = "A".repeat(10000);
    let input = format!("**{}**", long_text);

    let result = render_markdown(&input, &MarkdownConfig::test()).unwrap();
    assert!(!result.html.is_empty(), "Should handle long input");
}

// ====================================================================================
// Helper Function Tests
// ====================================================================================

#[test]
fn test_contains_markdown_true() {
    assert!(contains_markdown("**bold**"), "Should detect bold");
    assert!(contains_markdown("`code`"), "Should detect code");
    assert!(
        contains_markdown("```rust\ncode\n```"),
        "Should detect code block"
    );
    assert!(contains_markdown("# Header"), "Should detect header");
    assert!(contains_markdown("- Item"), "Should detect list");
    assert!(contains_markdown("> Quote"), "Should detect blockquote");
    assert!(contains_markdown("[link](url)"), "Should detect link");
    assert!(contains_markdown("| table |"), "Should detect table");
}

#[test]
fn test_contains_markdown_false() {
    assert!(
        !contains_markdown("Plain text only"),
        "Should not flag plain text"
    );
    assert!(!contains_markdown("12345"), "Should not flag numbers");
    assert!(
        !contains_markdown("Text with some asterisks* but not markdown"),
        "Should not flag single asterisk"
    );
}

#[test]
fn test_extract_plain_text() {
    let markdown = "Hello **world** with `code` and *italic*.";
    let plain = extract_plain_text(markdown);

    assert_eq!(plain, "Hello world with code and italic.");
}

#[test]
fn test_word_count() {
    assert_eq!(word_count("One two three"), 3);
    assert_eq!(word_count("**Bold** words count"), 3);
    assert_eq!(word_count("```\ncode\n```"), 1);
    assert_eq!(word_count(""), 0);
}

#[test]
fn test_char_count() {
    assert_eq!(char_count("Hello"), 5);
    assert_eq!(char_count("**H**"), 1); // Just 'H' without formatting
}

// ====================================================================================
// Integration Tests with Configurations
// ====================================================================================

#[test]
fn test_render_with_secure_config() {
    let input = "<script>alert('xss')</script>**bold**";
    let result = render_markdown(input, &MarkdownConfig::secure()).unwrap();

    assert!(result.has_unsafe_html);
    assert!(!result.html.contains("<script>"));
    assert!(result.html.contains("bold") || result.html.contains("<strong>"));
}

#[test]
fn test_render_with_test_config() {
    let input = "<b>HTML</b> **markdown**";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    // Test config allows HTML
    assert!(
        result.html.contains("<b>HTML</b>"),
        "Test config should allow safe HTML: {}",
        result.html
    );
}

#[test]
fn test_render_to_html_simple() {
    let html = render_to_html("**Bold** text");

    assert!(
        html.contains("<strong>Bold</strong>")
            || html.contains("<b>Bold</b>")
            || html.contains("<strong>"),
        "Simple render should work: {}",
        html
    );
}

// ====================================================================================
// Regression Tests
// ====================================================================================

#[test]
fn test_code_block_with_backticks_inside() {
    let input = r#"```rust
let s = "some `code` inside";
```"#;

    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert_eq!(result.code_blocks.len(), 1);
    assert!(
        result.code_blocks[0].1.contains("`code`"),
        "Should preserve backticks in code"
    );
}

#[test]
fn test_link_with_special_chars() {
    let input = r#"[Link](https://example.com?foo=bar&baz=qux)"#;
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    // Ampersands get escaped to &amp; in HTML
    assert!(
        result
            .html
            .contains("https://example.com?foo=bar&amp;baz=qux")
            || result.html.contains("https://example.com?foo=bar&baz=qux"),
        "Should preserve URL params: {}",
        result.html
    );
}

#[test]
fn test_code_block_language_detection() {
    let langs = vec![
        ("rs", "rs"),
        ("rust", "rust"),
        ("js", "js"),
        ("javascript", "javascript"),
        ("ts", "ts"),
        ("typescript", "typescript"),
        ("py", "py"),
        ("python", "python"),
        ("sh", "sh"),
        ("bash", "bash"),
        ("json", "json"),
        ("yaml", "yaml"),
        ("toml", "toml"),
        ("html", "html"),
        ("css", "css"),
        ("sql", "sql"),
    ];

    for (input_lang, expected) in langs {
        let code = format!("```{input_lang}\ncode\n```");
        let result = render_markdown(&code, &MarkdownConfig::test()).unwrap();

        assert_eq!(
            result.code_blocks[0].0,
            Some(expected.to_string()),
            "Should detect language: {input_lang}"
        );
    }
}

#[test]
fn test_markdown_horizontal_rule() {
    let inputs = vec!["---", "***", "___"];

    for input in inputs {
        let result = render_markdown(input, &MarkdownConfig::test()).unwrap();
        assert!(
            result.html.contains("<hr")
                || result.html.contains("<hr/>")
                || result.html.contains("<hr />"),
            "Should render horizontal rule for '{}': {}",
            input,
            result.html
        );
    }
}

#[test]
fn test_markdown_line_breaks() {
    let input = "Line 1  \nLine 2";
    let result = render_markdown(input, &MarkdownConfig::test()).unwrap();

    assert!(
        result.html.contains("<br")
            || result.html.contains("<br/>")
            || result.html.contains("<br />"),
        "Should render line breaks: {}",
        result.html
    );
}

// ====================================================================================
// Performance Test
// ====================================================================================

#[test]
fn test_render_performance() {
    use std::time::Instant;

    let input = MIXED_CONTENT.repeat(10); // 10x the mixed content

    let start = Instant::now();
    let result = render_markdown(&input, &MarkdownConfig::test()).unwrap();
    let duration = start.elapsed();

    assert!(!result.html.is_empty());
    assert!(
        duration.as_millis() < 1000,
        "Rendering should complete in under 1 second, took {:?}",
        duration
    );
}
