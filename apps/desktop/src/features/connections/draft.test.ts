import { mysqlDriver, postgresDriver } from "../../test/fixtures";
import { draftProblems, emptyDraft, emptySsh, emptySshHost, secretsFor } from "./draft";

describe("secretsFor", () => {
  it("sends only changed secrets: values set, null clears, keep is absent", () => {
    expect(secretsFor({})).toBeUndefined();
    expect(secretsFor({ dbPassword: { kind: "keep" } })).toBeUndefined();
    expect(
      secretsFor({
        dbPassword: { kind: "set", value: "p" },
        sshKeyPassphrase: { kind: "clear" },
        jumpPassword: { kind: "keep" },
      }),
    ).toEqual({ dbPassword: "p", sshKeyPassphrase: null });
  });
});

describe("SSH draft checks", () => {
  it("require host, user and a key file for key authentication, also on the jump host", () => {
    const draft = {
      ...emptyDraft(postgresDriver),
      name: "x",
      ssh: { server: { ...emptySshHost(), auth: "key_file" as const }, jump: emptySshHost() },
    };

    expect(draftProblems(draft, postgresDriver)).toEqual([
      "SSH: enter a host",
      "SSH: enter a user",
      "SSH: choose a key file",
      "Jump host: enter a host",
      "Jump host: enter a user",
    ]);
  });

  it("start with the agent on port 22 and no jump host", () => {
    expect(emptySsh()).toEqual({
      server: { host: "", port: 22, user: "", auth: "agent", keyFile: null },
      jump: null,
    });
    expect(emptyDraft(mysqlDriver).ssh).toBeNull();
  });
});
