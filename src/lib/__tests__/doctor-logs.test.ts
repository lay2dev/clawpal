import { describe, expect, test } from "bun:test";

import { getDoctorLogTransport } from "@/lib/doctor-logs";

describe("getDoctorLogTransport", () => {
  test("keeps clawpal logs local even when browsing a remote instance", () => {
    expect(getDoctorLogTransport("clawpal")).toBe("local");
  });

  test("keeps gateway and helper logs instance-scoped", () => {
    expect(getDoctorLogTransport("gateway")).toBe("instance");
    expect(getDoctorLogTransport("helper")).toBe("instance");
  });
});
