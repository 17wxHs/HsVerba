import { useEffect, useState } from "react";
import { addGlossary, deleteGlossary, listGlossary } from "../api";
import type { GlossaryRow } from "../types";
import { IconPlus, IconTrash } from "./Icons";

export default function GlossaryView() {
  const [rows, setRows] = useState<GlossaryRow[]>([]);
  const [term, setTerm] = useState("");
  const [tr, setTr] = useState("");
  const [note, setNote] = useState("");
  const [err, setErr] = useState("");

  const reload = async () => setRows(await listGlossary());

  useEffect(() => {
    reload();
  }, []);

  const add = async () => {
    setErr("");
    if (!term.trim() || !tr.trim()) {
      setErr("术语和译法都要填");
      return;
    }
    try {
      await addGlossary(term, tr, note);
      setTerm("");
      setTr("");
      setNote("");
      reload();
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <div className="view">
      <h1>术语表</h1>
      <div className="sub">
        这里的术语在翻译时会被强制使用，并做译后校验——实测这是把术语一致性做到 100% 的唯一可靠办法。
      </div>

      <div className="card">
        <div className="row">
          <div className="field" style={{ marginBottom: 10 }}>
            <label>原文术语</label>
            <input
              value={term}
              onChange={(e) => setTerm(e.target.value)}
              placeholder="idempotency key"
              spellCheck={false}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
          </div>
          <div className="field" style={{ marginBottom: 10 }}>
            <label>指定译法</label>
            <input
              value={tr}
              onChange={(e) => setTr(e.target.value)}
              placeholder="幂等键"
              spellCheck={false}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
          </div>
        </div>
        <div className="field" style={{ marginBottom: 12 }}>
          <label>备注（可选）</label>
          <input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="只在 API 文档里这么用"
            spellCheck={false}
          />
        </div>
        <button className="primary" onClick={add}>
          <span style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
            <IconPlus /> 添加术语
          </span>
        </button>
        {err && <span className="err" style={{ marginLeft: 12 }}>{err}</span>}
      </div>

      <div className="card" style={{ padding: "4px 20px" }}>
        {rows.length === 0 ? (
          <div className="empty-state">还没有术语。加上几条，翻译时会自动生效。</div>
        ) : (
          rows.map((r) => (
            <div className="list-item" key={r.id}>
              <div className="body">
                <div className="src">
                  {r.term} <span style={{ color: "var(--faint)" }}>→</span> {r.translation}
                </div>
                {r.note && <div className="tgt">{r.note}</div>}
              </div>
              <button
                className="icon-btn"
                onClick={async () => {
                  await deleteGlossary(r.id);
                  reload();
                }}
                title="删除"
              >
                <IconTrash />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
