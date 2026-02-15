import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SettingsProps {
  visible: boolean;
  onClose: () => void;
}

export function Settings({ visible, onClose }: SettingsProps) {
  const [currentHotkey, setCurrentHotkey] = useState("");
  const [newHotkey, setNewHotkey] = useState("");
  const [status, setStatus] = useState<{ type: "idle" | "success" | "error"; message?: string }>({ type: "idle" });

  useEffect(() => {
    if (visible) {
      invoke<string>("get_hotkey").then((hotkey) => {
        setCurrentHotkey(hotkey);
        setNewHotkey(hotkey);
        setStatus({ type: "idle" });
      });
    }
  }, [visible]);

  if (!visible) return null;

  const handleSave = async () => {
    if (newHotkey.trim() === "" || newHotkey === currentHotkey) return;
    setStatus({ type: "idle" });

    try {
      await invoke("set_hotkey", { newHotkey: newHotkey.trim() });
      setCurrentHotkey(newHotkey.trim());
      setStatus({ type: "success", message: "Hotkey updated!" });
    } catch (e) {
      setStatus({ type: "error", message: String(e) });
    }
  };

  return (
    <div className="flex flex-col gap-2 px-4 py-3 bg-black/80 backdrop-blur-xl rounded-2xl border border-white/10 w-64">
      <div className="flex items-center justify-between">
        <span className="text-white/80 text-xs font-medium">Hotkey Settings</span>
        <button
          onClick={onClose}
          className="text-white/40 hover:text-white/80 text-xs transition-colors"
        >
          ✕
        </button>
      </div>
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={newHotkey}
          onChange={(e) => {
            setNewHotkey(e.target.value);
            setStatus({ type: "idle" });
          }}
          placeholder="e.g. Alt+Space"
          className="flex-1 bg-white/10 text-white text-xs px-2 py-1.5 rounded-lg border border-white/10 outline-none focus:border-white/30 placeholder:text-white/30"
        />
        <button
          onClick={handleSave}
          disabled={newHotkey.trim() === "" || newHotkey === currentHotkey}
          className="text-xs px-2.5 py-1.5 rounded-lg bg-white/15 text-white/80 hover:bg-white/25 disabled:opacity-30 disabled:cursor-default transition-colors"
        >
          Save
        </button>
      </div>
      {status.type === "success" && (
        <span className="text-green-400 text-xs">{status.message}</span>
      )}
      {status.type === "error" && (
        <span className="text-red-400 text-xs">{status.message}</span>
      )}
    </div>
  );
}
