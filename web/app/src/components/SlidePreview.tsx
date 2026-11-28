import { splitSlides } from "@magi/shared";

export function SlidePreview({ lyrics, color }: { lyrics: string; color?: string }) {
  const slides = splitSlides(lyrics);
  if (slides.length === 0) return <p className="text-xs text-faint">No slides yet. Type the words; a blank line starts a new slide.</p>;
  return (
    <ol className="grid grid-cols-2 gap-1.5">
      {slides.map((slide, i) => (
        <li
          key={i}
          className="relative grid aspect-video place-items-center overflow-hidden rounded-md border border-line bg-sunk px-1.5 text-center text-[8px] leading-tight font-semibold whitespace-pre-line text-fg"
        >
          <span className="line-clamp-5">{slide}</span>
          <span className="absolute bottom-0.5 left-1 font-label text-[9px] text-faint" style={{ color }}>
            {i + 1}
          </span>
        </li>
      ))}
    </ol>
  );
}

export const slideCount = (lyrics: string) => splitSlides(lyrics).length;
