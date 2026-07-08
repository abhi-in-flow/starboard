import { useState } from "react";
import { githubAvatarUrl, ownerFromFullName } from "@/lib/github";
import { cn } from "@/lib/utils";

type Props = {
  fullName: string;
  /** Pixel size of the square avatar. */
  size?: number;
  className?: string;
};

function initials(owner: string): string {
  const cleaned = owner.replace(/[^a-zA-Z0-9]/g, "");
  return (cleaned.slice(0, 2) || "?").toUpperCase();
}

export function RepoAvatar({ fullName, size = 40, className }: Props) {
  const owner = ownerFromFullName(fullName);
  const [failed, setFailed] = useState(false);

  if (failed || !owner) {
    return (
      <span
        className={cn(
          "inline-flex shrink-0 items-center justify-center rounded-lg bg-muted text-[11px] font-semibold text-muted-foreground",
          className,
        )}
        style={{ width: size, height: size }}
        aria-hidden
      >
        {initials(owner || fullName)}
      </span>
    );
  }

  return (
    <img
      src={githubAvatarUrl(owner, size * 2)}
      alt=""
      width={size}
      height={size}
      loading="lazy"
      decoding="async"
      onError={() => setFailed(true)}
      className={cn(
        "shrink-0 rounded-lg border border-border/60 bg-muted object-cover",
        className,
      )}
      style={{ width: size, height: size }}
    />
  );
}
