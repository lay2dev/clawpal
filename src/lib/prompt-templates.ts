import doctorStartMarkdown from "../../prompts/frontend/doctor-start.md?raw";
import installHubFallbackMarkdown from "../../prompts/frontend/install-hub-fallback.md?raw";

function extractPromptBlock(markdown: string): string {
  const marker = "```prompt";
  const start = markdown.indexOf(marker);
  if (start < 0) return markdown.trim();
  const bodyStart = start + marker.length;
  const rest = markdown.slice(bodyStart);
  const end = rest.indexOf("```");
  if (end < 0) return rest.trim();
  return rest.slice(0, end).trim();
}

export function renderPromptTemplate(template: string, vars: Record<string, string>): string {
  let output = template;
  for (const [key, value] of Object.entries(vars)) {
    output = output.split(key).join(value);
  }
  return output;
}

export function doctorStartPromptTemplate(): string {
  return extractPromptBlock(doctorStartMarkdown);
}

export function installHubFallbackPromptTemplate(): string {
  return extractPromptBlock(installHubFallbackMarkdown);
}
