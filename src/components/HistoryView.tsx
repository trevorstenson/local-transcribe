import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface HistoryEntry {
  id: number;
  text: string;
  timestamp_ms: number;
  duration_ms: number;
}

function formatTimestamp(ms: number): string {
  const date = new Date(ms);
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function HistoryView() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [copiedId, setCopiedId] = useState<number | null>(null);
  const [search, setSearch] = useState("");

  const fetchHistory = async () => {
    try {
      const result = await invoke<HistoryEntry[]>("get_history");
      setEntries(result);
    } catch (e) {
      console.error("Failed to fetch history:", e);
    }
  };

  useEffect(() => {
    fetchHistory();

    const unlisten = listen("history-updated", () => {
      fetchHistory();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleCopy = async (entry: HistoryEntry) => {
    try {
      await invoke("copy_history_entry", { text: entry.text });
      setCopiedId(entry.id);
      setTimeout(() => setCopiedId(null), 1500);
    } catch (e) {
      console.error("Failed to copy:", e);
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await invoke("delete_history_entry", { id });
      setEntries((prev) => prev.filter((e) => e.id !== id));
    } catch (e) {
      console.error("Failed to delete:", e);
    }
  };

  const handleClearAll = async () => {
    try {
      await invoke("clear_history");
      setEntries([]);
    } catch (e) {
      console.error("Failed to clear history:", e);
    }
  };

  const filtered = search
    ? entries.filter((e) =>
        e.text.toLowerCase().includes(search.toLowerCase())
      )
    : entries;

  return (
    <div className="flex flex-col h-full bg-gray-900 text-white p-6">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h1 className="text-lg font-semibold">Transcription History</h1>
          <p className="text-sm text-white/50">
            {entries.length} transcription{entries.length !== 1 ? "s" : ""}
          </p>
        </div>
        {entries.length > 0 && (
          <button
            onClick={handleClearAll}
            className="text-xs text-red-400/60 hover:text-red-400 transition-colors px-2 py-1"
          >
            Clear All
          </button>
        )}
      </div>

      {entries.length > 3 && (
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search transcriptions..."
          className="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-sm text-white mb-3 placeholder-white/30 focus:outline-none focus:border-blue-500"
        />
      )}

      <div className="flex flex-col gap-2 flex-1 overflow-y-auto">
        {filtered.length === 0 && (
          <div className="flex-1 flex items-center justify-center text-white/30 text-sm">
            {search ? "No matching transcriptions" : "No transcriptions yet"}
          </div>
        )}

        {filtered.map((entry) => {
          const isExpanded = expandedId === entry.id;
          const isCopied = copiedId === entry.id;

          return (
            <div
              key={entry.id}
              className="flex flex-col gap-1.5 p-3 rounded-xl border border-white/10 bg-white/5"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2 text-xs text-white/40">
                  <span>{formatTimestamp(entry.timestamp_ms)}</span>
                  <span className="text-white/20">|</span>
                  <span>{formatDuration(entry.duration_ms)}</span>
                </div>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => handleCopy(entry)}
                    className="text-xs text-white/30 hover:text-white/70 transition-colors px-1.5 py-0.5"
                    title="Copy to clipboard"
                  >
                    {isCopied ? "Copied!" : "Copy"}
                  </button>
                  <button
                    onClick={() => handleDelete(entry.id)}
                    className="text-xs text-white/30 hover:text-red-400 transition-colors px-1.5 py-0.5"
                    title="Delete"
                  >
                    <svg
                      width="12"
                      height="12"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <line x1="18" y1="6" x2="6" y2="18" />
                      <line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  </button>
                </div>
              </div>

              <button
                onClick={() => setExpandedId(isExpanded ? null : entry.id)}
                className="text-left"
              >
                <p
                  className={`text-sm text-white/80 leading-relaxed ${
                    isExpanded ? "" : "line-clamp-2"
                  }`}
                >
                  {entry.text}
                </p>
                {!isExpanded && entry.text.length > 120 && (
                  <span className="text-xs text-white/30 mt-0.5">
                    Show more
                  </span>
                )}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
