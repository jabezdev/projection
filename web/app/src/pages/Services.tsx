import { api } from "@magi/backend/api";
import { useMutation, useQuery } from "convex/react";
import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { useSafe } from "../lib/errors";
import { formatDate, nextSundayIso, todayIso, useMe } from "../lib/me";

export function Services() {
  const me = useMe();
  const services = useQuery(api.services.list);
  const today = todayIso();
  const upcoming = (services ?? []).filter((s) => s.date >= today).reverse();
  const past = (services ?? []).filter((s) => s.date < today);

  return (
    <div className="space-y-6">
      {me.can.arrange && <NewService />}

      {services === undefined ? (
        <p className="text-muted">Loading…</p>
      ) : (
        <>
          <ServiceList title="Upcoming" services={upcoming} empty="Nothing planned yet." />
          {past.length > 0 && <ServiceList title="Past" services={past} />}
        </>
      )}
    </div>
  );
}

function NewService() {
  const safe = useSafe();
  const navigate = useNavigate();
  const create = useMutation(api.services.create);
  const [date, setDate] = useState(nextSundayIso);
  const [title, setTitle] = useState("Sunday Service");

  return (
    <form
      className="card flex flex-wrap items-center gap-2"
      onSubmit={async (e) => {
        e.preventDefault();
        const id = await safe(create)({ date, title });
        if (id) navigate(`/services/${id}`);
      }}
    >
      <input type="date" value={date} onChange={(e) => setDate(e.target.value)} required />
      <input className="min-w-0 flex-1" value={title} onChange={(e) => setTitle(e.target.value)} />
      <button className="btn-primary">Plan service</button>
    </form>
  );
}

function ServiceList({
  title,
  services,
  empty,
}: {
  title: string;
  services: { _id: string; date: string; title: string }[];
  empty?: string;
}) {
  return (
    <section className="space-y-2">
      <h2 className="text-sm font-semibold text-muted">{title}</h2>
      {services.length === 0 ? (
        <p className="text-sm text-muted">{empty}</p>
      ) : (
        <ul className="divide-y divide-line/60 overflow-hidden rounded-xl border border-line/60 bg-card">
          {services.map((s) => (
            <li key={s._id}>
              <Link to={`/services/${s._id}`} className="flex gap-3 px-4 py-3 hover:bg-hover">
                <span className="w-28 shrink-0 text-muted">{formatDate(s.date)}</span>
                <span className="font-medium">{s.title}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
