/** A stand-in for the router's Link: an anchor with the path, search and hash it leads to. */
export function TestLink({
  to,
  params,
  search,
  hash,
  children,
  className,
  title,
}: {
  to: string;
  params?: Record<string, string>;
  search?: Record<string, string>;
  hash?: string;
  children: React.ReactNode;
  className?: string;
  title?: string;
}) {
  const path = Object.entries(params ?? {}).reduce((p, [k, v]) => p.replace(`$${k}`, v), to);
  const query = search ? `?${new URLSearchParams(search)}` : "";
  return (
    <a href={`${path}${query}${hash ? `#${hash}` : ""}`} className={className} title={title}>
      {children}
    </a>
  );
}
