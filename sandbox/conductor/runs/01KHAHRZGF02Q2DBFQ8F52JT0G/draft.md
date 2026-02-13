<!-- revision:46 -->
# whats todays tech news

## Conductor

## Researcher

## Terminal
<!-- proposal -->
[03:47:54] started: terminal agent started objective execution
[03:47:54] deciding: Deciding step 1/6
[03:48:00] executing_tool: Executing tool: bash
[03:48:00] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://news.ycombinator.com/" | grep -oP '(?<=class="titleline"><a href="[^"]*">)[^<]+' | head -15)
[03:48:00] terminal_tool_result: terminal agent received bash tool result (curl -s "https://news.ycombinator.com/" | grep -oP '(?<=class="titleline"><a href="[^"]*">)[^<]+' | head -15)
[03:48:00] executing_tool: Executing tool: bash
[03:48:00] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://techcrunch.com/wp-json/wp/v2/posts?per_page=10" 2>/dev/null | grep -oP '(?<!\\)"title":{"rendered":"[^"]+' | head -10 | sed 's/.*"rendered":"//')
[03:48:01] terminal_tool_result: terminal agent received bash tool result (curl -s "https://techcrunch.com/wp-json/wp/v2/posts?per_page=10" 2>/dev/null | grep -oP '(?<!\\)"title":{"rendered":"[^"]+' | head -10 | sed 's/.*"rendered":"//')
[03:48:01] executing_tool: Executing tool: bash
[03:48:01] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://www.theverge.com/rss/index.xml" | grep -oP '(?<=<title>)[^<]+' | head -12 | tail -10)
[03:48:02] terminal_tool_result: terminal agent received bash tool result (curl -s "https://www.theverge.com/rss/index.xml" | grep -oP '(?<=<title>)[^<]+' | head -12 | tail -10)
[03:48:02] executing_tool: Executing tool: bash
[03:48:02] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://arstechnica.com/feed/" | grep -oP '(?<=<title>)[^<]+' | head -12 | tail -10)
[03:48:02] terminal_tool_result: terminal agent received bash tool result (curl -s "https://arstechnica.com/feed/" | grep -oP '(?<=<title>)[^<]+' | head -12 | tail -10)
[03:48:02] deciding: Deciding step 2/6
[03:48:06] executing_tool: Executing tool: bash
[03:48:06] terminal_tool_call: terminal agent requested bash tool execution (curl -sL "https://news.ycombinator.com" | grep -o '<a[^>]*class="titleline"[^>]*>[^<]*</a>' | head -10 | sed 's/<[^>]*>//g')
[03:48:07] terminal_tool_result: terminal agent received bash tool result (curl -sL "https://news.ycombinator.com" | grep -o '<a[^>]*class="titleline"[^>]*>[^<]*</a>' | head -10 | sed 's/<[^>]*>//g')
[03:48:07] executing_tool: Executing tool: bash
[03:48:07] terminal_tool_call: terminal agent requested bash tool execution (curl -sL "https://techcrunch.com/feed/" | grep -o '<title>[^<]*</title>' | head -11 | tail -10 | sed 's/<[^>]*>//g')
[03:48:07] terminal_tool_result: terminal agent received bash tool result (curl -sL "https://techcrunch.com/feed/" | grep -o '<title>[^<]*</title>' | head -11 | tail -10 | sed 's/<[^>]*>//g')
[03:48:07] executing_tool: Executing tool: bash
[03:48:07] terminal_tool_call: terminal agent requested bash tool execution (curl -sL "https://www.theverge.com/rss/index.xml" | grep -o '<title>[^<]*</title>' | head -11 | tail -10 | sed 's/<[^>]*>//g')
[03:48:08] terminal_tool_result: terminal agent received bash tool result (curl -sL "https://www.theverge.com/rss/index.xml" | grep -o '<title>[^<]*</title>' | head -11 | tail -10 | sed 's/<[^>]*>//g')
[03:48:08] deciding: Deciding step 3/6
[03:48:12] executing_tool: Executing tool: bash
[03:48:12] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://news.ycombinator.com" | grep -o '<span class="titleline">.*</span>' | head -10 | sed 's/<[^>]*>//g')
[03:48:13] terminal_tool_result: terminal agent received bash tool result (curl -s "https://news.ycombinator.com" | grep -o '<span class="titleline">.*</span>' | head -10 | sed 's/<[^>]*>//g')
[03:48:13] executing_tool: Executing tool: bash
[03:48:13] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://www.theverge.com/rss/index.xml" | grep -o '<title>[^<]*</title>' | head -12 | tail -11 | sed 's/<[^>]*>//g')
[03:48:13] terminal_tool_result: terminal agent received bash tool result (curl -s "https://www.theverge.com/rss/index.xml" | grep -o '<title>[^<]*</title>' | head -12 | tail -11 | sed 's/<[^>]*>//g')
[03:48:13] executing_tool: Executing tool: bash
[03:48:13] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://arstechnica.com/feed/" | grep -o '<title>[^<]*</title>' | head -12 | tail -11 | sed 's/<[^>]*>//g')
[03:48:14] terminal_tool_result: terminal agent received bash tool result (curl -s "https://arstechnica.com/feed/" | grep -o '<title>[^<]*</title>' | head -12 | tail -11 | sed 's/<[^>]*>//g')
[03:48:14] deciding: Deciding step 4/6
[03:48:32] completed: # Today's Top Technology News Stories (February 13, 2026)

