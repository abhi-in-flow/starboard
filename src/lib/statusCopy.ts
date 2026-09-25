export function ollamaAvailabilityLabel(
  available: boolean | null | undefined,
  configured?: boolean,
): string {
  if (available === true) {
    return "Ollama is reachable";
  }
  if (available === false) {
    return configured === false
      ? "Ollama is not configured"
      : "Ollama is unreachable";
  }
  return configured === false ? "Ollama is not configured" : "Checking Ollama…";
}

export function ollamaShortLabel(
  available: boolean | null | undefined,
  configured?: boolean,
): string {
  if (available === true) {
    return "Reachable";
  }
  if (available === false) {
    return configured === false ? "Not configured" : "Unreachable";
  }
  return configured === false ? "Not configured" : "Checking";
}

export function githubConnectionLabel(
  connected: boolean | undefined,
  username?: string | null,
): string {
  if (connected === true) {
    return username ? `Connected as ${username}` : "Connected";
  }
  if (connected === false) {
    return "Not connected";
  }
  return "Checking GitHub…";
}
