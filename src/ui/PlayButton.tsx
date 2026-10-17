import { Play } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";
import { Button } from "./Button";

/** The primary Play action; its icon does a pogo bounce on hover and focus. */
export function PlayButton({
  className = "",
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <Button className={`bb-pogo ${className}`} {...props}>
      <Play className="bb-pogo-icon size-4" aria-hidden />
      {children}
    </Button>
  );
}
