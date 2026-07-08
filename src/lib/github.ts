/** Owner login from `owner/repo` full name. */
export function ownerFromFullName(fullName: string): string {
  const slash = fullName.indexOf("/");
  if (slash <= 0) {
    return fullName;
  }
  return fullName.slice(0, slash);
}

/** Public GitHub avatar URL (no API / no stored field needed). */
export function githubAvatarUrl(owner: string, size = 80): string {
  return `https://github.com/${encodeURIComponent(owner)}.png?size=${size}`;
}
