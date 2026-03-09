export type DoctorLogSource = "clawpal" | "gateway" | "helper";

export type DoctorLogTransport = "local" | "instance";

export function getDoctorLogTransport(source: DoctorLogSource): DoctorLogTransport {
  return source === "clawpal" ? "local" : "instance";
}
