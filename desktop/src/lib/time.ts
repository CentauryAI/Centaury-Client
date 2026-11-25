// Small time helpers for org screens. The server mixes RFC 3339 ("…T…+00:00") and
// Postgres-style ("2026-11-25 19:23:56.21+00") stamps — normalise before parsing.
export function parseStamp(s: string | null | undefined): Date | null {
  if (!s) return null;
  let v = s.trim().replace(" ", "T");
  if (/[+-]\d\d$/.test(v)) v += ":00";
  const d = new Date(v);
  return Number.isNaN(d.getTime()) ? null : d;
}

export function ago(s: string | null | undefined): string {
  const d = parseStamp(s);
  if (!d) return "never";
  const sec = Math.max(0, Math.round((Date.now() - d.getTime()) / 1000));
  if (sec < 60) return "just now";
  const min = Math.round(sec / 60);
  if (min < 60) return `${min} min ago`;
  const h = Math.round(min / 60);
  if (h < 48) return `${h} h ago`;
  return `${Math.round(h / 24)} d ago`;
}

export function exact(s: string | null | undefined): string {
  const d = parseStamp(s);
  return d ? d.toLocaleString("en", { dateStyle: "medium", timeStyle: "short" }) : "";
}
