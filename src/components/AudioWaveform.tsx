import { useRef, useEffect } from "react";

interface AudioWaveformProps {
  levels: number[];
}

const BAR_WIDTH = 3;
const BAR_GAP = 2.5;
const MIN_BAR_HEIGHT = 2;
const MAX_BAR_HEIGHT = 40;
const CANVAS_HEIGHT = 48;

export function AudioWaveform({ levels }: AudioWaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const levelsRef = useRef(levels);
  levelsRef.current = levels;

  const numBars = levels.length;
  const canvasWidth = numBars * (BAR_WIDTH + BAR_GAP) - BAR_GAP;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = canvasWidth * dpr;
    canvas.height = CANVAS_HEIGHT * dpr;
    ctx.scale(dpr, dpr);

    let animId: number;
    const draw = () => {
      ctx.clearRect(0, 0, canvasWidth, CANVAS_HEIGHT);
      const currentLevels = levelsRef.current;
      const centerY = CANVAS_HEIGHT / 2;

      for (let i = 0; i < currentLevels.length; i++) {
        const level = currentLevels[i];
        const barHeight = MIN_BAR_HEIGHT + level * (MAX_BAR_HEIGHT - MIN_BAR_HEIGHT);
        const x = i * (BAR_WIDTH + BAR_GAP);
        const y = centerY - barHeight / 2;

        const alpha = 0.5 + level * 0.5;
        ctx.fillStyle = `rgba(255, 255, 255, ${alpha})`;

        const radius = BAR_WIDTH / 2;
        ctx.beginPath();
        ctx.roundRect(x, y, BAR_WIDTH, barHeight, radius);
        ctx.fill();
      }

      animId = requestAnimationFrame(draw);
    };

    animId = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animId);
  }, [canvasWidth]);

  return (
    <canvas
      ref={canvasRef}
      style={{
        width: canvasWidth,
        height: CANVAS_HEIGHT,
      }}
    />
  );
}
