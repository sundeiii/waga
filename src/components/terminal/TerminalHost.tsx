import { useEffect, useRef } from "react";
import { terminalEngine } from "@/services/terminalEngine";

/** Stable DOM container that holds every xterm instance. Never unmounted. */
export function TerminalHost({ visible }: { visible: boolean }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current) return;
    terminalEngine.attachHost(ref.current);
    let frame = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => terminalEngine.fit());
    });
    ro.observe(ref.current);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(frame);
    };
  }, []);

  useEffect(() => {
    if (visible)
      requestAnimationFrame(() => {
        terminalEngine.fit();
        terminalEngine.focus();
      });
  }, [visible]);

  return <div ref={ref} data-terminal-host="" className="absolute inset-0" />;
}
