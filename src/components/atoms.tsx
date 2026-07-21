// Small reusable form atoms.

interface SwitchProps {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint?: string;
}

export function Switch({ checked, onChange, label, hint }: SwitchProps) {
  return (
    <div className="field">
      <label>
        {label}
        {hint && (
          <div className="muted" style={{ fontSize: "var(--text-xs)" }}>
            {hint}
          </div>
        )}
      </label>
      <button
        role="switch"
        aria-checked={checked}
        className="switch"
        onClick={() => onChange(!checked)}
        aria-label={label}
      />
    </div>
  );
}

interface NumberFieldProps {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (v: number) => void;
}

export function NumberField({
  label,
  value,
  min,
  max,
  step,
  onChange,
}: NumberFieldProps) {
  return (
    <div className="field">
      <label>{label}</label>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}
