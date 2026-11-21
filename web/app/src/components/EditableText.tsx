import { useEffect, useState } from "react";

type Props = {
  value: string;
  onSave: (value: string) => void;
  multiline?: boolean;
  placeholder?: string;
  className?: string;
  readOnly?: boolean;
};

const SAVE_DELAY_MS = 700;

export function EditableText({ value, onSave, multiline, placeholder, className, readOnly }: Props) {
  const [draft, setDraft] = useState<string | null>(null);

  useEffect(() => {
    if (draft === null || draft === value) return;
    const timer = setTimeout(() => onSave(draft), SAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [draft]);

  const shared = {
    value: draft ?? value,
    placeholder,
    readOnly,
    className,
    onFocus: () => setDraft(value),
    onChange: (e: { target: { value: string } }) => setDraft(e.target.value),
    onBlur: () => {
      if (draft !== null && draft !== value) onSave(draft);
      setDraft(null);
    },
  };

  return multiline ? <textarea {...shared} /> : <input {...shared} />;
}
