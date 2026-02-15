export function PulseAnimation() {
  return (
    <div className="relative flex items-center justify-center w-3 h-3">
      <div className="absolute w-3 h-3 rounded-full bg-red-500 animate-ping opacity-75" />
      <div className="w-2.5 h-2.5 rounded-full bg-red-500" />
    </div>
  );
}
