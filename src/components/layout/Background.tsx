import { useEffect } from "react";

/** Slow ambient gradient orbs. Paused while the window is hidden. */
export function Background() {
  useEffect(() => {
    const onVis = () => document.documentElement.classList.toggle("app-paused", document.hidden);
    document.addEventListener("visibilitychange", onVis);
    onVis();
    return () => document.removeEventListener("visibilitychange", onVis);
  }, []);
  return (
    <div className="ambient" aria-hidden="true">
      <div className="app-tint" />
      <div className="orb a" />
      <div className="orb b" />
      <div className="orb c" />
      <div className="grain" />
    </div>
  );
}
