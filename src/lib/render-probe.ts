/**
 * Lightweight render-timing probe for measuring first-render latency of
 * individual UI sections.  Each probe records `performance.mark()` entries
 * and exposes a snapshot via `window.__RENDER_PROBES__` for E2E collection.
 *
 * Usage:
 *   const probe = useMemo(() => new RenderProbe('home'), []);
 *   useEffect(() => { if (status) probe.hit('status'); }, [status]);
 */

export interface RenderProbeSnapshot {
  /** Milliseconds from probe creation to each first-hit. */
  [label: string]: number;
}

declare global {
  interface Window {
    __RENDER_PROBES__?: Record<string, RenderProbeSnapshot>;
  }
}

export class RenderProbe {
  readonly page: string;
  private readonly epoch: number;
  private readonly marks: Record<string, number> = {};

  constructor(page: string) {
    this.page = page;
    this.epoch = performance.now();
    performance.mark(`${page}:mount`);
  }

  /** Record the first render of a named section. Subsequent calls with the same label are no-ops. */
  hit(label: string): void {
    if (this.marks[label] != null) return;
    const elapsed = Math.round(performance.now() - this.epoch);
    this.marks[label] = elapsed;
    performance.mark(`${this.page}:${label}`);
    this.flush();
  }

  /** Alias for `hit('settled')` — marks the moment all data is loaded. */
  settled(): void {
    this.hit("settled");
  }

  /** Return a copy of collected marks. */
  snapshot(): RenderProbeSnapshot {
    return { ...this.marks };
  }

  /** Write current marks to `window.__RENDER_PROBES__` for external readers. */
  private flush(): void {
    if (typeof window === "undefined") return;
    window.__RENDER_PROBES__ ??= {};
    window.__RENDER_PROBES__[this.page] = { ...this.marks };
  }
}
