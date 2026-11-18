import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { createMockBackend } from "../../test/mockBackend";
import { SettingsView } from "./SettingsView";
import { DEFAULT_SETTINGS } from "./defaults";
import { useSettings } from "./settingsStore";
import { useStartup } from "./startup";
import { resolveTheme, useApplyTheme } from "./theme";
import { settingsProblem } from "./validate";

let calls: { cmd: string; args: Record<string, unknown> }[] = [];

beforeEach(() => {
  calls = [];
  useSettings.setState({ settings: DEFAULT_SETTINGS, loaded: false, error: null });
  const backend = createMockBackend();
  mockIPC((cmd, payload) => {
    calls.push({
      cmd,
      args: (payload as { args?: Record<string, unknown> } | undefined)?.args ?? {},
    });
    return backend(cmd, payload);
  });
});

afterEach(() => {
  clearMocks();
});

function show(onClose = vi.fn(), onShowTrustedHosts = vi.fn()) {
  render(<SettingsView onClose={onClose} onShowTrustedHosts={onShowTrustedHosts} />);
  return { onClose, onShowTrustedHosts };
}

describe("SettingsView", () => {
  it("saves the startup update check setting", async () => {
    const { onClose } = show();

    await userEvent.click(screen.getByLabelText("Check for updates at startup"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(useSettings.getState().settings.checkForUpdates).toBe(false);
  });

  it("saves the reconnect at startup setting", async () => {
    const { onClose } = show();

    await userEvent.click(screen.getByLabelText("Reconnect the last active connection at startup"));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(useSettings.getState().settings.reconnectOnStart).toBe(false);
  });

  it("checks for updates on request", async () => {
    show();

    await userEvent.click(screen.getByRole("button", { name: "Check now" }));

    await waitFor(() => {
      expect(calls.some((c) => c.cmd === "update_check")).toBe(true);
    });
  });

  it("saves the edited settings and adopts them", async () => {
    const { onClose } = show();

    await userEvent.selectOptions(screen.getByLabelText("Theme"), "light");
    await userEvent.clear(screen.getByLabelText("Default page size"));
    await userEvent.type(screen.getByLabelText("Default page size"), "500");
    await userEvent.clear(screen.getByLabelText("NULL shown as"));
    await userEvent.type(screen.getByLabelText("NULL shown as"), "∅");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(useSettings.getState().settings).toMatchObject({
      theme: "light",
      defaultPageSize: 500,
      nullText: "∅",
    });
    expect(calls.find((c) => c.cmd === "settings_set")?.args.settings).toMatchObject({
      theme: "light",
    });
  });

  it("explains an out-of-range value and does not save it", async () => {
    show();

    await userEvent.clear(screen.getByLabelText("Editor font size"));
    await userEvent.type(screen.getByLabelText("Editor font size"), "99");

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Editor font size must be a whole number between 9 and 32.",
    );
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("opens the trusted hosts list and the logs folder", async () => {
    const { onShowTrustedHosts } = show();

    await userEvent.click(screen.getByRole("button", { name: "Trusted SSH hosts…" }));
    await userEvent.click(screen.getByRole("button", { name: "Open logs folder" }));

    expect(onShowTrustedHosts).toHaveBeenCalled();
    await waitFor(() => {
      expect(calls.some((c) => c.cmd === "logs_dir")).toBe(true);
    });
  });

  it("shows how long the last start took", async () => {
    await useStartup.getState().report();
    show();

    expect(screen.getByText("Started in 42 ms")).toBeInTheDocument();
  });

  it("loads the backend's settings", async () => {
    await useSettings.getState().save({ ...DEFAULT_SETTINGS, historyCap: 5 });
    useSettings.setState({ settings: DEFAULT_SETTINGS });

    await useSettings.getState().load();

    expect(useSettings.getState()).toMatchObject({ loaded: true, settings: { historyCap: 5 } });
  });
});

describe("settings helpers", () => {
  it("resolves the system theme from the system preference", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("puts the chosen theme on the document", async () => {
    function Themed() {
      useApplyTheme();
      return null;
    }
    render(<Themed />);
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, theme: "light" } });

    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("light");
    });
    expect(document.documentElement.style.colorScheme).toBe("light");
  });

  it("checks every bound", () => {
    expect(settingsProblem(DEFAULT_SETTINGS)).toBeNull();
    expect(settingsProblem({ ...DEFAULT_SETTINGS, historyCap: -1 })).toMatch(/History size/);
    expect(settingsProblem({ ...DEFAULT_SETTINGS, defaultRowLimit: 1.5 })).toMatch(/row limit/);
    expect(settingsProblem({ ...DEFAULT_SETTINGS, editorFontFamily: " " })).toMatch(/font/);
    expect(settingsProblem({ ...DEFAULT_SETTINGS, nullText: "x".repeat(21) })).toMatch(/NULL/);
  });
});
