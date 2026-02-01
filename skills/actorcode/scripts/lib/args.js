export function parseArgs(argv) {
  const args = [];
  const options = {};
  const rest = [];

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];

    if (token === "--") {
      rest.push(...argv.slice(i + 1));
      break;
    }

    if (token.startsWith("--")) {
      const [key, inlineValue] = token.slice(2).split("=");
      if (inlineValue !== undefined) {
        options[key] = inlineValue;
        continue;
      }

      const next = argv[i + 1];
      if (next && !next.startsWith("--")) {
        options[key] = next;
        i += 1;
      } else {
        options[key] = true;
      }

      continue;
    }

    args.push(token);
  }

  return { args, options, rest };
}
