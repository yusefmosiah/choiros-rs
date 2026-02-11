use serde_json::Value;

use super::{ResearchCitation, ResearchProviderCall, ResearcherError, ResearcherFetchUrlResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchProvider {
    Tavily,
    Brave,
    Exa,
}

impl SearchProvider {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Tavily => "tavily",
            Self::Brave => "brave",
            Self::Exa => "exa",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderSelection {
    AutoSequential,
    Single(SearchProvider),
    Parallel(Vec<SearchProvider>),
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderSearchOutput {
    pub provider: SearchProvider,
    pub citations: Vec<ResearchCitation>,
    pub raw_results_count: usize,
    pub latency_ms: u64,
}

fn parse_provider_token(input: &str) -> Option<SearchProvider> {
    match input.trim().to_ascii_lowercase().as_str() {
        "tavily" => Some(SearchProvider::Tavily),
        "brave" => Some(SearchProvider::Brave),
        "exa" => Some(SearchProvider::Exa),
        _ => None,
    }
}

pub(crate) fn all_providers() -> Vec<SearchProvider> {
    vec![
        SearchProvider::Tavily,
        SearchProvider::Brave,
        SearchProvider::Exa,
    ]
}

pub(crate) fn parse_provider_selection(input: Option<&str>) -> ProviderSelection {
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return ProviderSelection::AutoSequential;
    };

    let lower = raw.to_ascii_lowercase();
    if lower == "auto" {
        return ProviderSelection::AutoSequential;
    }
    if lower == "all" || lower == "*" {
        return ProviderSelection::Parallel(all_providers());
    }
    if lower.contains(',') {
        let mut seen = std::collections::HashSet::<&'static str>::new();
        let mut providers = Vec::new();
        for token in lower.split(',') {
            if let Some(provider) = parse_provider_token(token) {
                let key = provider.as_str();
                if seen.insert(key) {
                    providers.push(provider);
                }
            }
        }
        return match providers.len() {
            0 => ProviderSelection::AutoSequential,
            1 => ProviderSelection::Single(providers[0]),
            _ => ProviderSelection::Parallel(providers),
        };
    }
    if let Some(single) = parse_provider_token(&lower) {
        ProviderSelection::Single(single)
    } else {
        ProviderSelection::AutoSequential
    }
}

fn map_time_range_to_brave(value: Option<&str>) -> Option<String> {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "day" || v == "d" => Some("pd".to_string()),
        Some(v) if v == "week" || v == "w" => Some("pw".to_string()),
        Some(v) if v == "month" || v == "m" => Some("pm".to_string()),
        Some(v) if v == "year" || v == "y" => Some("py".to_string()),
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

pub(crate) fn merge_citations(outputs: &[ProviderSearchOutput]) -> Vec<ResearchCitation> {
    let mut seen_urls = std::collections::HashSet::<String>::new();
    let mut merged = Vec::new();
    for output in outputs {
        for citation in &output.citations {
            if seen_urls.insert(citation.url.clone()) {
                merged.push(citation.clone());
            }
        }
    }
    merged
}

pub(crate) fn provider_label_from_outputs(outputs: &[ProviderSearchOutput]) -> Option<String> {
    if outputs.is_empty() {
        return None;
    }
    Some(
        outputs
            .iter()
            .map(|output| output.provider.as_str().to_string())
            .collect::<Vec<_>>()
            .join("->"),
    )
}

pub(crate) async fn fetch_url(
    request: &super::ResearcherFetchUrlRequest,
) -> Result<ResearcherFetchUrlResult, ResearcherError> {
    let url = request.url.trim();
    if url.is_empty() {
        return Err(ResearcherError::Validation(
            "fetch_url url cannot be empty".to_string(),
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ResearcherError::Validation(
            "fetch_url url must start with http:// or https://".to_string(),
        ));
    }

    let timeout_ms = request.timeout_ms.unwrap_or(30_000).clamp(3_000, 120_000);
    let max_chars = request.max_chars.unwrap_or(8_000).clamp(500, 64_000);
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| ResearcherError::ProviderRequest("http_client".to_string(), e.to_string()))?;

    let response = http
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "ChoirOS-Researcher/0.1 (+fetch_url)",
        )
        .send()
        .await
        .map_err(|e| ResearcherError::ProviderRequest("fetch_url".to_string(), e.to_string()))?;

    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(ToString::to_string);
    let body = response
        .text()
        .await
        .map_err(|e| ResearcherError::ProviderRequest("fetch_url".to_string(), e.to_string()))?;

    Ok(ResearcherFetchUrlResult {
        url: url.to_string(),
        final_url,
        status_code: status.as_u16(),
        content_type: content_type.clone(),
        content_excerpt: extract_text_excerpt(&body, content_type.as_deref(), max_chars),
        content_length: body.len(),
        success: status.is_success(),
    })
}

pub(crate) fn extract_text_excerpt(
    body: &str,
    content_type: Option<&str>,
    max_chars: usize,
) -> String {
    let looks_html = content_type
        .map(|ct| ct.to_ascii_lowercase().contains("html"))
        .unwrap_or_else(|| body.contains("<html") || body.contains("<body"));

    let normalized = if looks_html {
        let script_re = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>").ok();
        let style_re = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>").ok();
        let tag_re = regex::Regex::new(r"(?is)<[^>]+>").ok();

        let no_script = script_re
            .as_ref()
            .map(|re| re.replace_all(body, " ").to_string())
            .unwrap_or_else(|| body.to_string());
        let no_style = style_re
            .as_ref()
            .map(|re| re.replace_all(&no_script, " ").to_string())
            .unwrap_or(no_script);
        tag_re
            .as_ref()
            .map(|re| re.replace_all(&no_style, " ").to_string())
            .unwrap_or(no_style)
    } else {
        body.to_string()
    };

    normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

pub(crate) async fn run_provider_selection(
    http: &reqwest::Client,
    selection: ProviderSelection,
    query: &str,
    max_results: u32,
    time_range: Option<&str>,
    include_domains: Option<&[String]>,
    exclude_domains: Option<&[String]>,
) -> (
    Vec<ProviderSearchOutput>,
    Vec<ResearchProviderCall>,
    Vec<String>,
) {
    let (providers, run_in_parallel) = match selection {
        ProviderSelection::AutoSequential => (all_providers(), false),
        ProviderSelection::Single(provider) => (vec![provider], false),
        ProviderSelection::Parallel(list) => (list, true),
    };

    if run_in_parallel {
        let outcomes = futures_util::future::join_all(providers.iter().map(|provider| async {
            let provider = *provider;
            let started = tokio::time::Instant::now();
            let result = run_provider_search(
                provider,
                http,
                query,
                max_results,
                time_range,
                include_domains,
                exclude_domains,
            )
            .await;
            let elapsed = started.elapsed().as_millis() as u64;
            (provider, elapsed, result)
        }))
        .await;

        return collect_provider_outcomes(outcomes);
    }

    let mut outcomes = Vec::new();
    for provider in providers {
        let started = tokio::time::Instant::now();
        let result = run_provider_search(
            provider,
            http,
            query,
            max_results,
            time_range,
            include_domains,
            exclude_domains,
        )
        .await;
        let elapsed = started.elapsed().as_millis() as u64;
        outcomes.push((provider, elapsed, result));
    }
    collect_provider_outcomes(outcomes)
}

fn collect_provider_outcomes(
    outcomes: Vec<(
        SearchProvider,
        u64,
        Result<ProviderSearchOutput, ResearcherError>,
    )>,
) -> (
    Vec<ProviderSearchOutput>,
    Vec<ResearchProviderCall>,
    Vec<String>,
) {
    let mut outputs = Vec::new();
    let mut calls = Vec::new();
    let mut errors = Vec::new();

    for (provider, elapsed, result) in outcomes {
        match result {
            Ok(mut output) => {
                output.latency_ms = elapsed;
                calls.push(ResearchProviderCall {
                    provider: provider.as_str().to_string(),
                    latency_ms: elapsed,
                    result_count: output.citations.len(),
                    succeeded: true,
                    error: None,
                });
                outputs.push(output);
            }
            Err(err) => {
                let err_text = err.to_string();
                errors.push(format!("{}: {}", provider.as_str(), err_text));
                calls.push(ResearchProviderCall {
                    provider: provider.as_str().to_string(),
                    latency_ms: elapsed,
                    result_count: 0,
                    succeeded: false,
                    error: Some(err_text),
                });
            }
        }
    }

    (outputs, calls, errors)
}

async fn run_provider_search(
    provider: SearchProvider,
    http: &reqwest::Client,
    query: &str,
    max_results: u32,
    time_range: Option<&str>,
    include_domains: Option<&[String]>,
    exclude_domains: Option<&[String]>,
) -> Result<ProviderSearchOutput, ResearcherError> {
    match provider {
        SearchProvider::Tavily => {
            search_tavily(
                http,
                query,
                max_results,
                time_range,
                include_domains,
                exclude_domains,
            )
            .await
        }
        SearchProvider::Brave => search_brave(http, query, max_results, time_range).await,
        SearchProvider::Exa => {
            search_exa(http, query, max_results, include_domains, exclude_domains).await
        }
    }
}

async fn search_tavily(
    http: &reqwest::Client,
    query: &str,
    max_results: u32,
    time_range: Option<&str>,
    include_domains: Option<&[String]>,
    exclude_domains: Option<&[String]>,
) -> Result<ProviderSearchOutput, ResearcherError> {
    let api_key = std::env::var("TAVILY_API_KEY")
        .map_err(|_| ResearcherError::MissingApiKey("TAVILY_API_KEY".to_string()))?;

    let mut body = serde_json::json!({
        "query": query,
        "search_depth": "basic",
        "max_results": max_results,
        "include_answer": false,
        "include_raw_content": false
    });
    if let Some(time_range) = time_range {
        body["time_range"] = Value::String(time_range.to_string());
    }
    if let Some(include_domains) = include_domains {
        body["include_domains"] =
            serde_json::to_value(include_domains).unwrap_or_else(|_| Value::Array(Vec::new()));
    }
    if let Some(exclude_domains) = exclude_domains {
        body["exclude_domains"] =
            serde_json::to_value(exclude_domains).unwrap_or_else(|_| Value::Array(Vec::new()));
    }

    let response = http
        .post("https://api.tavily.com/search")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| ResearcherError::ProviderRequest("tavily".to_string(), e.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ResearcherError::ProviderRequest(
            "tavily".to_string(),
            format!("status {}: {}", status, body),
        ));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|e| ResearcherError::ProviderParse("tavily".to_string(), e.to_string()))?;
    let results = payload
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ResearcherError::ProviderParse(
                "tavily".to_string(),
                "missing results array".to_string(),
            )
        })?;

    let mut citations = Vec::new();
    for row in results {
        let url = row
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            continue;
        }
        citations.push(ResearchCitation {
            id: row
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(&url)
                .to_string(),
            provider: "tavily".to_string(),
            title: row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string(),
            url,
            snippet: row
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            published_at: row
                .get("published_date")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            score: row.get("score").and_then(|v| v.as_f64()),
        });
    }

    Ok(ProviderSearchOutput {
        provider: SearchProvider::Tavily,
        raw_results_count: citations.len(),
        citations,
        latency_ms: 0,
    })
}

