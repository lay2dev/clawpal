import type { DoctorReport } from "./types";

export interface DoctorState {
  doctor: DoctorReport | null;
  message: string;
}

export const initialDoctorState: DoctorState = {
  doctor: null,
  message: "",
};

export type DoctorAction =
  | { type: "setDoctor"; doctor: DoctorReport }
  | { type: "setMessage"; message: string };

export function doctorReducer(state: DoctorState, action: DoctorAction): DoctorState {
  switch (action.type) {
    case "setDoctor":
      return { ...state, doctor: action.doctor };
    case "setMessage":
      return { ...state, message: action.message };
    default:
      return state;
  }
}
