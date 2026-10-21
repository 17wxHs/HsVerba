import { useState } from "react";

const LETTER_ROWS = ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"];
const DIGITS = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const FN_KEYS = Array.from({ length: 12 }, (_, i) => `F${i + 1}`);
const SPECIALS: { label: string; code: string }[] = [
  { label: "Space", code: "Space" },
  { label: "Tab", code: "Tab" },
  { label: "Enter", code: "Enter" },
  { label: "Esc", code: "Esc" },
  { label: "←", code: "Left" },
  { label: "→", code: "Right" },
  { label: "↑", code: "Up" },
  { label: "↓", code: "Down" },
];
const SOLO_MODS: { label: string; code: string }[] = [
  { label: "Ctrl", code: "Control" },
  { label: "Alt", code: "Alt" },
  { label: "Shift", code: "Shift" },
];

function keyCode(k: string): string {
  if (/^[a-z]$/.test(k)) return "Key" + k.toUpperCase();
  if (/^[0-9]$/.test(k)) return "Digit" + k;
  return k;
}

export default function KeyPicker({
  onPick,
  onClose,
}: {
  onPick: (value: string) => void;
  onClose: () => void;
}) {
  const [mode, setMode] = useState<"combo" | "double">("double");
  const [ctrl, setCtrl] = useState(false);
  const [alt, setAlt] = useState(false);
  const [shift, setShift] = useState(false);

  const mods = mode === "combo" ? [ctrl && "CommandOrControl", alt && "Alt", shift && "Shift"].filter(Boolean) : [];

  const pick = (code: string) => {
    if (mode === "double") {
      // 按住的修饰键用短名写在 Double 之后
      const held = [ctrl && "Ctrl", alt && "Alt", shift && "Shift"].filter(Boolean);
      onPick(["Double", ...held, code].join("+"));
    } else {
      if (mods.length === 0) return; // 组合键必须带修饰键
      onPick([...mods, code].join("+"));
    }
    onClose();
  };

  const Key = ({ label, code, disabled }: { label: string; code: string; disabled?: boolean }) => (
    <button
      className="kp-key"
      disabled={disabled}
      onClick={() => pick(code)}
      title={disabled ? "该用法不适用当前模式" : undefined}
    >
      {label}
    </button>
  );

  return (
    <div className="kp-panel" onMouseDown={(e) => e.preventDefault()}>
      <div className="kp-head">
        <div className="kp-modes">
          <button className={mode === "double" ? "on" : ""} onClick={() => setMode("double")}>
            双击触发
          </button>
          <button className={mode === "combo" ? "on" : ""} onClick={() => setMode("combo")}>
            组合键
          </button>
        </div>
        <div className="kp-mods">
          <label>
            <input type="checkbox" checked={ctrl} onChange={(e) => setCtrl(e.target.checked)} />
            {mode === "double" ? "按住 Ctrl" : "Ctrl"}
          </label>
          <label>
            <input type="checkbox" checked={alt} onChange={(e) => setAlt(e.target.checked)} />
            {mode === "double" ? "按住 Alt" : "Alt"}
          </label>
          <label>
            <input type="checkbox" checked={shift} onChange={(e) => setShift(e.target.checked)} />
            {mode === "double" ? "按住 Shift" : "Shift"}
          </label>
        </div>
        <button className="kp-close" onClick={onClose}>
          ✕
        </button>
      </div>
      <div className="kp-hint">
        {mode === "double"
          ? "点一个键 = 连按两下该键触发；勾选「按住 Ctrl」= 按住 Ctrl 再连按两下（推荐，几乎不会误触）。"
          : "勾选修饰键，再点主键。"}
      </div>

      <div className="kp-grid">
        {LETTER_ROWS.map((row) => (
          <div className="kp-row" key={row}>
            {row.split("").map((k) => (
              <Key key={k} label={k.toUpperCase()} code={keyCode(k)} />
            ))}
          </div>
        ))}
        <div className="kp-row">
          {DIGITS.map((k) => (
            <Key key={k} label={k} code={keyCode(k)} />
          ))}
        </div>
        <div className="kp-row">
          {FN_KEYS.map((k) => (
            <Key key={k} label={k} code={k} />
          ))}
        </div>
        <div className="kp-row">
          {SPECIALS.map((s) => (
            <Key key={s.code} label={s.label} code={s.code} />
          ))}
          {SOLO_MODS.map((s) => (
            <Key
              key={s.code}
              label={s.label}
              code={s.code}
              disabled={mode === "combo"}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