async fn search_brave(
    http: &reqwest::Client,
    query: &str,
    max_results: u32,
    time_range: Option<&str>,
) -> Result<ProviderSearchOutput, ResearcherError> {
    let api_key = std::env::var("BRAVE_API_KEY")
        .map_err(|_| ResearcherError::MissingApiKey("BRAVE_API_KEY".to_string()))?;
    let mut req = http
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .query(&[("q", query), ("count", &max_results.to_string())]);
    if let Some(tf) = map_time_range_to_brave(time_range) {
        req = req.query(&[("freshness", tf)]);
    }

    let response = req
        .send()
        .await
        .map_err(|e| ResearcherError::ProviderRequest("brave".to_string(), e.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ResearcherError::ProviderRequest(
            "brave".to_string(),
            format!("status {}: {}", status, body),
        ));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|e| ResearcherError::ProviderParse("brave".to_string(), e.to_string()))?;
    let results = payload
        .get("web")
        .and_then(|v| v.get("results"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ResearcherError::ProviderParse(
                "brave".to_string(),
                "missing web.results array".to_string(),
            )
        })?;

    let mut citations = Vec::new();
    for row in results {
        let url = row
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            continue;
        }
        citations.push(ResearchCitation {
            id: url.clone(),
            provider: "brave".to_string(),
            title: row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string(),
            url,
            snippet: row
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            published_at: row
                .get("age")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            score: None,
        });
    }

    Ok(ProviderSearchOutput {
        provider: SearchProvider::Brave,
        raw_results_count: citations.len(),
        citations,
        latency_ms: 0,
    })
}

