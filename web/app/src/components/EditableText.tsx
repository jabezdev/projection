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

/**
 * A text field bound to server data.
 *
 * While you type it shows your draft and saves after a short pause (and on
 * blur). When you're not typing it shows the live server value, so edits from
 * other people appear without overwriting what you're in the middle of.
 */
export function EditableText({ value, onSave, multiline, placeholder, className, readOnly }: Props) {
  const [draft, setDraft] = useState<string | null>(null);

  useEffect(() => {
    if (draft === null || draft === value) return;
    const timer = setTimeout(() => onSave(draft), SAVE_DELAY_MS);
    return () => clearTimeout(timer);
    // Only the draft drives saving; `value` catching up shouldn't re-save.
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
