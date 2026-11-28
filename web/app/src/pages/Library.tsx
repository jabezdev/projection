import { api } from "@magi/backend/api";
import { useQuery } from "convex/react";
import { useDeferredValue, useState } from "react";
import { Link } from "react-router";
import { useMe } from "../lib/me";

export function Library() {
  const me = useMe();
  const [search, setSearch] = useState("");
  const deferred = useDeferredValue(search);
  const songs = useQuery(api.songs.list, { search: deferred || undefined });

  return (
    <div className="space-y-4">
      <div className="flex gap-2">
        <input
          className="flex-1"
          type="search"
          placeholder="Search titles, authors, lyrics…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {me.can.editLibrary && (
          <Link to="/library/new" className="btn-primary">
            New song
          </Link>
        )}
      </div>

      {songs === undefined ? (
        <p className="text-muted">Loading…</p>
      ) : songs.length === 0 ? (
        <p className="text-muted">{search ? "No songs match." : "The library is empty."}</p>
      ) : (
        <ul className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-card">
          {songs.map((song) => (
            <li key={song.id}>
              <Link to={`/library/${song.id}`} className="flex items-center gap-3 px-4 py-3 hover:bg-hover">
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{song.title}</span>
                  <span className="block truncate text-sm text-faint">{song.credits.authors?.map((a) => a.name).join(", ")}</span>
                </span>
                {song.key && <span className="rounded-md bg-hover px-2 py-0.5 font-label text-sm font-semibold text-muted">{song.key}</span>}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
