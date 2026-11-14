import { describeConnectError, hostKeyFrom, stageLabel } from "./hostKey";

const key = { host: "bastion", port: 22, algorithm: "ssh-ed25519", fingerprint: "SHA256:x" };

describe("host key helpers", () => {
  it("reads the key to trust from the error detail", () => {
    expect(
      hostKeyFrom({
        code: "ssh_host_key_unknown",
        message: "",
        detail: { ...key, stage: "host_key" },
      }),
    ).toEqual(key);
  });

  it("ignores errors without a complete key", () => {
    expect(hostKeyFrom({ code: "ssh_host_key_unknown", message: "" })).toBeNull();
    expect(hostKeyFrom({ code: "x", message: "", detail: { ...key, port: "22" } })).toBeNull();
  });

  it("labels failures by stage", () => {
    const error = { code: "ssh_auth", message: "rejected", detail: { stage: "ssh_auth" } };

    expect(stageLabel(error)).toBe("SSH authentication failed");
    expect(describeConnectError(error)).toBe("SSH authentication failed: rejected");
    expect(describeConnectError({ code: "x", message: "plain" })).toBe("plain");
    expect(stageLabel({ code: "x", message: "", detail: { stage: "unknown_stage" } })).toBeNull();
  });
});
