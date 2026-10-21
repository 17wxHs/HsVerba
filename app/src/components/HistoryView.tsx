import { useEffect, useState } from "react";
import { clearHistory, deleteHistory, listHistory, toggleStar } from "../api";
import type { HistoryRow } from "../types";
import { IconStar, IconTrash } from "./Icons";

export default function HistoryView({
  onLoad,
}: {
  onLoad: (row: HistoryRow) => void;
}) {
  const [rows, setRows] = useState<HistoryRow[]>([]);
  const [q, setQ] = useState("");

  const reload = async (query: string) => setRows(await listHistory(query));

  useEffect(() => {
    const t = setTimeout(() => reload(q), 200);
    return () => clearTimeout(t);
  }, [q]);

  const star = async (id: number) => {
    await toggleStar(id);
    reload(q);
  };

  const del = async (id: number) => {
    await deleteHistory(id);
    reload(q);
  };

  const fmt = (ts: number) => {
    const d = new Date(ts * 1000);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };

  return (
    <div className="view">
      <h1>历史记录</h1>
      <div className="sub">全部存在本机数据库，共 {rows.length} 条</div>

      <input
        className="search"
        placeholder="搜索原文或译文…"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />

      <div className="card" style={{ padding: "4px 20px" }}>
        {rows.length === 0 ? (
          <div className="empty-state">还没有记录。翻译过的内容会自动存在这里。</div>
        ) : (
          rows.map((r) => (
            <div className="list-item" key={r.id}>
              <div className="body" onClick={() => onLoad(r)}>
                <div className="src">{r.source}</div>
                <div className="tgt">{r.target}</div>
                <div className="meta">
                  <span>{fmt(r.createdAt)}</span>
                  <span>{r.model}</span>
                </div>
              </div>
              <button
                className={`icon-btn ${r.starred ? "starred" : ""}`}
                onClick={() => star(r.id)}
                title="收藏"
              >
                <IconStar filled={r.starred} />
              </button>
              <button className="icon-btn" onClick={() => del(r.id)} title="删除">
                <IconTrash />
              </button>
            </div>
          ))
        )}
      </div>

      {rows.length > 0 && (
        <button
          onClick={async () => {
            await clearHistory();
            reload(q);
          }}
        >
          清空未收藏的记录
        </button>
      )}
    </div>
  );
}
