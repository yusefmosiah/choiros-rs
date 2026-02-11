// ChoirOS Researcher BAML Contracts - DEPRECATED
//
// DEPRECATION NOTICE (2026-02-11):
// The ResearcherActor now uses the unified agent harness (agent_harness module)
// with the standard Decide function from agent.baml. These types and the
// ResearcherPlanStep function are kept for backward compatibility but are
// no longer used by the main researcher loop.
//
// The researcher now follows the standard pattern:
//   DECIDE (via PlanAction/Decide) -> EXECUTE tools -> loop or return
//
// Tool execution is handled by ResearcherAdapter which implements AgentAdapter.

// ============================================================================
// Status Enums (kept for backward compatibility)
// ============================================================================

enum ResearchStatus {
  Ongoing    // Still researching, gathering information
  Complete   // Satisfied with the answer
  Blocked    // Cannot proceed (insufficient info, access issues, etc.)
}

// ============================================================================
// Planning Types (kept for backward compatibility)
// ============================================================================

class ResearcherPlanInput {
  objective string
  current_query string
  round int
  max_rounds int
  working_draft_path string  // Path to the current working draft file
  last_error string?
}

class ResearcherPlanOutput {
  action ResearchAction        // What to do next
  query string?                // If searching
  provider string?             // Search provider hint
  url string?                  // If fetching
  file_path string?            // If reading/writing file
  content string?              // If writing to file
  old_text string?             // If editing file (text to replace)
  new_text string?             // If editing file (replacement text)
  reason string                // Why this action was chosen
  status ResearchStatus        // Current research status
}

enum ResearchAction {
  Search       // Perform web search
  FetchUrl     // Fetch specific URL
  FileRead     // Read local file
  FileWrite    // Write/create file
  FileEdit     // Edit existing file
  Complete     // Research is complete
  Block        // Cannot proceed
}

// ============================================================================
// Functions (DEPRECATED - use Decide from agent.baml instead)
// ============================================================================

function ResearcherPlanStep(input: ResearcherPlanInput) -> ResearcherPlanOutput {
  client FastResponse
  prompt #"
    DEPRECATED: This function is kept for backward compatibility.
    The ResearcherActor now uses the unified agent harness with Decide.

    You are the ChoirOS Researcher. Your job is to gather information and maintain a working draft document.

    Objective: {{ input.objective }}
    Current Query: {{ input.current_query }}
    Round: {{ input.round }} / {{ input.max_rounds }}
    Working Draft: {{ input.working_draft_path }}
    Last Error: {{ input.last_error }}

    Available Actions:
    - Search: Use web_search with query and optional provider (tavily, brave, exa, auto)
    - FetchUrl: Use fetch_url to retrieve content from a specific URL
    - FileRead: Read an existing file to reference previous research or context
    - FileWrite: Create or overwrite a file (usually your working draft)
    - FileEdit: Modify specific text in an existing file (preserves what you don't change)
    - Complete: Mark research as done when you have a satisfactory answer
    - Block: Mark as blocked if you cannot proceed

    Working Document Strategy:
    - Start by writing an initial draft with your plan
    - Update the draft as you discover new information
    - Use FileEdit to refine specific sections without rewriting everything
    - The draft should evolve from "plan" → "notes" → "findings" → "final answer"
    - Write in freeform markdown - no forced structure
    - Put the most important finding first (don't bury the lede)

    Guidelines:
    - Be concise in your writing - update the draft frequently
    - Cite sources inline as markdown links: [title](url)
    - If a search returns little, try a different query or provider
    - Read existing files if the objective references local content
    - When Complete, ensure the working draft contains your final answer

    {{ ctx.output_format }}
  "#
}
