import { Component, type ErrorInfo, type ReactNode } from "react";
import { TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/Button";

/** Keeps one crashing view from blanking the whole window. */
export class ErrorBoundary extends Component<{ children: ReactNode; resetKey?: unknown }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("UI error:", error, info.componentStack);
  }

  componentDidUpdate(prev: { resetKey?: unknown }) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) this.setState({ error: null });
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="grid h-full place-items-center p-6">
        <div className="glass max-w-md rounded-2xl p-6 text-center">
          <TriangleAlert className="mx-auto size-6 text-warning" />
          <h2 className="mt-3 text-[15px] font-semibold">This view hit an error</h2>
          <p className="selectable mt-1.5 break-words font-mono text-[12px] text-muted">{this.state.error.message}</p>
          <p className="mt-2 text-[12px] text-faint">Your sessions are still running.</p>
          <Button className="mt-4" variant="primary" onClick={() => this.setState({ error: null })}>
            Try again
          </Button>
        </div>
      </div>
    );
  }
}
