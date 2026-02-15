import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ModelInfo {
  name: string;
  size_mb: number;
  description: string;
  downloaded: boolean;
  selected: boolean;
}

export function ModelSettings() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchModels = async () => {
    try {
      const result = await invoke<ModelInfo[]>("get_models");
      setModels(result);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    fetchModels();

    const unlistenModel = listen("model-changed", () => {
      fetchModels();
      setLoading(null);
    });

    return () => {
      unlistenModel.then((fn) => fn());
    };
  }, []);

  const handleSelectModel = async (modelName: string) => {
    setLoading(modelName);
    setError(null);
    try {
      await invoke("select_model", { modelName });
      await fetchModels();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  };

  return (
    <div className="flex flex-col h-full bg-gray-900 text-white p-6">
      <h1 className="text-lg font-semibold mb-1">Dictate Settings</h1>
      <p className="text-sm text-white/50 mb-4">
        Choose your transcription model
      </p>

      <div className="flex flex-col gap-2 flex-1 overflow-y-auto">
        {models.map((model) => (
          <ModelCard
            key={model.name}
            model={model}
            isLoading={loading === model.name}
            disabled={loading !== null}
            onSelect={() => handleSelectModel(model.name)}
          />
        ))}
      </div>

      {error && <div className="mt-3 text-red-400 text-sm">{error}</div>}
    </div>
  );
}

function ModelCard({
  model,
  isLoading,
  disabled,
  onSelect,
}: {
  model: ModelInfo;
  isLoading: boolean;
  disabled: boolean;
  onSelect: () => void;
}) {
  const sizeLabel =
    model.size_mb >= 1000
      ? `${(model.size_mb / 1024).toFixed(1)} GB`
      : `${model.size_mb} MB`;

  return (
    <button
      onClick={onSelect}
      disabled={model.selected || disabled}
      className={`flex flex-col gap-1 p-3 rounded-xl border text-left transition-colors ${
        model.selected
          ? "border-blue-500 bg-blue-500/10"
          : "border-white/10 bg-white/5 hover:bg-white/10"
      } disabled:cursor-default`}
    >
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">{model.name}</span>
        <div className="flex items-center gap-2">
          <span className="text-xs text-white/40">{sizeLabel}</span>
          {model.selected && (
            <span className="text-xs text-blue-400 font-medium">Active</span>
          )}
          {!model.downloaded && !model.selected && (
            <span className="text-xs text-white/30">Not downloaded</span>
          )}
          {model.downloaded && !model.selected && (
            <span className="text-xs text-green-400/60">Ready</span>
          )}
        </div>
      </div>
      <span className="text-xs text-white/40">{model.description}</span>
      {isLoading && (
        <div className="flex items-center gap-2 mt-1">
          <div className="w-2.5 h-2.5 rounded-full border-2 border-blue-400 border-t-transparent animate-spin" />
          <span className="text-xs text-blue-400">
            {model.downloaded ? "Loading..." : "Downloading..."}
          </span>
        </div>
      )}
    </button>
  );
}
