// ChoirOS Researcher BAML Contracts
// Agentic planner + synthesis authority for researcher loops.

enum ResearcherNextAction {
  Search
  FetchUrl
  Complete
  Block
}

enum ResearcherObjectiveStatus {
  Complete
  Incomplete
  Blocked
}

class ResearcherProviderCallSummary {
  provider string
  latency_ms int
  result_count int
  succeeded bool
  error string?
}

class ResearcherCitationInput {
  provider string
  title string
  url string
  snippet string
  published_at string?
  score float?
}

class ResearcherFetchedPageInput {
  url string
  status_code int
  content_excerpt string
  success bool
}

class ResearcherPlanInput {
  objective string
  current_query string
  round int
  max_rounds int
  provider_hint string?
  max_results_hint int?
  last_error string?
  provider_calls ResearcherProviderCallSummary[]
  citations ResearcherCitationInput[]
  fetched_pages ResearcherFetchedPageInput[]
}

class ResearcherPlanOutput {
  action ResearcherNextAction
  query string?
  provider string?
  fetch_url string?
  max_results int?
  time_range string?
  rationale string
  confidence float
  completion_reason string?
  recommended_next_capability string?
  recommended_next_objective string?
}

class ResearcherSynthesisInput {
  objective string
  query string
  provider_label string
  citations ResearcherCitationInput[]
  provider_calls ResearcherProviderCallSummary[]
  fetched_pages ResearcherFetchedPageInput[]
  raw_results_count int
  errors string[]
}

class ResearcherSynthesisOutput {
  summary string
  objective_status ResearcherObjectiveStatus
  completion_reason string
  recommended_next_capability string?
  recommended_next_objective string?
  key_findings string[]
  gaps string[]
  confidence float
}

function ResearcherPlanStep(input: ResearcherPlanInput) -> ResearcherPlanOutput {
  client FastResponse
  prompt #"
    You are the ChoirOS Researcher planner.
    Your job is to decide the next agentic step for this research run.

    Objective: {{ input.objective }}
    Current Query: {{ input.current_query }}
    Round: {{ input.round }} / {{ input.max_rounds }}
    Provider Hint: {{ input.provider_hint }}
    Max Results Hint: {{ input.max_results_hint }}
    Last Error: {{ input.last_error }}

    Previous Provider Calls:
    {{ input.provider_calls }}

    Current Citations:
    {{ input.citations }}

    Fetched Pages:
    {{ input.fetched_pages }}

    Decision contract:
    - Use action=Search when more evidence should be gathered.
    - Use action=FetchUrl when one or more specific URLs need deeper extraction.
    - Use action=Complete only when available evidence is sufficient for objective completion.
    - Use action=Block when the objective cannot progress without another capability/human help.
    - If action=Search: provide query and provider.
      provider can be: "auto", "tavily", "brave", "exa", or comma list like "tavily,exa".
    - If action=FetchUrl: provide fetch_url.
    - If action=Complete or action=Block: include completion_reason.
    - For broad objectives (for example "what's the news?"), favor multiple independent sources.
    - Never ask for deterministic scoring/ranking logic. Use semantic judgment.
    - Keep rationale concrete and short.

    {{ ctx.output_format }}
  "#
}

function ResearcherSummarizeEvidence(input: ResearcherSynthesisInput) -> ResearcherSynthesisOutput {
  client FastResponse
  prompt #"
    You are the ChoirOS Researcher synthesizer.
    Produce a semantic summary from gathered evidence and decide objective terminality.

    Objective: {{ input.objective }}
    Query: {{ input.query }}
    Provider Label: {{ input.provider_label }}
    Raw Results Count: {{ input.raw_results_count }}
    Errors: {{ input.errors }}

    Provider Calls:
    {{ input.provider_calls }}

    Citations:
    {{ input.citations }}

    Fetched Pages:
    {{ input.fetched_pages }}

    Output contract:
    - summary should be an evidence-based briefing, not a raw link list.
    - objective_status must be Complete, Incomplete, or Blocked.
    - completion_reason must justify objective_status.
    - key_findings should include the most meaningful semantic findings.
    - gaps should list unresolved questions or weak evidence areas.
    - recommended_next_capability/objective should be present if status is Incomplete or Blocked.
    - Do not rely on deterministic thresholds in wording; judge semantically.

    {{ ctx.output_format }}
  "#
}
