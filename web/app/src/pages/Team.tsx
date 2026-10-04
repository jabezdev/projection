import { api } from "@magi/backend/api";
import type { Doc } from "@magi/backend/dataModel";
import { useMutation, useQuery } from "convex/react";
import { useState } from "react";
import { useSafe } from "../lib/errors";
import { useMe } from "../lib/me";

type Role = Doc<"users">["role"];

const ROLES: { role: Role; description: string }[] = [
  { role: "admin", description: "Everything, including roles" },
  { role: "librarian", description: "Maintains the song library" },
  { role: "leader", description: "Edits the library, arranges services" },
  { role: "operator", description: "Arranges services, runs projection" },
  { role: "singer", description: "Reads lyrics and arrangements" },
];

export function Team() {
  const me = useMe();
  const safe = useSafe();
  const users = useQuery(api.users.list);
  const setRole = useMutation(api.users.setRole);

  return (
    <div className="space-y-6">
      {me.can.pairDevice && <PairDevice />}

      <section className="space-y-2">
        <h2 className="text-sm font-semibold text-muted">Team</h2>
        <ul className="divide-y divide-line/60 rounded-xl border border-line/60 bg-card">
          {users?.map((user) => (
            <li key={user._id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="truncate">{user.name}</div>
                <div className="truncate text-xs text-muted">{user.email}</div>
              </div>
              {me.can.manageTeam ? (
                <select value={user.role} onChange={(e) => safe(setRole)({ userId: user._id, role: e.target.value as Role })}>
                  {ROLES.map(({ role }) => (
                    <option key={role} value={role}>
                      {role}
                    </option>
                  ))}
                </select>
              ) : (
                <span className="text-sm text-muted capitalize">{user.role}</span>
              )}
            </li>
          ))}
        </ul>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs text-muted">
          {ROLES.map(({ role, description }) => (
            <div key={role} className="contents">
              <dt className="capitalize">{role}</dt>
              <dd>{description}</dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}

function PairDevice() {
  const safe = useSafe();
  const createCode = useMutation(api.devices.createPairingCode);
  const revoke = useMutation(api.devices.revoke);
  const devices = useQuery(api.devices.listMine);
  const [pairing, setPairing] = useState<{ code: string; expiresAt: number } | null>(null);

  return (
    <section className="card space-y-3">
      <h2 className="font-semibold">Projection computers</h2>
      <p className="text-sm text-muted">
        To connect the projection app, get a code here and type it into the app. The computer then works as you
        until you remove it.
      </p>
      {pairing ? (
        <div>
          <div className="font-mono text-3xl tracking-widest">{pairing.code}</div>
          <div className="text-xs text-muted">
            Valid until {new Date(pairing.expiresAt).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
          </div>
        </div>
      ) : (
        <button className="btn-primary" onClick={async () => setPairing((await safe(createCode)()) ?? null)}>
          Get pairing code
        </button>
      )}
      {devices && devices.length > 0 && (
        <ul className="space-y-1">
          {devices.map((d) => (
            <li key={d._id} className="flex items-center gap-2 text-sm">
              <span className="flex-1">{d.name}</span>
              <span className="text-xs text-muted">paired {new Date(d.pairedAt).toLocaleDateString()}</span>
              <button className="btn" onClick={() => confirm(`Remove ${d.name}?`) && safe(revoke)({ deviceId: d._id })}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
