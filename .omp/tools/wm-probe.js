// wm_probe — model-callable omp custom tool.
//
// Wraps scripts/wm-probe.py so the agent can inspect REAL on-screen window
// geometry from the macOS window server while debugging Rift's tiling. It reads
// ground truth (Quartz CGWindowList), detects tile overlaps, and — with
// rift=true — diffs Rift's intended frames (`rift-cli query windows`, joined on
// window_server_id) against what actually landed on screen.
//
// Discovered automatically as a native OMP tool from <repo>/.omp/tools/.
// See omp://custom-tools.md for the factory/execute contract.

/** @type {import("@oh-my-pi/pi-coding-agent").CustomToolFactory} */
const factory = (pi) => {
  const { zod: z } = pi;

  return {
    name: "wm_probe",
    label: "WM Probe",
    description:
      "Inspect REAL on-screen window geometry from the macOS window server to debug Rift's " +
      "tiling. Lists actual per-window bounds, flags overlapping tiles, and (when rift=true) " +
      "diffs Rift's intended frames against what actually landed on screen (drift). Needs a " +
      "real GUI login session; returns structured details plus a text summary.",
    parameters: z.object({
      apps: z
        .array(z.string())
        .optional()
        .describe("Only include these owner apps (case-insensitive), e.g. ['Emacs','alacritty']."),
      rift: z
        .boolean()
        .optional()
        .default(false)
        .describe(
          "Cross-check against `rift-cli query windows`: attaches Rift's intended frame per " +
            "window (joined on window_server_id) and reports drift for managed tiles.",
        ),
      riftCli: z
        .string()
        .optional()
        .describe("Path to rift-cli. Default: ./result/bin/rift-cli, then $PATH."),
      minWidth: z
        .number()
        .optional()
        .describe("Ignore windows narrower than this in px (default 200)."),
      overlapPx: z
        .number()
        .optional()
        .describe("Report overlaps whose intersection area exceeds this in px^2 (default 25)."),
      tolerance: z
        .number()
        .optional()
        .describe("Drift threshold in px for the rift cross-check (default 2)."),
    }),

    async execute(_toolCallId, params, onUpdate, _ctx, signal) {
      const args = ["run", "--quiet", "scripts/wm-probe.py", "--json"];
      if (params.apps?.length) args.push("--app", ...params.apps);
      if (params.rift) args.push("--rift");
      if (params.riftCli) args.push(params.riftCli); // consumed by --rift's optional value
      if (params.minWidth != null) args.push("--min-width", String(params.minWidth));
      if (params.overlapPx != null) args.push("--overlap-px", String(params.overlapPx));
      if (params.tolerance != null) args.push("--tolerance", String(params.tolerance));

      onUpdate?.({ content: [{ type: "text", text: "Reading window-server geometry..." }] });

      const res = await pi.exec("uv", args, { signal, cwd: pi.cwd });
      if (res.killed) throw new Error("wm_probe cancelled");

      // The probe exits nonzero when it FINDS an anomaly (overlap/drift). That is
      // a successful run, not a tool failure — parse stdout regardless of code.
      let data;
      try {
        data = JSON.parse(res.stdout);
      } catch {
        const detail = (res.stderr || res.stdout || "").trim();
        throw new Error(
          `wm_probe could not parse probe output (exit ${res.code}). ` +
            (detail ? `Output: ${detail}` : "No output — is this a GUI login session?"),
        );
      }

      const overlaps = data.overlaps ?? [];
      const drift = data.drift ?? [];
      const windows = data.windows ?? [];
      const joined = windows.filter((w) => w.intent).length;

      const lines = [];
      lines.push(`${windows.length} window(s) on screen` + (params.rift ? `, ${joined} joined to Rift intent` : ""));
      if (params.rift) {
        lines.push(
          drift.length === 0
            ? "drift: none — every managed tile landed where Rift intended"
            : `drift: ${drift.length} tile(s) off > ${data.drift_tolerance_px}px:`,
        );
        for (const d of drift) {
          lines.push(
            `  ${d.owner}(${d.num}) Δx=${d.delta.x.toFixed(0)} Δy=${d.delta.y.toFixed(0)} ` +
              `Δw=${d.delta.w.toFixed(0)} Δh=${d.delta.h.toFixed(0)}`,
          );
        }
      }
      lines.push(
        overlaps.length === 0
          ? "overlaps: none"
          : `overlaps: ${overlaps.length} pair(s):`,
      );
      for (const o of overlaps) {
        lines.push(`  ${o.a_owner}(${o.a}) <> ${o.b_owner}(${o.b}): ${o.area_px}px^2`);
      }

      const anomalies = overlaps.length + drift.length;
      lines.unshift(
        anomalies === 0 ? "OK — no overlaps or drift." : `FOUND ${anomalies} anomaly/anomalies.`,
      );

      return {
        content: [{ type: "text", text: lines.join("\n") }],
        details: {
          ok: anomalies === 0,
          windowCount: windows.length,
          joined,
          overlaps,
          drift,
          driftTolerancePx: data.drift_tolerance_px,
          windows,
        },
      };
    },
  };
};

export default factory;
