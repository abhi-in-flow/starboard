import { openUrl } from "@tauri-apps/plugin-opener";
import type { Components } from "react-markdown";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { prepareReadmeExcerpt } from "@/lib/readme";
import { safeMarkdownUrl } from "@/lib/safeUrl";

const components: Components = {
  a: ({ href, children }) => {
    const text = String(children ?? "").trim();
    const safe = safeMarkdownUrl(href);
    // Skip empty badge/shield links left after image stripping.
    if (!text && !safe) {
      return null;
    }
    if (!safe) {
      return <span>{children ?? href}</span>;
    }
    return (
      <button
        type="button"
        className="text-primary underline underline-offset-2 hover:opacity-80"
        onClick={() => {
          void openUrl(safe);
        }}
      >
        {children ?? safe}
      </button>
    );
  },
  img: ({ src, alt }) => {
    const safe = safeMarkdownUrl(src);
    return safe ? (
      <img
        src={safe}
        alt={alt ?? ""}
        className="my-2 max-h-40 max-w-full rounded-md border border-border object-contain"
        loading="lazy"
      />
    ) : null;
  },
  code: ({ className, children, ...props }) => {
    const isBlock = Boolean(className?.includes("language-"));
    if (isBlock) {
      return (
        <code
          className="block overflow-x-auto rounded-md bg-muted px-3 py-2 text-[11px] leading-relaxed"
          {...props}
        >
          {children}
        </code>
      );
    }
    return (
      <code
        className="rounded bg-muted px-1 py-0.5 font-mono text-[11px]"
        {...props}
      >
        {children}
      </code>
    );
  },
  pre: ({ children }) => <pre className="my-2 overflow-x-auto">{children}</pre>,
  ul: ({ children }) => (
    <ul className="my-2 list-disc space-y-1 pl-5">{children}</ul>
  ),
  ol: ({ children }) => (
    <ol className="my-2 list-decimal space-y-1 pl-5">{children}</ol>
  ),
  h1: ({ children }) => (
    <h1 className="mb-2 mt-3 text-base font-semibold">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-2 mt-3 text-sm font-semibold">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-1.5 mt-2 text-sm font-medium">{children}</h3>
  ),
  p: ({ children }) => <p className="my-2 leading-relaxed">{children}</p>,
  strong: ({ children }) => (
    <strong className="font-semibold">{children}</strong>
  ),
  em: ({ children }) => <em className="italic">{children}</em>,
  blockquote: ({ children }) => (
    <blockquote className="my-2 border-l-2 border-border pl-3 text-muted-foreground">
      {children}
    </blockquote>
  ),
  table: ({ children }) => (
    <div className="my-2 overflow-x-auto">
      <table className="w-full border-collapse text-left text-[11px]">
        {children}
      </table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-border bg-muted/60 px-2 py-1 font-medium">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-border px-2 py-1">{children}</td>
  ),
  hr: () => <hr className="my-3 border-border" />,
};

type Props = {
  markdown: string;
};

export function MarkdownExcerpt({ markdown }: Props) {
  const cleaned = prepareReadmeExcerpt(markdown);
  if (!cleaned.trim()) {
    return (
      <p className="text-sm text-muted-foreground">
        README is mostly badges/HTML chrome — no prose excerpt.
      </p>
    );
  }
  return (
    <div className="max-h-72 overflow-auto rounded-md border border-border bg-muted/30 p-3 text-xs text-foreground [&_a]:text-primary">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {cleaned}
      </ReactMarkdown>
    </div>
  );
}

export default MarkdownExcerpt;