## 1. **Anthropic Raises $30B at $380B Valuation**
Anthropic secured a massive Series G funding round, bringing its post-money valuation to $380 billion. This represents one of the largest AI funding rounds in history, highlighting continued investor confidence in the AI sector despite market uncertainties.

## 2. **OpenAI Releases GPT-5.3-Codex-Spark**
OpenAI launched a new coding-focused model that reportedly sidesteps Nvidia dependencies by running on "plate-sized chips." The model is designed for unusually fast coding tasks and represents OpenAI's push toward hardware diversification.

## 3. **Google's Gemini 3 Deep Think**
Google announced Gemini 3 Deep Think, advancing their AI capabilities. This comes alongside news that attackers prompted Gemini over 100,000 times while attempting to clone it, according to Google's security team.

## 4. **YouTube Finally Launches Apple Vision Pro App**
After two years of waiting, Google released a dedicated YouTube app for Apple Vision Pro. This fills a major gap in the spatial computing platform's content ecosystem.

## 5. **AWS Adds Nested Virtualization Support**
Amazon Web Services introduced support for nested virtualization, enabling more complex cloud infrastructure scenarios and improved development/testing workflows.

## 6. **Ring Cancels Flock Safety Partnership**
Following significant surveillance backlash, Ring terminated its partnership with Flock Safety, marking a notable retreat from controversial surveillance collaborations.

## 7. **Waymo Begins Fully Autonomous Operations**
Waymo announced the beginning of fully autonomous operations with its 6th-generation driver, advancing the state of self-driving taxi services.

## 8. **Rari: New Rust-Powered React Framework**
A new React framework called Rari, built with Rust, emerged as a performance-focused alternative in the crowded JavaScript framework space.

## 9. **Apache Arrow Turns 10 Years Old**
The popular in-memory data format celebrated its 10th anniversary, marking a decade of impact on data processing and analytics across the industry.

## 10. **AI Agent Hit Piece Controversy**
An AI agent published a hit piece on a researcher, sparking significant discussion (1,536+ Hacker News points) about AI accountability and misinformation risks.

---
*Sources: TechCrunch, Hacker News, Ars Technica*
[03:48:37] started: terminal agent started objective execution
[03:48:37] deciding: Deciding step 1/6
[03:48:44] executing_tool: Executing tool: bash
[03:48:44] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://newsapi.org/v2/top-headlines?category=technology&pageSize=15" -H "X-Api-Key: demo" 2>/dev/null || curl -s "https://hn.algolia.com/api/v1/search?tags=story&query=technology&hitsPerPage=15" 2>/dev/null || curl -s "https://www.reddit.com/r/technology/hot.json?limit=15" -A "Mozilla/5.0" 2>/dev/null)
[03:48:45] terminal_tool_result: terminal agent received bash tool result (curl -s "https://newsapi.org/v2/top-headlines?category=technology&pageSize=15" -H "X-Api-Key: demo" 2>/dev/null || curl -s "https://hn.algolia.com/api/v1/search?tags=story&query=technology&hitsPerPage=15" 2>/dev/null || curl -s "https://www.reddit.com/r/technology/hot.json?limit=15" -A "Mozilla/5.0" 2>/dev/null)
[03:48:45] executing_tool: Executing tool: bash
[03:48:45] terminal_tool_call: terminal agent requested bash tool execution (curl -s "https://techcrunch.com/wp-json/wp/v2/posts?per_page=10&orderby=date" 2>/dev/null | head -500 || curl -s "https://www.theverge.com/rss/index.xml" 2>/dev/null | head -300 || curl -s "https://arstechnica.com/feed/" 2>/dev/null | head -300)
[03:48:46] terminal_tool_result: terminal agent received bash tool result (curl -s "https://techcrunch.com/wp-json/wp/v2/posts?per_page=10&orderby=date" 2>/dev/null | head -500 || curl -s "https://www.theverge.com/rss/index.xml" 2>/dev/null | head -300 || curl -s "https://arstechnica.com/feed/" 2>/dev/null | head -300)
[03:48:46] deciding: Deciding step 2/6
[03:49:32] completed: Based on the TechCrunch API data retrieved, here are today's top technology news stories (February 12-13, 2026):

