import { useState, useRef, useEffect, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";
import type { DictationState } from "../types";
import { useAudioLevels } from "../hooks/useAudioLevels";
import { AudioWaveform } from "./AudioWaveform";
import { Settings } from "./Settings";

const CORRECTION_AUTO_DISMISS_MS = 3000;

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatLang(code: string): string {
  if (!code) return "--";
  return code.toUpperCase();
}

interface OverlayProps {
  state: DictationState;
}

export function Overlay({ state }: OverlayProps) {
  const [showSettings, setShowSettings] = useState(false);
  const textRef = useRef<HTMLDivElement>(null);
  const audioLevels = useAudioLevels(state.type === "Recording");

  const isDragging = useRef(false);
  const dragStartMouse = useRef({ x: 0, y: 0 });
  const dragStartPos = useRef({ x: 0, y: 0 });

  const partialText = state.type === "Recording" ? state.partial_text : undefined;
  const partialTranslation =
    state.type === "Recording" ? state.partial_translation : undefined;

  useEffect(() => {
    if (textRef.current) {
      textRef.current.scrollTop = textRef.current.scrollHeight;
    }
  }, [partialText, partialTranslation]);

  // CorrectionPreview: auto-dismiss timer with progress tracking
  const [previewProgress, setPreviewProgress] = useState(0);
  const previewTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const previewAnimRef = useRef<number | null>(null);
  const previewStartRef = useRef<number>(0);

  useEffect(() => {
    if (state.type !== "CorrectionPreview") {
      setPreviewProgress(0);
      if (previewTimerRef.current) {
        clearTimeout(previewTimerRef.current);
        previewTimerRef.current = null;
      }
      if (previewAnimRef.current) {
        cancelAnimationFrame(previewAnimRef.current);
        previewAnimRef.current = null;
      }
      return;
    }

    previewStartRef.current = Date.now();

    const animate = () => {
      const elapsed = Date.now() - previewStartRef.current;
      const progress = Math.min(elapsed / CORRECTION_AUTO_DISMISS_MS, 1);
      setPreviewProgress(progress);
      if (progress < 1) {
        previewAnimRef.current = requestAnimationFrame(animate);
      }
    };
    previewAnimRef.current = requestAnimationFrame(animate);

    previewTimerRef.current = setTimeout(() => {
      invoke("accept_corrections");
    }, CORRECTION_AUTO_DISMISS_MS);

    return () => {
      if (previewTimerRef.current) {
        clearTimeout(previewTimerRef.current);
        previewTimerRef.current = null;
      }
      if (previewAnimRef.current) {
        cancelAnimationFrame(previewAnimRef.current);
        previewAnimRef.current = null;
      }
    };
  }, [state.type]);

  // Preview keyboard event listeners
  useEffect(() => {
    if (state.type !== "CorrectionPreview" && state.type !== "TranslationPreview") return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (state.type === "CorrectionPreview") {
        if (e.key === "Escape") {
          invoke("undo_corrections");
        } else if (e.key === "Enter") {
          invoke("accept_corrections");
        }
      } else if (state.type === "TranslationPreview") {
        if (e.key === "Escape") {
          invoke("reject_translation");
        } else if (e.key === "Enter") {
          invoke("accept_translation");
        }
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [state.type]);

  const handleMouseDown = useCallback(async (e: React.MouseEvent) => {
    if (e.button !== 0) return;

    const appWindow = getCurrentWindow();
    const pos = await appWindow.outerPosition();
    const scaleFactor = await appWindow.scaleFactor();

    dragStartPos.current = {
      x: pos.x / scaleFactor,
      y: pos.y / scaleFactor,
    };
    dragStartMouse.current = { x: e.screenX, y: e.screenY };
    isDragging.current = true;
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isDragging.current) return;

      const deltaX = e.screenX - dragStartMouse.current.x;
      const deltaY = e.screenY - dragStartMouse.current.y;

      const newX = dragStartPos.current.x + deltaX;
      const newY = dragStartPos.current.y + deltaY;

      getCurrentWindow().setPosition(new LogicalPosition(newX, newY));
    };

    const handleMouseUp = () => {
      if (!isDragging.current) return;
      isDragging.current = false;

      const appWindow = getCurrentWindow();
      appWindow.outerPosition().then(async (pos) => {
        const scaleFactor = await appWindow.scaleFactor();
        invoke("save_overlay_position", {
          x: pos.x / scaleFactor,
          y: pos.y / scaleFactor,
        });
      });
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };
  }, []);

  if (state.type === "Idle" && !showSettings) {
    return null;
  }

  const isCorrectionPreview = state.type === "CorrectionPreview";
  const isTranslationPreview = state.type === "TranslationPreview";
  const isPreview = isCorrectionPreview || isTranslationPreview;
  const displayedCorrections = isCorrectionPreview
    ? state.corrections.slice(0, 5)
    : [];
  const remainingCount = isCorrectionPreview
    ? Math.max(0, state.corrections.length - 5)
    : 0;

  return (
    <div
      className="flex flex-col items-center gap-2 cursor-grab active:cursor-grabbing"
      onMouseDown={handleMouseDown}
    >
      {state.type !== "Idle" && (
        <div className="flex flex-col gap-2 px-5 py-3 bg-black/80 backdrop-blur-xl rounded-2xl border border-white/10 max-w-[300px]">
          {!isPreview && (
            <div className="flex items-center gap-3">
              {state.type === "Recording" && (
                <>
                  <div className="w-2 h-2 rounded-full bg-red-500 animate-pulse flex-shrink-0" />
                  <span className="text-white/40 text-sm font-medium tabular-nums">
                    {formatDuration(state.duration_ms)}
                  </span>
                  <span className="text-white/70 text-sm font-medium">
                    {state.partial_text ? "Transcribing..." : "Listening..."}
                  </span>
                  <span className="text-white/35 text-xs font-medium">
                    {formatLang(state.source_lang)} → {formatLang(state.target_lang)}
                  </span>
                </>
              )}

              {state.type === "Processing" && (
                <>
                  <div className="w-3 h-3 rounded-full border-2 border-blue-400 border-t-transparent animate-spin" />
                  <span className="text-blue-400 text-sm font-medium">Transcribing...</span>
                </>
              )}

              {state.type === "Translating" && (
                <>
                  <div className="w-3 h-3 rounded-full border-2 border-cyan-400 border-t-transparent animate-spin" />
                  <span className="text-cyan-400 text-sm font-medium">Translating...</span>
                </>
              )}

              {state.type === "Downloading" && (
                <>
                  <div className="w-3 h-3 rounded-full border-2 border-green-400 border-t-transparent animate-spin" />
                  <span className="text-green-400 text-sm font-medium">
                    Downloading model... {Math.round(state.progress * 100)}%
                  </span>
                </>
              )}

              {state.type === "Error" && (
                <>
                  <div className="w-2.5 h-2.5 rounded-full bg-yellow-400" />
                  <span className="text-yellow-400 text-sm font-medium">{state.message}</span>
                </>
              )}

              <div className="ml-auto flex items-center gap-1.5 flex-shrink-0">
                <button
                  onMouseDown={(e) => e.stopPropagation()}
                  onClick={() => setShowSettings(!showSettings)}
                  className="text-white/30 hover:text-white/70 transition-colors"
                  title="Settings"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
                  </svg>
                </button>
                <button
                  onMouseDown={(e) => e.stopPropagation()}
                  onClick={() => invoke("cancel_recording")}
                  className="text-white/30 hover:text-white/70 transition-colors"
                  title="Cancel"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <line x1="18" y1="6" x2="6" y2="18" />
                    <line x1="6" y1="6" x2="18" y2="18" />
                  </svg>
                </button>
              </div>
            </div>
          )}

          {isCorrectionPreview && (
            <div className="flex flex-col gap-1.5">
              <div className="flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-green-400 flex-shrink-0" />
                <span className="text-green-400 text-sm font-medium">Corrected</span>
              </div>
              <div className="flex flex-col gap-1">
                {displayedCorrections.map((c, i) => (
                  <div key={i} className="flex items-center gap-1.5 text-xs">
                    <span className="text-white/40 line-through">{c.original}</span>
                    <span className="text-white/30">→</span>
                    <span className="text-green-400">{c.replacement}</span>
                  </div>
                ))}
                {remainingCount > 0 && (
                  <span className="text-white/30 text-xs">+{remainingCount} more</span>
                )}
              </div>
              <div className="w-full h-0.5 bg-white/10 rounded-full overflow-hidden mt-1">
                <div
                  className="h-full bg-green-400/60 rounded-full transition-none"
                  style={{ width: `${(1 - previewProgress) * 100}%` }}
                />
              </div>
              <span className="text-white/25 text-[10px] text-center">
                Enter to accept · Esc to undo
              </span>
            </div>
          )}

          {isTranslationPreview && (
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-cyan-400 flex-shrink-0" />
                <span className="text-cyan-400 text-sm font-medium">
                  Translated {formatLang(state.source_lang)} → {formatLang(state.target_lang)}
                </span>
              </div>
              <div className="text-[10px] uppercase tracking-wide text-white/35">Source</div>
              <div className="text-sm text-white/50 leading-relaxed">{state.source_text}</div>
              <div className="text-[10px] uppercase tracking-wide text-white/35">Output</div>
              <div className="text-sm text-white/95 leading-relaxed">{state.translated_text}</div>
              <span className="text-white/25 text-[10px] text-center">
                Enter to paste · Esc for original
              </span>
            </div>
          )}

          {state.type === "Recording" && (
            <AudioWaveform levels={audioLevels} />
          )}

          {state.type === "Recording" && state.partial_text && (
            <div ref={textRef} className="max-h-[280px] overflow-y-auto flex flex-col gap-1">
              <div className="text-white/90 text-sm leading-relaxed">{state.partial_text}</div>
              {state.partial_translation && (
                <div className="text-white/55 text-sm leading-relaxed">
                  {state.partial_translation}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      <div onMouseDown={(e) => e.stopPropagation()}>
        <Settings visible={showSettings} onClose={() => setShowSettings(false)} />
      </div>
    </div>
  );
}
