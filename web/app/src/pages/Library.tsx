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
          placeholder="Search titles, artists, lyrics…"
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
        <ul className="divide-y divide-line/60 overflow-hidden rounded-xl border border-line/60 bg-card">
          {songs.map((song) => (
            <li key={song._id}>
              <Link to={`/library/${song._id}`} className="flex items-baseline gap-3 px-4 py-3 hover:bg-hover">
                <span className="font-medium">{song.title}</span>
                <span className="truncate text-sm text-muted">{song.artist}</span>
                {song.defaultKey && <span className="ml-auto text-xs text-muted">{song.defaultKey}</span>}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
