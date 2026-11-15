import type { SshConfigEntry } from "../../ipc";
import { settingsFromConfig } from "./sshConfig";

const entry = (alias: string, more: Partial<SshConfigEntry> = {}): SshConfigEntry => ({
  alias,
  hostName: null,
  user: null,
  port: null,
  identityFile: null,
  proxyJump: null,
  ...more,
});

describe("settingsFromConfig", () => {
  it("fills the server, and the jump host from another alias", () => {
    const entries = [
      entry("bastion", { hostName: "b.example.com", user: "ops", port: 2222 }),
      entry("db", {
        hostName: "db.internal",
        user: "admin",
        identityFile: "/home/me/.ssh/id",
        proxyJump: "bastion",
      }),
    ];

    expect(settingsFromConfig("db", entries)).toEqual({
      server: {
        host: "db.internal",
        port: 22,
        user: "admin",
        auth: "key_file",
        keyFile: "/home/me/.ssh/id",
      },
      jump: { host: "b.example.com", port: 2222, user: "ops", auth: "agent", keyFile: null },
    });
  });

  it("reads a jump written as user@host:port, and uses the alias as the host name", () => {
    const settings = settingsFromConfig("x", [entry("x", { proxyJump: "me@gw.example.com:2200" })]);

    expect(settings?.server.host).toBe("x");
    expect(settings?.jump).toEqual({
      host: "gw.example.com",
      port: 2200,
      user: "me",
      auth: "agent",
      keyFile: null,
    });
  });

  it("knows nothing of an alias that is not there", () => {
    expect(settingsFromConfig("missing", [])).toBeNull();
  });
});
