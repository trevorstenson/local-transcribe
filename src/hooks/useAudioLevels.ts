import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

const NUM_BARS = 48;

export function useAudioLevels(active: boolean): number[] {
  const [levels, setLevels] = useState<number[]>(() => new Array(NUM_BARS).fill(0));
  const smoothedRef = useRef<number[]>(new Array(NUM_BARS).fill(0));
  const latestRawRef = useRef<number[]>(new Array(NUM_BARS).fill(0));

  useEffect(() => {
    if (!active) {
      smoothedRef.current = new Array(NUM_BARS).fill(0);
      setLevels(new Array(NUM_BARS).fill(0));
      return;
    }

    const unlisten = listen<number[]>("audio-levels", (event) => {
      latestRawRef.current = event.payload;
    });

    let running = true;
    const animate = () => {
      if (!running) return;
      const raw = latestRawRef.current;
      const smoothed = smoothedRef.current;

      for (let i = 0; i < NUM_BARS; i++) {
        const target = Math.min((raw[i] || 0) * 4, 1);
        const speed = target > smoothed[i] ? 0.4 : 0.15;
        smoothed[i] += (target - smoothed[i]) * speed;
      }

      setLevels([...smoothed]);
      requestAnimationFrame(animate);
    };

    const rafId = requestAnimationFrame(animate);

    return () => {
      running = false;
      cancelAnimationFrame(rafId);
      unlisten.then((fn) => fn());
    };
  }, [active]);

  return levels;
}
