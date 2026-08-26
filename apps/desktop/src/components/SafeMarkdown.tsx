import ReactMarkdown from "react-markdown";

export function SafeMarkdown({ children }: { children: string }) {
  return (
    <ReactMarkdown
      skipHtml
      components={{
        a: ({ children: label, href }) => (
          <span className="safe-link" title={href ? `External link: ${href}` : "External link"}>
            {label}
          </span>
        ),
      }}
    >
      {children}
    </ReactMarkdown>
  );
}
