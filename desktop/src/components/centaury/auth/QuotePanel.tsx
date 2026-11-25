
import { useState } from "react";
import { ArrowLeft, ArrowRight } from "lucide-react";

export type Quote = { text: string; name: string; role?: string; org?: string };

const ARROW =
  "flex h-9 w-9 items-center justify-center rounded-full border border-border bg-card/70 text-foreground/70 transition-colors hover:text-foreground";

/*
 * Right-hand login panel: a background texture (`texture`, usually
 * <AsciiArt>) and, anchored at the bottom, the glass quote card with two
 * arrows that step through `quotes`. Quotes describe the product or come from
 * real people with permission: never invented testimonials.
 */
export default function QuotePanel({
  quotes,
  texture,
  prevLabel = "Previous",
  nextLabel = "Next",
}: {
  quotes: readonly Quote[];
  texture?: React.ReactNode;
  prevLabel?: string;
  nextLabel?: string;
}) {
  const [index, setIndex] = useState(0);
  const q = quotes[index];
  const step = (d: number) => setIndex((index + quotes.length + d) % quotes.length);

  return (
    <div className="relative hidden min-h-[520px] overflow-hidden rounded-2xl border border-border lg:block">
      {texture}
      <div className="absolute inset-x-6 bottom-6 rounded-2xl border border-border bg-card/65 p-6 backdrop-blur-md">
        <p className="text-[15px] leading-relaxed text-foreground">{q.text}</p>
        <div className="mt-5 flex items-end justify-between">
          <div>
            <p className="font-heading text-lg font-semibold">{q.name}</p>
            {q.role && <p className="mt-0.5 text-sm text-muted-foreground">{q.role}</p>}
            {q.org && <p className="text-sm text-muted-foreground">{q.org}</p>}
          </div>
          {quotes.length > 1 && (
            <div className="flex gap-2">
              <button type="button" aria-label={prevLabel} onClick={() => step(-1)} className={ARROW}>
                <ArrowLeft size={15} />
              </button>
              <button type="button" aria-label={nextLabel} onClick={() => step(1)} className={ARROW}>
                <ArrowRight size={15} />
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
