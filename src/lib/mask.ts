// Pure redaction helpers for locked mode. These never see the locked flag —
// callers decide when to apply them — so they stay trivially testable and can't
// accidentally leak the real value.

/** Fixed redaction glyph run. Deliberately constant-length so it leaks no hint
 *  about the original name's length or content. */
export const REDACTED = "•••••••••";

const SEP = " › ";

/** Mask a bare file name (or any single identifying label). */
export function maskName(_name: string): string {
  return REDACTED;
}

/** Mask a full path while keeping a path-shaped silhouette so cards still read
 *  as "a file somewhere", without exposing any folder or file name. */
export function maskPath(_path: string): string {
  return `•••${SEP}•••${SEP}${REDACTED}`;
}
