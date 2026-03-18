import { describe, expect, test } from "bun:test";

import type { RemoteDoctorRepairResult } from "../types";
import { resolveDoctorPageFeatureVisibility } from "../doctor-page-features";

describe("resolveDoctorPageFeatureVisibility", () => {
  test("shows only the formal Rescue Bot surface", () => {
    expect(resolveDoctorPageFeatureVisibility()).toEqual({
      showDoctorClaw: false,
      showOtherAgentHelp: false,
      showRescueBot: true,
    });
  });

  test("accepts remote doctor repair result shape", () => {
    const result: RemoteDoctorRepairResult = {
      mode: "remoteDoctor",
      status: "completed",
      round: 3,
      phase: "reporting_detect",
      lastPlanKind: "detect",
      latestDiagnosisHealthy: true,
      lastCommand: ["openclaw", "doctor", "--json"],
      sessionId: "session-1",
      message: "Remote Doctor repair completed.",
    };

    expect(result.mode).toBe("remoteDoctor");
    expect(result.latestDiagnosisHealthy).toBe(true);
  });
});
