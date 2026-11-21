import { api } from "@magi/backend/api";
import type { Id } from "@magi/backend/dataModel";
import { useMutation, useQuery } from "convex/react";
import { useDeferredValue, useState } from "react";
import { useSafe } from "../../lib/errors";

export function AddSong({ serviceId }: { serviceId: Id<"services"> }) {
  const safe = useSafe();
  const addSong = useMutation(api.services.addSong);
  const [search, setSearch] = useState("");
  const deferred = useDeferredValue(search.trim());
  const results = useQuery(api.songs.list, deferred ? { search: deferred } : "skip");

  return (
    <div className="card space-y-2">
      <input
        className="w-full"
        type="search"
        placeholder="Add a song from the library…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      {results?.map((song) => (
        <button
          key={song._id}
          className="flex w-full items-baseline gap-3 rounded-lg px-3 py-2 text-left hover:bg-hover"
          onClick={async () => {
            setSearch("");
            await safe(addSong)({ serviceId, songId: song._id });
          }}
        >
          <span>{song.title}</span>
          <span className="text-sm text-muted">{song.artist}</span>
        </button>
      ))}
      {results?.length === 0 && <p className="px-3 text-sm text-muted">No songs match.</p>}
    </div>
  );
}