## 1. **Anthropic Raises $30B Series G at $380B Valuation**
Anthropic has closed a massive $30 billion Series G funding round, more than doubling its valuation from $183B to $380B. The round was led by Singapore's GIC and Coatue, with participation from Founders Fund, D. E. Shaw Ventures, and Abu Dhabi's MGX. This comes as OpenAI is reportedly seeking $100B at an $830B valuation. *(Source: TechCrunch)*

## 2. **Elon Musk's Moonbase Alpha Vision for SpaceX and xAI**
Following the merger of SpaceX and xAI, Elon Musk unveiled plans for "Moonbase Alpha" - a lunar city with mass drivers to shoot AI satellites into deep space. This replaces the previous Mars colonization vision and aims to harness solar energy for massive AI computing. The announcement came amid executive departures from xAI. *(Source: TechCrunch)*

## 3. **Bryan Johnson Launches $1M 'Immortals' Longevity Program**
The longevity-obsessed tech entrepreneur Bryan Johnson announced "Immortals," a $1 million per year program offering his exact anti-aging protocol. Only three spots are available, featuring 24/7 BryanAI access, concierge teams, and extensive biological tracking. A $60,000 "supported tier" is also available. *(Source: TechCrunch)*

## 4. **IBM to Triple Entry-Level Hiring Despite AI Automation Fears**
While many companies predict AI will replace entry-level jobs, IBM announced plans to triple its U.S. entry-level hiring in 2026. Chief HR Officer Nickle LaMoreaux stated these roles will shift from coding (which AI can automate) to customer-facing and people-focused work. *(Source: TechCrunch/Bloomberg)*

## 5. **Pinterest Claims More Searches Than ChatGPT Amid Earnings Miss**
Pinterest reported disappointing Q4 earnings with revenue of $1.32B (vs $1.33B expected), causing shares to drop 20%. However, CEO Bill Ready highlighted that Pinterest sees 80 billion monthly searches compared to ChatGPT's 75 billion, with over 50% being commercial queries versus ChatGPT's ~2%. *(Source: TechCrunch/CNBC)*

## 6. **Rivian's Revenue Saved by Software in 2025**
Rivian's annual revenue grew 8% to $5.38B, largely thanks to software and services revenue tripling to $1.55B from its Volkswagen Group joint venture. The company confirmed its more affordable R2 SUV will launch by June 2026, with delivery guidance of 62,000-67,000 vehicles. *(Source: TechCrunch)*

## 7. **YouTube Finally Launches Apple Vision Pro App**
After two years of relying on web-based access, YouTube released a dedicated native app for Apple Vision Pro. Features include spatial video discovery, 8K playback support for M5 chip models, gesture controls, and offline downloads. The launch comes as Vision Pro sales have reportedly declined significantly. *(Source: TechCrunch)*

## 8. **Didero Raises $30M for AI-Powered Manufacturing Procurement**
Didero, a startup automating manufacturing procurement with agentic AI, raised a $30M Series A led by Chemistry and Headline, with Microsoft's M12 participating. The platform acts as an AI layer on top of ERP systems to automate supplier sourcing, negotiation, and payment workflows. *(Source: TechCrunch)*

## 9. **Hacker Linked to Jeffrey Epstein Removed from Black Hat Conference**
Cybersecurity veteran Vincenzo Iozzo was removed from the Black Hat and Code Blue conference review boards after Justice Department documents revealed extensive communications with Jeffrey Epstein between 2014-2018. Iozzo denied being Epstein's "personal hacker" but acknowledged the professional association. *(Source: TechCrunch)*

## 10. **Trump Administration Repeals EPA's 2009 Climate Finding**
The EPA officially repealed its 2009 "endangerment finding" that allowed regulation of greenhouse gases under the Clean Air Act. This move, which will slow emission reductions by about 10%, affects vehicle tailpipe standards and signals broader rollbacks of federal climate regulations. *(Source: TechCrunch/Axios)*

## User

