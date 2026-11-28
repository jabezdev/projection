import { api } from "@magi/backend/api";
import { useQuery } from "convex/react";

export function NowShowing() {
  const now = useQuery(api.live.nowShowing);
  if (now === undefined) return <p className="text-muted">Loading…</p>;
  if (now === null) return <p className="text-muted">Nothing is on screen. This page follows PALTO during a service.</p>;
  return (
    <div className="space-y-6">
      <p className="text-sm text-muted">
        {now.serviceTitle}
        {now.itemTitle && ` · ${now.itemTitle}`}
        {now.section && ` · ${now.section}`}
      </p>
      <p className={`text-3xl leading-snug font-semibold whitespace-pre-line md:text-5xl ${now.blank ? "text-muted" : ""}`}>
        {now.blank ? "Screen is blank" : now.text || " "}
      </p>
      {now.next && (
        <div className="border-t border-line pt-4">
          <p className="mb-1 text-xs font-semibold tracking-wide text-muted uppercase">Next</p>
          <p className="text-xl whitespace-pre-line text-muted">{now.next}</p>
        </div>
      )}
    </div>
  );
}
