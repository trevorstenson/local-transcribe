import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { VocabularyModal } from "./VocabularyModal";

interface ModelInfo {
  name: string;
  size_mb: number;
  description: string;
  downloaded: boolean;
  selected: boolean;
  english_only: boolean;
}

const LANGUAGES = [
  { code: "auto", label: "Auto-detect" },
  { code: "en", label: "English" },
  { code: "es", label: "Spanish" },
  { code: "fr", label: "French" },
  { code: "de", label: "German" },
  { code: "it", label: "Italian" },
  { code: "pt", label: "Portuguese" },
  { code: "zh", label: "Chinese" },
  { code: "ja", label: "Japanese" },
  { code: "ko", label: "Korean" },
  { code: "ru", label: "Russian" },
  { code: "ar", label: "Arabic" },
  { code: "hi", label: "Hindi" },
  { code: "nl", label: "Dutch" },
  { code: "pl", label: "Polish" },
  { code: "tr", label: "Turkish" },
  { code: "sv", label: "Swedish" },
  { code: "uk", label: "Ukrainian" },
];

export function ModelSettings() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [smartPaste, setSmartPaste] = useState(true);
  const [autostart, setAutostart] = useState(false);
  const [language, setLanguage] = useState("en");
  const [vocabEnabled, setVocabEnabled] = useState<boolean>(true);
  const [showVocabModal, setShowVocabModal] = useState(false);

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
    invoke<boolean>("get_smart_paste").then(setSmartPaste);
    invoke<string>("get_language").then(setLanguage);
    invoke<boolean>("get_vocab_enabled").then(setVocabEnabled);
    isEnabled().then(setAutostart).catch(() => {});

    const unlistenModel = listen("model-changed", () => {
      fetchModels();
      setLoading(null);
    });

    return () => {
      unlistenModel.then((fn) => fn());
    };
  }, []);

  const handleLanguageChange = async (newLang: string) => {
    const oldLang = language;
    setLanguage(newLang);
    setError(null);
    try {
      await invoke("set_language", { language: newLang });
      await fetchModels();
    } catch (e) {
      setLanguage(oldLang);
      setError(String(e));
    }
  };

  const handleToggleAutostart = async () => {
    const newValue = !autostart;
    setAutostart(newValue);
    try {
      if (newValue) {
        await enable();
      } else {
        await disable();
      }
    } catch (e) {
      setAutostart(!newValue);
      setError(String(e));
    }
  };

  const handleToggleSmartPaste = async () => {
    const newValue = !smartPaste;
    setSmartPaste(newValue);
    try {
      await invoke("set_smart_paste", { enabled: newValue });
    } catch (e) {
      setSmartPaste(!newValue);
      setError(String(e));
    }
  };

  const handleToggleVocab = async () => {
    const newValue = !vocabEnabled;
    setVocabEnabled(newValue);
    try {
      await invoke("set_vocab_enabled", { enabled: newValue });
    } catch (e) {
      setVocabEnabled(!newValue);
      setError(String(e));
    }
  };

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

  // Filter models based on selected language
  const showEnglishOnly = language === "en";
  const filteredModels = models.filter((m) =>
    showEnglishOnly ? m.english_only : !m.english_only
  );

  return (
    <div className="flex flex-col h-full bg-gray-900 text-white p-6">
      <h1 className="text-lg font-semibold mb-1">Dictate Settings</h1>
      <p className="text-sm text-white/50 mb-4">
        Choose your language and transcription model
      </p>

      <div className="mb-4">
        <label className="text-sm font-medium mb-1.5 block">Language</label>
        <select
          value={language}
          onChange={(e) => handleLanguageChange(e.target.value)}
          className="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-2 text-sm text-white appearance-none cursor-pointer hover:bg-white/10 transition-colors focus:outline-none focus:border-blue-500"
        >
          {LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code} className="bg-gray-900">
              {lang.label}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-col gap-2 flex-1 overflow-y-auto">
        {filteredModels.map((model) => (
          <ModelCard
            key={model.name}
            model={model}
            isLoading={loading === model.name}
            disabled={loading !== null}
            onSelect={() => handleSelectModel(model.name)}
          />
        ))}
      </div>

      <div className="mt-4 pt-4 border-t border-white/10 flex flex-col gap-3">
        <button
          onClick={handleToggleSmartPaste}
          className="flex items-center justify-between w-full"
        >
          <div className="flex flex-col items-start">
            <span className="text-sm font-medium">Smart Paste</span>
            <span className="text-xs text-white/40">
              {smartPaste
                ? "Only auto-pastes when a text field is focused"
                : "Always tries to paste immediately"}
            </span>
          </div>
          <div
            className={`w-9 h-5 rounded-full transition-colors flex items-center ${
              smartPaste ? "bg-blue-500 justify-end" : "bg-white/20 justify-start"
            }`}
          >
            <div className="w-4 h-4 bg-white rounded-full mx-0.5" />
          </div>
        </button>

        <button
          onClick={handleToggleAutostart}
          className="flex items-center justify-between w-full"
        >
          <div className="flex flex-col items-start">
            <span className="text-sm font-medium">Launch at Login</span>
            <span className="text-xs text-white/40">
              {autostart
                ? "Dictate starts automatically when you log in"
                : "Dictate must be started manually"}
            </span>
          </div>
          <div
            className={`w-9 h-5 rounded-full transition-colors flex items-center ${
              autostart ? "bg-blue-500 justify-end" : "bg-white/20 justify-start"
            }`}
          >
            <div className="w-4 h-4 bg-white rounded-full mx-0.5" />
          </div>
        </button>

        <button
          onClick={handleToggleVocab}
          className="flex items-center justify-between w-full"
        >
          <div className="flex flex-col items-start">
            <span className="text-sm font-medium">Personal Vocabulary</span>
            <span className="text-xs text-white/40">
              {vocabEnabled
                ? "Corrects transcriptions using your custom dictionary"
                : "Vocabulary corrections are disabled"}
            </span>
          </div>
          <div
            className={`w-9 h-5 rounded-full transition-colors flex items-center ${
              vocabEnabled ? "bg-blue-500 justify-end" : "bg-white/20 justify-start"
            }`}
          >
            <div className="w-4 h-4 bg-white rounded-full mx-0.5" />
          </div>
        </button>

        {vocabEnabled && (
          <button
            onClick={() => setShowVocabModal(true)}
            className="text-xs text-blue-400 hover:text-blue-300 transition-colors text-left pl-0.5"
          >
            Manage Vocabulary...
          </button>
        )}
      </div>

      {error && <div className="mt-3 text-red-400 text-sm">{error}</div>}

      <VocabularyModal
        visible={showVocabModal}
        onClose={() => setShowVocabModal(false)}
      />
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
