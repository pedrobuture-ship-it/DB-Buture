import type { ButtonHTMLAttributes, ReactNode } from "react";
import type { Evidence, Severity } from "@/types/schema";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export function Button({
  variant = "secondary",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  const styles: Record<ButtonVariant, string> = {
    primary: "bg-cyan text-ink-0 hover:bg-cyan/90 border-cyan font-medium",
    secondary: "bg-ink-3 text-fg hover:bg-ink-4 border-line-strong",
    ghost: "bg-transparent text-fg-muted hover:text-fg hover:bg-ink-3 border-transparent",
    danger: "bg-red-wash text-red hover:bg-red/20 border-red/40",
  };
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "inline-flex h-7 items-center justify-center gap-1.5 rounded-sm border px-3 text-[12px] transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        styles[variant],
        className,
      )}
    />
  );
}

/** Small uppercase technical label. */
export function Label({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cx("font-mono text-[10px] uppercase tracking-[0.12em] text-fg-dim", className)}
    >
      {children}
    </span>
  );
}

export function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={cx("font-mono text-[12px]", className)}>{children}</span>;
}

export function Panel({
  title,
  action,
  children,
  className,
}: {
  title?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cx("rounded-md border border-line bg-ink-1", className)}>
      {title && (
        <header className="flex h-8 items-center justify-between border-b border-line px-3">
          <Label>{title}</Label>
          {action}
        </header>
      )}
      {children}
    </section>
  );
}

export function Stat({
  label,
  value,
  hint,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
}) {
  return (
    <div className="border-r border-line px-4 py-3 last:border-r-0">
      <Label>{label}</Label>
      <div className="mt-1 font-mono text-[22px] leading-none text-fg">{value}</div>
      {hint && <div className="mt-1.5 text-[11px] text-fg-muted">{hint}</div>}
    </div>
  );
}

type Tone = "neutral" | "cyan" | "amber" | "red" | "green";

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: Tone }) {
  const tones: Record<Tone, string> = {
    neutral: "border-line-strong text-fg-muted",
    cyan: "border-cyan-dim bg-cyan-wash text-cyan",
    amber: "border-amber/40 bg-amber-wash text-amber",
    red: "border-red/40 bg-red-wash text-red",
    green: "border-green/40 bg-green-wash text-green",
  };
  return (
    <span
      className={cx(
        "inline-flex items-center rounded-sm border px-1.5 py-px font-mono text-[10px] uppercase tracking-wider",
        tones[tone],
      )}
    >
      {children}
    </span>
  );
}

const severityTone: Record<Severity, Tone> = {
  high: "red",
  medium: "amber",
  low: "cyan",
  info: "neutral",
};

export function SeverityBadge({ severity }: { severity: Severity }) {
  return <Badge tone={severityTone[severity]}>{severity}</Badge>;
}

const evidenceLabel: Record<Evidence, string> = {
  observed_fact: "observed fact",
  heuristic: "heuristic",
  suggestion: "suggestion",
  unknown: "unknown",
};
const evidenceTone: Record<Evidence, Tone> = {
  observed_fact: "green",
  heuristic: "amber",
  suggestion: "cyan",
  unknown: "neutral",
};

/** Facts, heuristics and suggestions must never look alike. */
export function EvidenceBadge({ evidence }: { evidence: Evidence }) {
  return <Badge tone={evidenceTone[evidence]}>{evidenceLabel[evidence]}</Badge>;
}

export function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex h-full min-h-48 flex-col items-center justify-center gap-2 p-8 text-center">
      <div className="font-mono text-[11px] uppercase tracking-[0.14em] text-fg-muted">{title}</div>
      {children && <p className="max-w-md text-[12px] text-fg-dim">{children}</p>}
      {action}
    </div>
  );
}

export function ErrorNote({ error }: { error: { kind: string; message: string } }) {
  return (
    <div
      role="alert"
      className="rounded-sm border border-red/40 bg-red-wash px-3 py-2 text-[12px] text-red"
    >
      <span className="mr-2 font-mono text-[10px] uppercase tracking-wider opacity-70">
        {error.kind}
      </span>
      {error.message}
    </div>
  );
}
