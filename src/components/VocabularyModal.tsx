import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface VocabEntry {
  id: number;
  phrase: string;
  replacement: string;
  enabled: boolean;
}

interface VocabularyModalProps {
  visible: boolean;
  onClose: () => void;
}

export function VocabularyModal({ visible, onClose }: VocabularyModalProps) {
  const [entries, setEntries] = useState<VocabEntry[]>([]);
  const [phrase, setPhrase] = useState("");
  const [replacement, setReplacement] = useState("");

  const fetchEntries = async () => {
    try {
      const result = await invoke<VocabEntry[]>("get_vocabulary");
      setEntries(result);
    } catch (e) {
      console.error("Failed to fetch vocabulary:", e);
    }
  };

  useEffect(() => {
    if (visible) {
      fetchEntries();
    }
  }, [visible]);

  if (!visible) return null;

  const handleAdd = async () => {
    const trimmedPhrase = phrase.trim();
    const trimmedReplacement = replacement.trim();
    if (!trimmedPhrase || !trimmedReplacement) return;

    try {
      await invoke("add_vocab_entry", {
        phrase: trimmedPhrase,
        replacement: trimmedReplacement,
      });
      setPhrase("");
      setReplacement("");
      await fetchEntries();
    } catch (e) {
      console.error("Failed to add entry:", e);
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await invoke("delete_vocab_entry", { id });
      await fetchEntries();
    } catch (e) {
      console.error("Failed to delete entry:", e);
    }
  };

  const handleToggleEnabled = async (entry: VocabEntry) => {
    try {
      await invoke("update_vocab_entry", {
        id: entry.id,
        phrase: entry.phrase,
        replacement: entry.replacement,
        enabled: !entry.enabled,
      });
      await fetchEntries();
    } catch (e) {
      console.error("Failed to update entry:", e);
    }
  };

  return (
    <div className="absolute inset-0 flex items-center justify-center z-50 bg-black/50">
      <div className="bg-black/90 backdrop-blur-xl rounded-2xl border border-white/10 w-80 p-4 flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-white">
            Manage Vocabulary
          </span>
          <button
            onClick={onClose}
            className="text-white/40 hover:text-white/80 text-xs transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="flex gap-2">
          <input
            type="text"
            value={phrase}
            onChange={(e) => setPhrase(e.target.value)}
            placeholder="Phrase"
            className="flex-1 bg-white/10 text-white text-xs px-2 py-1.5 rounded-lg border border-white/10 outline-none focus:border-white/30 placeholder:text-white/30"
          />
          <input
            type="text"
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            placeholder="Replacement"
            className="flex-1 bg-white/10 text-white text-xs px-2 py-1.5 rounded-lg border border-white/10 outline-none focus:border-white/30 placeholder:text-white/30"
          />
          <button
            onClick={handleAdd}
            disabled={!phrase.trim() || !replacement.trim()}
            className="text-xs px-2.5 py-1.5 rounded-lg bg-blue-500 text-white hover:bg-blue-600 disabled:opacity-30 disabled:cursor-default transition-colors"
          >
            Add
          </button>
        </div>

        <div className="max-h-[400px] overflow-y-auto flex flex-col gap-1.5">
          {entries.length === 0 && (
            <div className="text-center text-white/30 text-xs py-6">
              No vocabulary entries yet. Add your first correction above.
            </div>
          )}

          {entries.map((entry) => (
            <div
              key={entry.id}
              className="flex items-center gap-2 p-2 rounded-lg border border-white/10 bg-white/5"
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5 text-xs">
                  <span
                    className={`truncate ${entry.enabled ? "text-white/80" : "text-white/30"}`}
                  >
                    {entry.phrase}
                  </span>
                  <span className="text-white/20 shrink-0">&rarr;</span>
                  <span
                    className={`truncate ${entry.enabled ? "text-green-400/80" : "text-white/30"}`}
                  >
                    {entry.replacement}
                  </span>
                </div>
              </div>

              <button
                onClick={() => handleToggleEnabled(entry)}
                className="shrink-0"
                title={entry.enabled ? "Disable" : "Enable"}
              >
                <div
                  className={`w-7 h-4 rounded-full transition-colors flex items-center ${
                    entry.enabled
                      ? "bg-blue-500 justify-end"
                      : "bg-white/20 justify-start"
                  }`}
                >
                  <div className="w-3 h-3 bg-white rounded-full mx-0.5" />
                </div>
              </button>

              <button
                onClick={() => handleDelete(entry.id)}
                className="text-white/30 hover:text-red-400 transition-colors shrink-0"
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
          ))}
        </div>
      </div>
    </div>
  );
}
