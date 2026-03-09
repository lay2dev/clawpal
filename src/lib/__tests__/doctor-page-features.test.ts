import { describe, expect, test } from "bun:test";

import { resolveDoctorPageFeatureVisibility } from "../doctor-page-features";

describe("resolveDoctorPageFeatureVisibility", () => {
  test("shows only the formal Rescue Bot surface", () => {
    expect(resolveDoctorPageFeatureVisibility()).toEqual({
      showDoctorClaw: false,
      showOtherAgentHelp: false,
      showRescueBot: true,
    });
  });
});