async fn search_exa(
    http: &reqwest::Client,
    query: &str,
    max_results: u32,
    include_domains: Option<&[String]>,
    exclude_domains: Option<&[String]>,
) -> Result<ProviderSearchOutput, ResearcherError> {
    let api_key = std::env::var("EXA_API_KEY")
        .map_err(|_| ResearcherError::MissingApiKey("EXA_API_KEY".to_string()))?;

    let mut body = serde_json::json!({
        "query": query,
        "numResults": max_results,
        "type": "auto",
        "contents": { "text": true }
    });
    if let Some(include_domains) = include_domains {
        body["includeDomains"] =
            serde_json::to_value(include_domains).unwrap_or_else(|_| Value::Array(Vec::new()));
    }
    if let Some(exclude_domains) = exclude_domains {
        body["excludeDomains"] =
            serde_json::to_value(exclude_domains).unwrap_or_else(|_| Value::Array(Vec::new()));
    }

    let response = http
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ResearcherError::ProviderRequest("exa".to_string(), e.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ResearcherError::ProviderRequest(
            "exa".to_string(),
            format!("status {}: {}", status, body),
        ));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|e| ResearcherError::ProviderParse("exa".to_string(), e.to_string()))?;
    let results = payload
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            ResearcherError::ProviderParse("exa".to_string(), "missing results array".to_string())
        })?;

    let mut citations = Vec::new();
    for row in results {
        let url = row
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            continue;
        }

        let snippet = row
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| {
                row.get("highlights")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
            })
            .unwrap_or_default()
            .to_string();

        let score = row.get("score").and_then(|v| v.as_f64()).or_else(|| {
            row.get("highlightScores")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_f64())
        });

        citations.push(ResearchCitation {
            id: row
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(&url)
                .to_string(),
            provider: "exa".to_string(),
            title: row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string(),
            url,
            snippet,
            published_at: row
                .get("publishedDate")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            score,
        });
    }

    Ok(ProviderSearchOutput {
        provider: SearchProvider::Exa,
        raw_results_count: citations.len(),
        citations,
        latency_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        extract_text_excerpt, parse_provider_selection, ProviderSelection, SearchProvider,
    };

    #[test]
    fn parse_provider_selection_defaults_to_auto() {
        assert_eq!(
            parse_provider_selection(None),
            ProviderSelection::AutoSequential
        );
        assert_eq!(
            parse_provider_selection(Some("")),
            ProviderSelection::AutoSequential
        );
        assert_eq!(
            parse_provider_selection(Some("unknown-provider")),
            ProviderSelection::AutoSequential
        );
    }

    #[test]
    fn parse_provider_selection_parallel_list() {
        assert_eq!(
            parse_provider_selection(Some("tavily,exa")),
            ProviderSelection::Parallel(vec![SearchProvider::Tavily, SearchProvider::Exa])
        );
    }

    #[test]
    fn extract_excerpt_strips_html() {
        let html =
            "<html><body><h1>Title</h1><script>ignored()</script><p>Hello world</p></body></html>";
        let excerpt = extract_text_excerpt(html, Some("text/html"), 24);
        assert!(!excerpt.contains('<'));
        assert!(excerpt.to_ascii_lowercase().contains("title"));
        assert!(excerpt.len() <= 24);
    }
}
