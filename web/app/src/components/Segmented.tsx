export function Segmented<T extends string>({ value, options, onChange }: { value: T; options: { value: T; label: string }[]; onChange: (value: T) => void }) {
  return (
    <div className="inline-flex rounded-lg border border-line bg-sunk p-0.5" role="tablist">
      {options.map((option) => (
        <button
          key={option.value}
          role="tab"
          aria-selected={value === option.value}
          className={`rounded-md px-3 py-1 font-label text-xs font-semibold tracking-[0.12em] uppercase ${
            value === option.value ? "bg-hover text-fg" : "text-faint hover:text-muted"
          }`}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
