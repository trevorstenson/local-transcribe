import { useState, useRef, useEffect, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";
import type { DictationState } from "../types";
import { useAudioLevels } from "../hooks/useAudioLevels";
import { AudioWaveform } from "./AudioWaveform";
import { Settings } from "./Settings";

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

  useEffect(() => {
    if (textRef.current) {
      textRef.current.scrollTop = textRef.current.scrollHeight;
    }
  }, [partialText]);

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

  return (
    <div
      className="flex flex-col items-center gap-2 cursor-grab active:cursor-grabbing"
      onMouseDown={handleMouseDown}
    >
      {state.type !== "Idle" && (
        <div className="flex flex-col gap-2 px-5 py-3 bg-black/80 backdrop-blur-xl rounded-2xl border border-white/10 max-w-[300px]">
          <div className="flex items-center gap-3">
            {state.type === "Recording" && (
              <>
                <div className="w-2 h-2 rounded-full bg-red-500 animate-pulse flex-shrink-0" />
                <span className="text-white/70 text-sm font-medium">
                  {state.partial_text ? "Transcribing..." : "Listening..."}
                </span>
              </>
            )}

            {state.type === "Processing" && (
              <>
                <div className="w-3 h-3 rounded-full border-2 border-blue-400 border-t-transparent animate-spin" />
                <span className="text-blue-400 text-sm font-medium">Transcribing...</span>
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

            <button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={() => setShowSettings(!showSettings)}
              className="ml-auto text-white/30 hover:text-white/70 transition-colors flex-shrink-0"
              title="Settings"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
            </button>
          </div>

          {state.type === "Recording" && (
            <AudioWaveform levels={audioLevels} />
          )}

          {state.type === "Recording" && state.partial_text && (
            <div
              ref={textRef}
              className="text-white/90 text-sm leading-relaxed max-h-[280px] overflow-y-auto"
            >
              {state.partial_text}
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
