import type { ReactNode } from "react";

/**
 * Lightweight inline markdown renderer — no dependencies.
 * Supports: **bold**, `inline code`, ```code blocks```, headings (#-###),
 * unordered lists (- / *), ordered lists (1.), and line breaks.
 */
export function SimpleMarkdown({ content }: { content: string }) {
  const blocks = splitBlocks(content);
  return (
    <>
      {blocks.map((block, i) => (
        <span key={i} className="contents">{renderBlock(block)}</span>
      ))}
    </>
  );
}

interface Block {
  type: "code" | "text";
  content: string;
  lang?: string;
}

function splitBlocks(text: string): Block[] {
  const blocks: Block[] = [];
  const codeBlockRe = /^```(\w*)\n([\s\S]*?)^```$/gm;
  let lastIndex = 0;
  let match;
  while ((match = codeBlockRe.exec(text)) !== null) {
    if (match.index > lastIndex) {
      blocks.push({ type: "text", content: text.slice(lastIndex, match.index) });
    }
    blocks.push({ type: "code", content: match[2], lang: match[1] || undefined });
    lastIndex = match.index + match[0].length;
  }
  if (lastIndex < text.length) {
    blocks.push({ type: "text", content: text.slice(lastIndex) });
  }
  return blocks;
}

function renderBlock(block: Block) {
  if (block.type === "code") {
    return (
      <pre className="my-1.5 p-2 rounded bg-muted text-xs font-mono overflow-x-auto">
        <code>{block.content.replace(/\n$/, "")}</code>
      </pre>
    );
  }

  const lines = block.content.split("\n");
  const elements: ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Headings
    const headingMatch = line.match(/^(#{1,3})\s+(.+)$/);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const cls = level === 1 ? "text-base font-bold mt-2 mb-1 break-words" : level === 2 ? "text-sm font-bold mt-1.5 mb-0.5 break-words" : "text-sm font-semibold mt-1 mb-0.5 break-words";
      elements.push(<div key={i} className={cls}>{renderInline(headingMatch[2])}</div>);
      i++;
      continue;
    }

    // Unordered list
    if (/^[\-\*]\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[\-\*]\s/.test(lines[i])) {
        items.push(lines[i].replace(/^[\-\*]\s+/, ""));
        i++;
      }
      elements.push(
        <ul key={`ul-${i}`} className="list-disc list-inside my-0.5 space-y-0.5 break-words">
          {items.map((item, j) => <li key={j}>{renderInline(item)}</li>)}
        </ul>
      );
      continue;
    }

    // Ordered list
    if (/^\d+\.\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\.\s/.test(lines[i])) {
        items.push(lines[i].replace(/^\d+\.\s+/, ""));
        i++;
      }
      elements.push(
        <ol key={`ol-${i}`} className="list-decimal list-inside my-0.5 space-y-0.5 break-words">
          {items.map((item, j) => <li key={j}>{renderInline(item)}</li>)}
        </ol>
      );
      continue;
    }

    // Empty line = paragraph break
    if (line.trim() === "") {
      elements.push(<div key={i} className="h-1" />);
      i++;
      continue;
    }

    // Regular text line
    elements.push(<div key={i} className="whitespace-pre-wrap break-words">{renderInline(line)}</div>);
    i++;
  }

  return <>{elements}</>;
}

function renderInline(text: string): ReactNode {
  // Split by inline patterns: **bold**, `code`
  const parts: ReactNode[] = [];
  const re = /(\*\*(.+?)\*\*|`([^`]+)`)/g;
  let lastIdx = 0;
  let match;
  let key = 0;

  while ((match = re.exec(text)) !== null) {
    if (match.index > lastIdx) {
      parts.push(text.slice(lastIdx, match.index));
    }
    if (match[2]) {
      // **bold**
      parts.push(<strong key={key++}>{match[2]}</strong>);
    } else if (match[3]) {
      // `code`
      parts.push(
        <code
          key={key++}
          className="px-1 py-0.5 rounded bg-muted text-xs font-mono whitespace-pre-wrap break-all"
        >
          {match[3]}
        </code>
      );
    }
    lastIdx = match.index + match[0].length;
  }
  if (lastIdx < text.length) {
    parts.push(text.slice(lastIdx));
  }
  return parts.length === 1 ? parts[0] : <>{parts}</>;
}
