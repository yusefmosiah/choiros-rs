// Provider smoke tests - verify new LLM providers work
// Run with: npx baml test

function ProviderSmoke(question: string) -> string {
  client HunterAlpha
  prompt #"
    {{ _.role("user") }}
    Answer in one sentence: {{ question }}
  "#
}

function ProviderSmokeHealer(question: string) -> string {
  client HealerAlpha
  prompt #"
    {{ _.role("user") }}
    Answer in one sentence: {{ question }}
  "#
}

function ProviderSmokeMercury(question: string) -> string {
  client Mercury
  prompt #"
    Answer in one sentence: {{ question }}
  "#
}

test HunterAlphaSmoke {
  functions [ProviderSmoke]
  args {
    question "What is the capital of France?"
  }
}

test HealerAlphaSmoke {
  functions [ProviderSmokeHealer]
  args {
    question "What is the capital of France?"
  }
}

function ProviderSmokeNemotron(question: string) -> string {
  client Nemotron
  prompt #"
    Answer in one sentence: {{ question }}
  "#
}

test NemotronSmoke {
  functions [ProviderSmokeNemotron]
  args {
    question "What is the capital of France?"
  }
}

test MercurySmoke {
  functions [ProviderSmokeMercury]
  args {
    question "What is the capital of France?"
  }
}
