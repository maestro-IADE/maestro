import type { ReactNode } from "react";

// The pieces every settings section is built from (docs/features/ui/settings-dialog.md "구현"): a group title over a
// card, rows inside the card (label and description on the left, the control on the right), a switch, a segmented
// choice.

/** An on/off setting: a toggle switch (a checkbox underneath, so keyboard and screen readers work as usual). */
export function Switch({ on, disabled, label, onChange }: { on: boolean; disabled?: boolean; label?: string; onChange: (on: boolean) => void }) {
  return (
    <label className="switch" title={on ? "끄기" : "켜기"}>
      <input type="checkbox" role="switch" aria-label={label} checked={on} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}

/** A group: its title outside the card (13/600), then the card. `head` adds controls at the title's right. */
export function Group({ title, head, children, plain }: { title?: string; head?: ReactNode; children: ReactNode; plain?: boolean }) {
  return (
    <section className="set-group">
      {(title || head) && (
        <div className="set-group-head">
          {title && <h4>{title}</h4>}
          {head && <><span className="spacer" />{head}</>}
        </div>
      )}
      {plain ? children : <div className="set-card">{children}</div>}
    </section>
  );
}

/** One row of a card: the label (13) and its description (12, muted, two lines at most), the control on the right. */
export function Row({ label, desc, children, target }: { label: ReactNode; desc?: ReactNode; children?: ReactNode; target?: boolean }) {
  return (
    <div className="set-row" data-target={target || undefined}>
      <div className="set-label">
        <div>{label}</div>
        {desc && <div className="set-desc">{desc}</div>}
      </div>
      {children}
    </div>
  );
}

/** A segmented choice: one button per option, the chosen one marked. */
export function Seg<T extends string | number>({ value, options, onChange, disabled }: {
  value: T;
  options: ReadonlyArray<readonly [T, ReactNode]>;
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="seg">
      {options.map(([v, label]) => (
        <button key={String(v)} disabled={disabled} data-on={v === value || undefined} onClick={() => onChange(v)}>{label}</button>
      ))}
    </div>
  );
}
