import { api } from "@magi/backend/api";
import type { FunctionReturnType } from "convex/server";
import { useMutation } from "convex/react";
import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { EditableText } from "../../components/EditableText";
import { useSafe } from "../../lib/errors";

type Song = NonNullable<FunctionReturnType<typeof api.songs.get>>;

export function Credits({ song, editable }: { song: Song; editable: boolean }) {
  const safe = useSafe();
  const update = useMutation(api.songs.update);
  const [open, setOpen] = useState(false);
  const songId = song.id;
  const credits = song.credits;
  const setCredits = (patch: Partial<typeof credits>) => safe(update)({ songId, credits: { ...credits, ...patch } });
  const text = (value: string) => value.trim() || undefined;
  const authors = credits.authors?.map((a) => a.name).join(", ") ?? "";
  const field = "w-full";

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-[1fr_6rem] gap-2 sm:grid-cols-[1fr_7rem_auto]">
        <Field label="Authors">
          <EditableText
            className={field}
            placeholder="Comma separated"
            value={authors}
            readOnly={!editable}
            onSave={(value) => {
              const names = value.split(",").map((n) => n.trim()).filter(Boolean);
              return setCredits({ authors: names.length ? names.map((name) => credits.authors?.find((a) => a.name === name) ?? { name }) : undefined });
            }}
          />
        </Field>
        <Field label="Key">
          <EditableText className={field} placeholder="G" value={song.key ?? ""} readOnly={!editable} onSave={(key) => safe(update)({ songId, key })} />
        </Field>
        <button className="btn col-span-2 self-end text-muted sm:col-span-1" aria-expanded={open} onClick={() => setOpen(!open)}>
          Copyright and CCLI
          <ChevronDown size={15} className={`transition-transform ${open ? "rotate-180" : ""}`} />
        </button>
      </div>
      {open && (
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Field label="CCLI number">
            <EditableText className={field} value={credits.ccli ?? ""} readOnly={!editable} onSave={(ccli) => setCredits({ ccli: text(ccli) })} />
          </Field>
          <Field label="Copyright">
            <EditableText className={field} value={credits.copyright ?? ""} readOnly={!editable} onSave={(copyright) => setCredits({ copyright: text(copyright) })} />
          </Field>
          <Field label="Publisher">
            <EditableText className={field} value={credits.publisher ?? ""} readOnly={!editable} onSave={(publisher) => setCredits({ publisher: text(publisher) })} />
          </Field>
          <Field label="Year">
            <EditableText
              className={field}
              value={credits.year ? String(credits.year) : ""}
              readOnly={!editable}
              onSave={(year) => setCredits({ year: Number.isInteger(Number(year)) && year.trim() ? Number(year) : undefined })}
            />
          </Field>
        </div>
      )}
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="eyebrow text-[10px] text-faint">{label}</span>
      {children}
    </label>
  );
}
