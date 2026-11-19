import type { Choice } from "@/bindings";

/// One answer of a FOMOD installer: the step-positioned group and the option chosen in it. The backend receives
/// these with the archive.
export type FomodChoice = Choice;
