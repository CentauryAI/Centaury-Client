
import { useState } from "react";
import { Eye, EyeOff, type LucideIcon } from "lucide-react";
import { cn } from "../lib/cn";

const INPUT =
  "w-full rounded-xl border border-border px-4 py-2.5 text-sm outline-none transition-colors placeholder:text-muted-foreground/70 focus:border-primary";

/* Two pills (Login / Sign Up): the active one is white with a border and shadow. */
export function ModeToggle<T extends string>({
  options,
  value,
  onChange,
}: {
  options: readonly { key: T; label: string; icon?: LucideIcon }[];
  value: T;
  onChange: (key: T) => void;
}) {
  return (
    <div className="flex justify-center gap-2">
      {options.map(({ key, label, icon: Icon }) => (
        <button
          key={key}
          type="button"
          onClick={() => onChange(key)}
          aria-pressed={value === key}
          className={cn(
            "flex items-center gap-1.5 rounded-full px-4 py-1.5 text-sm font-medium transition-all",
            value === key
              ? "border border-border bg-card text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          {Icon && <Icon size={14} />}
          {label}
        </button>
      ))}
    </div>
  );
}

/* Labeled field; `hint` goes to the right of the label ("Forgot password?"). */
export function AuthField({
  label,
  hint,
  className,
  ...props
}: {
  label: string;
  hint?: React.ReactNode;
} & React.ComponentPropsWithoutRef<"input">) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="flex items-center justify-between text-sm">
        <span className="font-medium">{label}</span>
        {hint}
      </span>
      <input className={cn(INPUT, className)} {...props} />
    </label>
  );
}

/* Password field with the eye toggle inside. */
export function PasswordField({
  label,
  hint,
  className,
  showLabel = "Show password",
  hideLabel = "Hide password",
  ...props
}: {
  label: string;
  hint?: React.ReactNode;
  showLabel?: string;
  hideLabel?: string;
} & Omit<React.ComponentPropsWithoutRef<"input">, "type">) {
  const [show, setShow] = useState(false);
  return (
    <label className="flex flex-col gap-1.5">
      <span className="flex items-center justify-between text-sm">
        <span className="font-medium">{label}</span>
        {hint}
      </span>
      <span className="relative">
        <input type={show ? "text" : "password"} className={cn(INPUT, "pr-10", className)} {...props} />
        <button
          type="button"
          onClick={() => setShow((s) => !s)}
          aria-label={show ? hideLabel : showLabel}
          className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
        >
          {show ? <EyeOff size={15} /> : <Eye size={15} />}
        </button>
      </span>
    </label>
  );
}

/* Auth CTA: dark slate gradient, rounded-xl. */
export function AuthSubmit({
  className,
  ...props
}: React.ComponentPropsWithoutRef<"button">) {
  return (
    <button
      type="submit"
      className={cn(
        "mt-1 rounded-xl bg-[linear-gradient(180deg,var(--band-lit),var(--band))] py-2.5 text-sm font-medium text-white transition-opacity dark:bg-none dark:bg-foreground dark:text-background hover:opacity-90 disabled:opacity-50",
        className
      )}
      {...props}
    />
  );
}

/* Hairlines + uppercase mono label. */
export function AuthDivider({ label = "or" }: { label?: string }) {
  return (
    <div className="my-6 flex items-center gap-3">
      <span className="h-px flex-1 bg-border" />
      <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </span>
      <span className="h-px flex-1 bg-border" />
    </div>
  );
}

/* Outline social button; lucide ships no brand icons, use <GoogleIcon>/<GithubIcon>. */
export function SocialButton({
  icon,
  className,
  children,
  ...props
}: { icon: React.ReactNode } & React.ComponentPropsWithoutRef<"button">) {
  return (
    <button
      type="button"
      className={cn(
        "flex items-center justify-center gap-2 rounded-xl border border-border py-2.5 text-sm font-medium text-foreground/90 transition-colors hover:bg-muted",
        className
      )}
      {...props}
    >
      {icon}
      {children}
    </button>
  );
}

/* Notice below the form: neutral `info`, amber `warn` (validation error). */
export function AuthNotice({
  tone = "info",
  children,
}: {
  tone?: "info" | "warn";
  children: React.ReactNode;
}) {
  return (
    <p
      role={tone === "warn" ? "alert" : undefined}
      className={cn(
        "rounded-xl px-3 py-2 text-xs",
        tone === "warn"
          ? "bg-status-pending/10 text-[#92600a]"
          : "bg-muted text-center text-muted-foreground"
      )}
    >
      {children}
    </p>
  );
}

export function GoogleIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-4 w-4" aria-hidden>
      <path
        fill="currentColor"
        d="M21.35 11.1H12v2.9h5.35c-.5 2.5-2.6 4.3-5.35 4.3a5.8 5.8 0 1 1 0-11.6c1.48 0 2.82.55 3.85 1.45l2.15-2.15A8.8 8.8 0 1 0 12 20.8c5.08 0 8.7-3.57 8.7-8.6 0-.37-.03-.73-.09-1.1Z"
      />
    </svg>
  );
}

export function GithubIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-4 w-4" aria-hidden>
      <path
        fill="currentColor"
        d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.7c-2.78.6-3.37-1.34-3.37-1.34-.45-1.16-1.11-1.47-1.11-1.47-.9-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.9 1.52 2.34 1.08 2.91.83.09-.65.35-1.09.63-1.34-2.22-.25-4.56-1.11-4.56-4.94 0-1.1.39-1.99 1.03-2.69-.1-.25-.45-1.27.1-2.64 0 0 .84-.27 2.75 1.02a9.6 9.6 0 0 1 5 0c1.91-1.3 2.75-1.02 2.75-1.02.55 1.37.2 2.39.1 2.64.64.7 1.03 1.6 1.03 2.69 0 3.84-2.34 4.68-4.57 4.93.36.31.68.92.68 1.85V21c0 .27.18.58.69.48A10 10 0 0 0 12 2Z"
      />
    </svg>
  );
}
